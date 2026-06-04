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
    assert_eq!(report.files_written, vec![std::path::PathBuf::from("wanxiang-lts-zh-hans.gram")]);
    assert!(report.files_skipped.is_empty());
    assert_eq!(fs::read(rime.join("wanxiang-lts-zh-hans.gram")).unwrap(), b"\x00\x01\x02gram bytes");
}
