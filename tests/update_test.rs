use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn release_json(tag: &str, assets: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": assets.iter().map(|(n, u)| serde_json::json!({
            "name": n, "browser_download_url": u, "size": 100
        })).collect::<Vec<_>>()
    })
}

fn build_fake_zip(out: &std::path::Path) {
    let f = std::fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
    z.start_file("wanxiang.schema.yaml", opts).unwrap();
    z.write_all(b"v1").unwrap();
    z.finish().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn update_installs_scheme_and_writes_manifest() {
    let mirror = MockServer::start().await;
    // Build a fake zip we'll serve as the asset.
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("rime-wanxiang-base.zip");
    build_fake_zip(&zip_path);
    let zip_bytes = std::fs::read(&zip_path).unwrap();

    // Mock release endpoints (one per repo); also serve the asset bytes.
    let asset_url = format!("{}/dl/rime-wanxiang-base.zip", mirror.uri());
    Mock::given(method("GET"))
        .and(path_regex(r".*amzxyz/rime_wanxiang/releases/latest$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json(
            "v1",
            &[("rime-wanxiang-base.zip", &asset_url)],
        )))
        .mount(&mirror)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r".*amzxyz/RIME-LMDG/releases/latest$"))
        .respond_with(ResponseTemplate::new(500)) // gram release "missing" — surfaces as error
        .mount(&mirror)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/dl/rime-wanxiang-base\.zip$"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_bytes))
        .mount(&mirror)
        .await;

    let d = TempDir::new().unwrap();
    let cfg_path = d.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[scheme]\nvariant = \"base\"\n[paths]\nrime_user_dir = \"{}\"\n\
         [network]\nmirrors = [\"{}\"]\ntimeout_secs = 5\n[deploy]\nauto = false\n",
            d.path().join("rime").display(),
            mirror.uri()
        ),
    )
    .unwrap();
    let manifest_path = d.path().join("manifest.json");

    // We only ask for the scheme to side-step the failing gram mock.
    let assert = Command::cargo_bin("wxupd")
        .unwrap()
        .env("WXUPD_CONFIG", &cfg_path)
        .env("WXUPD_MANIFEST", &manifest_path)
        .env("WXUPD_CACHE", d.path().join("cache"))
        .env("WXUPD_DATA", d.path().join("data"))
        .args(["update", "scheme"])
        .assert();
    assert.success();

    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(m["resources"]["scheme"]["tag"], "v1");
    let installed = d.path().join("rime/wanxiang.schema.yaml");
    assert_eq!(std::fs::read(&installed).unwrap(), b"v1");
}

#[tokio::test(flavor = "multi_thread")]
async fn update_records_check_error_in_failures() {
    let mirror = MockServer::start().await;
    // All release endpoints return 500 → all resources end with Status::Error.
    Mock::given(method("GET"))
        .and(path_regex(r".*releases/latest$"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mirror)
        .await;

    let d = TempDir::new().unwrap();
    let cfg_path = d.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[scheme]\nvariant = \"base\"\n[paths]\nrime_user_dir = \"{}\"\n\
             [network]\nmirrors = [\"{}\"]\ntimeout_secs = 5\n[deploy]\nauto = false\n",
            d.path().join("rime").display(),
            mirror.uri()
        ),
    )
    .unwrap();
    let manifest_path = d.path().join("manifest.json");

    let assert = Command::cargo_bin("wxupd")
        .unwrap()
        .env("WXUPD_CONFIG", &cfg_path)
        .env("WXUPD_MANIFEST", &manifest_path)
        .env("WXUPD_CACHE", d.path().join("cache"))
        .env("WXUPD_DATA", d.path().join("data"))
        .args(["update"])
        .assert();
    // Exit 3 because failures are non-empty.
    assert
        .failure()
        .code(3)
        .stderr(predicate::str::contains("FAILED"));
}
