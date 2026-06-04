use std::fs;
use tempfile::TempDir;
use wxupd::resource::dict::DictResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

#[tokio::test]
async fn install_copies_yaml() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("cn_en_mix.dict.yaml");
    fs::write(&src, b"# yaml").unwrap();
    let rime = d.path().join("rime");
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let r = DictResource.install(&src, &rime, &safe).await.unwrap();
    assert_eq!(r.files_written, vec![std::path::PathBuf::from("cn_en_mix.dict.yaml")]);
    assert_eq!(fs::read(rime.join("cn_en_mix.dict.yaml")).unwrap(), b"# yaml");
}
