use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wxupd::http::download;

#[tokio::test]
async fn downloads_and_returns_sha256() {
    let body = b"hello wxupd";
    let expected = "8b6a0a44c81e85e22ca9d4b13d4b18b9e1c97c5b6e6b29f6e7e6e3a45c3c3c40"; // recomputed below
    let srv = MockServer::start().await;
    Mock::given(method("GET")).and(path("/file.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&srv).await;

    // Compute the real expected hash so the test isn't brittle to my hand-typed value.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new(); h.update(body);
    let real_expected = hex::encode(h.finalize());

    let d = TempDir::new().unwrap();
    let out = d.path().join("dl.bin");
    let client = reqwest::Client::new();
    let actual = download(&client, &format!("{}/file.bin", srv.uri()), &out, None, false).await.unwrap();
    assert_eq!(actual, real_expected);
    assert_eq!(std::fs::read(&out).unwrap(), body);
    // Silence the unused placeholder.
    let _ = expected;
}

#[tokio::test]
async fn checksum_mismatch_deletes_file_and_errors() {
    let body = b"some bytes";
    let srv = MockServer::start().await;
    Mock::given(method("GET")).and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&srv).await;
    let d = TempDir::new().unwrap();
    let out = d.path().join("x");
    let client = reqwest::Client::new();
    let err = download(&client, &format!("{}/x", srv.uri()), &out, Some("00".repeat(32).as_str()), false).await.unwrap_err();
    assert!(err.to_string().contains("sha256 mismatch"));
    assert!(!out.exists());
}
