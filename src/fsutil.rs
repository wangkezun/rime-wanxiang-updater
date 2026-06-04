//! File replacement that tolerates a destination being memory-mapped by another
//! process.
//!
//! While Rime is running it keeps scheme/dict/gram files memory-mapped. On
//! Windows, overwriting (truncating) such a file in place fails with
//! `ERROR_USER_MAPPED_FILE` (os error 1224). A *rename*, however, is allowed: it
//! leaves the mapped bytes untouched and only relinks the name. So instead of
//! writing over the target we write a fresh temp file beside it, move any
//! existing target out of the way, and rename the temp into place.

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Prefix for the fresh temp file we stage new content in.
const TMP_PREFIX: &str = ".wxupd-tmp-";
/// Prefix for an old target moved aside because it could not be deleted in place.
const OLD_PREFIX: &str = ".wxupd-old-";

fn unique_suffix() -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), nanos, n)
}

/// Write all bytes from `src` to `dst`, replacing any existing file even when it
/// is held memory-mapped by a running process (e.g. the Rime engine on Windows).
pub fn replace_with_reader<R: Read>(dst: &Path, src: &mut R) -> Result<()> {
    let parent = dst.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!("{}{}", TMP_PREFIX, unique_suffix()));

    let result = (|| -> Result<()> {
        let mut f = File::create(&tmp).with_context(|| format!("create temp {}", tmp.display()))?;
        io::copy(src, &mut f).with_context(|| format!("write temp {}", tmp.display()))?;
        f.sync_all().ok();
        drop(f);
        swap_into_place(&tmp, dst)
    })();

    if result.is_err() {
        // Never leave our staging file behind on the failure path.
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Copy the file at `src` onto `dst` with the same mmap-tolerant replacement.
pub fn replace_copy(src: &Path, dst: &Path) -> Result<()> {
    let mut f = File::open(src).with_context(|| format!("open {}", src.display()))?;
    replace_with_reader(dst, &mut f)
}

/// Rename `tmp` onto `dst`. If `dst` already exists, move it aside first so the
/// final rename targets a free name — renaming over an existing mapped file would
/// have to delete it, which is exactly what fails with os error 1224.
fn swap_into_place(tmp: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        let parent = dst.parent().unwrap_or_else(|| Path::new("."));
        let aside = parent.join(format!("{}{}", OLD_PREFIX, unique_suffix()));
        fs::rename(dst, &aside).with_context(|| format!("move aside {}", dst.display()))?;
        // Best-effort: while the old file is still mapped this delete fails with
        // os error 1224. It then lingers as an `.wxupd-old-*` sidecar that the
        // next run's `purge_stale` sweeps up once the mapping is gone.
        let _ = fs::remove_file(&aside);
    }
    fs::rename(tmp, dst).with_context(|| format!("rename {} -> {}", tmp.display(), dst.display()))
}

/// Best-effort cleanup of staging/sidecar files left by interrupted or
/// mmap-blocked replacements anywhere under `root`.
pub fn purge_stale(root: &Path) {
    let _ = purge_inner(root);
}

fn purge_inner(dir: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let _ = purge_inner(&path);
        } else {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(TMP_PREFIX) || name.starts_with(OLD_PREFIX) {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn writes_new_file_and_creates_parent() {
        let d = TempDir::new().unwrap();
        let dst = d.path().join("sub/dir/out.bin");
        replace_with_reader(&dst, &mut &b"hello"[..]).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"hello");
        // No staging or sidecar files left behind.
        assert!(no_wxupd_files(d.path()));
    }

    #[test]
    fn overwrites_existing_file() {
        let d = TempDir::new().unwrap();
        let dst = d.path().join("out.bin");
        fs::write(&dst, b"old contents").unwrap();
        replace_with_reader(&dst, &mut &b"new"[..]).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new");
        assert!(no_wxupd_files(d.path()));
    }

    #[test]
    fn replace_copy_roundtrips() {
        let d = TempDir::new().unwrap();
        let src = d.path().join("src.bin");
        fs::write(&src, b"\x00\x01payload").unwrap();
        let dst = d.path().join("nested/dst.bin");
        replace_copy(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"\x00\x01payload");
    }

    #[test]
    fn failed_write_leaves_no_temp_and_keeps_original() {
        struct Boom;
        impl Read for Boom {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }
        let d = TempDir::new().unwrap();
        let dst = d.path().join("out.bin");
        fs::write(&dst, b"original").unwrap();
        let err = replace_with_reader(&dst, &mut Boom);
        assert!(err.is_err());
        // Original untouched, no staging file left behind.
        assert_eq!(fs::read(&dst).unwrap(), b"original");
        assert!(no_wxupd_files(d.path()));
    }

    #[test]
    fn purge_removes_sidecars_recursively() {
        let d = TempDir::new().unwrap();
        let sub = d.path().join("cn_dicts");
        fs::create_dir_all(&sub).unwrap();
        fs::write(d.path().join(format!("{}1", TMP_PREFIX)), b"x").unwrap();
        fs::write(sub.join(format!("{}2", OLD_PREFIX)), b"y").unwrap();
        let keep = sub.join("real.dict.yaml");
        fs::write(&keep, b"keep").unwrap();
        purge_stale(d.path());
        assert!(no_wxupd_files(d.path()));
        assert!(keep.exists());
    }

    fn no_wxupd_files(dir: &Path) -> bool {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let ft = entry.file_type().unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if ft.is_dir() {
                if !no_wxupd_files(&entry.path()) {
                    return false;
                }
            } else if name.starts_with(TMP_PREFIX) || name.starts_with(OLD_PREFIX) {
                return false;
            }
        }
        true
    }
}
