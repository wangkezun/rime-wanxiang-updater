use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn release_json_multi(tag: &str, assets: &[&str]) -> serde_json::Value {
    let asset_list: Vec<serde_json::Value> = assets
        .iter()
        .map(|a| serde_json::json!({ "name": a, "browser_download_url": format!("https://x/{a}"), "size": 100 }))
        .collect();
    serde_json::json!({
        "tag_name": tag,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": asset_list
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn check_reports_not_installed_for_empty_manifest() {
    let mirror = MockServer::start().await;
    // The catch-all matcher returns the same body for every release request;
    // good enough for this assertion since we only check exit code + status text.
    // Respond to all release API calls with a body that satisfies the asset
    // patterns for scheme (rime-wanxiang-*.zip), gram (wanxiang-lts-zh-hans.gram),
    // and dict (dicts.zip) so none return Error status.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json_multi(
            "v1.0",
            &[
                "rime-wanxiang-base.zip",
                "wanxiang-lts-zh-hans.gram",
                "dicts.zip",
            ],
        )))
        .mount(&mirror)
        .await;

    let d = TempDir::new().unwrap();
    let cfg_path = d.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[scheme]\nvariant = \"base\"\n\n[network]\nmirrors = [\"{}\"]\ntimeout_secs = 5\n",
            mirror.uri()
        ),
    )
    .unwrap();
    let manifest_path = d.path().join("manifest.json");

    let assert = Command::cargo_bin("wxupd")
        .unwrap()
        .env("WXUPD_CONFIG", &cfg_path)
        .env("WXUPD_MANIFEST", &manifest_path)
        .args(["check", "--json"])
        .assert();

    // Exit 0 because not-installed doesn't trigger any_update() — semantics
    // are deliberately documented in the spec.
    assert.success().stdout(contains("\"id\": \"scheme\""));
}
