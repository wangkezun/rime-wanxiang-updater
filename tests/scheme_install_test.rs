use std::fs;
use std::io::Write;
use tempfile::TempDir;
use wxupd::resource::scheme::SchemeResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

fn build_fake_zip(out: &std::path::Path) {
    let f = fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
    z.start_file("wanxiang.schema.yaml", opts).unwrap();
    z.write_all(b"schema: hi").unwrap();
    z.start_file("wanxiang.custom.yaml", opts).unwrap();
    z.write_all(b"keep me").unwrap();
    z.start_file("lua/sub.lua", opts).unwrap();
    z.write_all(b"-- lua").unwrap();
    z.finish().unwrap();
}

#[tokio::test]
async fn install_writes_files_and_skips_safelist() {
    let d = TempDir::new().unwrap();
    let zip = d.path().join("scheme.zip");
    build_fake_zip(&zip);
    let rime = d.path().join("rime");
    // Pre-existing user-customised file we must not clobber.
    fs::create_dir_all(&rime).unwrap();
    fs::write(rime.join("wanxiang.custom.yaml"), b"USER VERSION").unwrap();

    let safe = SafeList::defaults_plus(&[]).unwrap();
    let res = SchemeResource;
    let report = res.install(&zip, &rime, &safe).await.unwrap();

    assert!(report.files_written.iter().any(|p| p == std::path::Path::new("wanxiang.schema.yaml")));
    assert!(report.files_written.iter().any(|p| p == std::path::Path::new("lua/sub.lua")));
    assert!(report.files_skipped.iter().any(|p| p == std::path::Path::new("wanxiang.custom.yaml")));
    assert_eq!(fs::read(rime.join("wanxiang.custom.yaml")).unwrap(), b"USER VERSION");
    assert_eq!(fs::read(rime.join("wanxiang.schema.yaml")).unwrap(), b"schema: hi");
}
