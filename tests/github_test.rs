use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wxupd::github::{rewrite_asset_url, Github};

#[tokio::test]
async fn fetches_latest_release() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v9.9",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{
            "name": "wanxiang-base-v9.9.zip",
            "browser_download_url": "https://example.com/a.zip",
            "size": 1234
        }]
    });
    Mock::given(method("GET"))
        .and(path("/repos/amzxyz/rime_wanxiang/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let gh = Github::new(5, None).unwrap().with_api_base(server.uri());
    let rel = gh.latest_release("amzxyz/rime_wanxiang").await.unwrap();
    assert_eq!(rel.tag_name, "v9.9");
    assert_eq!(rel.assets.len(), 1);
}

#[tokio::test]
async fn fetches_release_by_tag() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "LTS",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{
            "name": "wanxiang-lts-zh-hans.gram",
            "browser_download_url": "https://example.com/a.gram",
            "size": 99
        }]
    });
    Mock::given(method("GET"))
        .and(path("/repos/amzxyz/RIME-LMDG/releases/tags/LTS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let gh = Github::new(5, None).unwrap().with_api_base(server.uri());
    let rel = gh.release_by_tag("amzxyz/RIME-LMDG", "LTS").await.unwrap();
    assert_eq!(rel.tag_name, "LTS");
    assert_eq!(rel.assets[0].name, "wanxiang-lts-zh-hans.gram");
}

#[test]
fn rewrites_asset_url_with_mirrors() {
    let url = "https://github.com/amzxyz/rime_wanxiang/releases/download/v1/rime-wanxiang-base.zip";
    let mirrors = vec![
        "https://ghproxy.com".to_string(),
        "https://ghfast.top".to_string(),
    ];
    let urls = rewrite_asset_url(url, &mirrors);
    assert_eq!(
        urls,
        vec![
            "https://ghproxy.com/https://github.com/amzxyz/rime_wanxiang/releases/download/v1/rime-wanxiang-base.zip",
            "https://ghfast.top/https://github.com/amzxyz/rime_wanxiang/releases/download/v1/rime-wanxiang-base.zip",
            url
        ]
    );
}

#[tokio::test]
async fn parses_asset_digest_field() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v1",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{
            "name": "a.zip",
            "browser_download_url": "https://example.com/a.zip",
            "size": 10,
            "digest": "sha256:abc123"
        }]
    });
    Mock::given(method("GET"))
        .and(path("/repos/amzxyz/rime_wanxiang/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;
    let gh = Github::new(5, None).unwrap().with_api_base(server.uri());
    let rel = gh.latest_release("amzxyz/rime_wanxiang").await.unwrap();
    assert_eq!(rel.assets[0].digest.as_deref(), Some("sha256:abc123"));
    assert_eq!(rel.assets[0].sha256(), Some("abc123"));
}
