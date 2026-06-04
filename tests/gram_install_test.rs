use std::fs;
use tempfile::TempDir;
use wxupd::resource::gram::GramResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

#[tokio::test]
async fn install_copies_single_file() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("dl.gram");
    fs::write(&src, b"\x00\x01\x02gram bytes").unwrap();
    let rime = d.path().join("rime");
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let report = GramResource.install(&src, &rime, &safe).await.unwrap();
    assert_eq!(
        report.files_written,
        vec![std::path::PathBuf::from("wanxiang-lts-zh-hans.gram")]
    );
    assert!(report.files_skipped.is_empty());
    assert_eq!(
        fs::read(rime.join("wanxiang-lts-zh-hans.gram")).unwrap(),
        b"\x00\x01\x02gram bytes"
    );
}

#[tokio::test]
async fn install_overwrites_existing_gram() {
    let d = TempDir::new().unwrap();
    let rime = d.path().join("rime");
    fs::create_dir_all(&rime).unwrap();
    // A stale .gram already on disk (as if from a prior version).
    fs::write(rime.join("wanxiang-lts-zh-hans.gram"), b"old gram").unwrap();

    let src = d.path().join("dl.gram");
    fs::write(&src, b"fresh gram").unwrap();
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let report = GramResource.install(&src, &rime, &safe).await.unwrap();

    assert_eq!(
        report.files_written,
        vec![std::path::PathBuf::from("wanxiang-lts-zh-hans.gram")]
    );
    assert_eq!(
        fs::read(rime.join("wanxiang-lts-zh-hans.gram")).unwrap(),
        b"fresh gram"
    );
    // No staging/sidecar leftovers in the Rime dir.
    for entry in fs::read_dir(&rime).unwrap() {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with(".wxupd-"), "unexpected leftover: {name}");
    }
}
