use chrono::Utc;
use std::collections::BTreeMap;
use tempfile::TempDir;
use wxupd::backup::write_tar_zst;
use wxupd::config::Config;
use wxupd::manifest::{HistoryEntry, Manifest, ResourceEntry};
use wxupd::ops::rollback::{run, RollbackArgs};

#[tokio::test]
async fn rolls_back_to_previous_and_removes_new_files() {
    let d = TempDir::new().unwrap();
    let rime = d.path().join("rime");
    std::fs::create_dir_all(&rime).unwrap();
    // Pretend v2 is currently installed: it added "v2-only.yaml" and modified "shared.yaml".
    std::fs::write(rime.join("v2-only.yaml"), b"V2 added me").unwrap();
    std::fs::write(rime.join("shared.yaml"), b"V2 contents").unwrap();

    // Backup tar.zst captures the v1 state of shared.yaml.
    let stash = d.path().join("stash"); std::fs::create_dir_all(&stash).unwrap();
    std::fs::write(stash.join("shared.yaml"), b"V1 contents").unwrap();
    let backup_path = d.path().join("backups/scheme/v1.tar.zst");
    write_tar_zst(&stash, &[std::path::PathBuf::from("shared.yaml")], &backup_path).unwrap();

    let mut resources = BTreeMap::new();
    resources.insert("scheme".into(), ResourceEntry {
        tag: "v2".into(),
        asset_name: "wanxiang-pinyin-v2.zip".into(),
        sha256: "x".into(),
        installed_at: Utc::now(),
        files_installed: vec!["v2-only.yaml".into(), "shared.yaml".into()],
        history: vec![HistoryEntry {
            tag: "v1".into(),
            asset_name: "wanxiang-pinyin-v1.zip".into(),
            sha256: "y".into(),
            backup: backup_path.clone(),
            installed_at: Utc::now(),
            files_installed: vec!["shared.yaml".into()],
        }],
    });
    let mut manifest = Manifest { schema_version: 1, resources };
    let manifest_path = d.path().join("manifest.json");
    manifest.save(&manifest_path).unwrap();

    let cfg = Config { deploy: wxupd::config::DeployCfg { auto: false }, ..Config::default() };
    let outcome = run(&cfg, &mut manifest, &manifest_path, &rime, RollbackArgs { only: vec!["scheme".into()], no_deploy: true }).await.unwrap();

    assert_eq!(outcome.rolled_back, vec![("scheme".into(), "v2".into(), "v1".into())]);
    assert!(!rime.join("v2-only.yaml").exists(), "v2-only.yaml should be deleted");
    assert_eq!(std::fs::read(rime.join("shared.yaml")).unwrap(), b"V1 contents");
    assert_eq!(manifest.resources["scheme"].tag, "v1");
    assert_eq!(manifest.resources["scheme"].history[0].tag, "v2");
}
