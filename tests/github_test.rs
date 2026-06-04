use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wxupd::github::Github;

#[tokio::test]
async fn picks_first_successful_mirror() {
    // Mirror server returns the release; api.github.com is unreachable in test
    // (we only point mirrors at the mock and never include the real base URL).
    let mirror = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v9.9",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{
            "name": "wanxiang-pinyin-v9.9.zip",
            "browser_download_url": "https://example.com/a.zip",
            "size": 1234
        }]
    });
    Mock::given(method("GET"))
        .and(path("/https://api.github.com/repos/amzxyz/rime_wanxiang/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mirror)
        .await;

    let gh = Github::new(5, vec![mirror.uri()], None).unwrap();
    let rel = gh.latest_release("amzxyz/rime_wanxiang").await.unwrap();
    assert_eq!(rel.tag_name, "v9.9");
    assert_eq!(rel.assets.len(), 1);
}

#[tokio::test]
async fn falls_through_to_next_mirror_on_500() {
    let bad = MockServer::start().await;
    let good = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v1.0",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": []
    });
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&bad).await;
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(&body)).mount(&good).await;

    let gh = Github::new(5, vec![bad.uri(), good.uri()], None).unwrap();
    let rel = gh.latest_release("foo/bar").await.unwrap();
    assert_eq!(rel.tag_name, "v1.0");
}
