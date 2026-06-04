use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Pack `rel_paths` (relative to `root`) into a zstd-compressed tar at `out`.
/// Missing files are skipped silently — they may have been deleted between
/// the manifest write and the backup call.
pub fn write_tar_zst(root: &Path, rel_paths: &[PathBuf], out: &Path) -> Result<()> {
    if let Some(p) = out.parent() {
        fs::create_dir_all(p)?;
    }
    let file = File::create(out).with_context(|| format!("create {}", out.display()))?;
    let enc = zstd::Encoder::new(BufWriter::new(file), 3)?.auto_finish();
    let mut tar = tar::Builder::new(enc);
    for rel in rel_paths {
        let abs = root.join(rel);
        if !abs.exists() {
            continue;
        }
        tar.append_path_with_name(&abs, rel)
            .with_context(|| format!("append {}", rel.display()))?;
    }
    tar.finish()?;
    Ok(())
}

/// Extract `archive` over `root`, overwriting any colliding files.
pub fn extract_tar_zst(archive: &Path, root: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let dec = zstd::Decoder::new(BufReader::new(file))?;
    let mut tar = tar::Archive::new(dec);
    let mut written = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let rel = entry.path()?.to_path_buf();
        let dst = root.join(&rel);
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
        }
        entry.unpack(&dst)?;
        written.push(rel);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_two_files_with_subdir() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        fs::create_dir_all(src.path().join("sub")).unwrap();
        fs::write(src.path().join("a.yaml"), b"hello").unwrap();
        fs::write(src.path().join("sub/b.yaml"), b"world").unwrap();
        let archive = src.path().join("backup.tar.zst");
        let files = vec![PathBuf::from("a.yaml"), PathBuf::from("sub/b.yaml")];
        write_tar_zst(src.path(), &files, &archive).unwrap();

        let written = extract_tar_zst(&archive, dst.path()).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(fs::read(dst.path().join("a.yaml")).unwrap(), b"hello");
        assert_eq!(fs::read(dst.path().join("sub/b.yaml")).unwrap(), b"world");
    }

    #[test]
    fn missing_files_skipped_silently() {
        let src = TempDir::new().unwrap();
        let archive = src.path().join("backup.tar.zst");
        let files = vec![PathBuf::from("does-not-exist.yaml")];
        // Should NOT error.
        write_tar_zst(src.path(), &files, &archive).unwrap();
        let dst = TempDir::new().unwrap();
        let written = extract_tar_zst(&archive, dst.path()).unwrap();
        assert!(written.is_empty());
    }
}
