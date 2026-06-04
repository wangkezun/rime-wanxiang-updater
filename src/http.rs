use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

/// Download `url` to `out_path`. Streams body, computing sha256 in the same pass.
/// If `expected_sha256` is Some, the file is removed on mismatch and we return ChecksumMismatch.
pub async fn download(
    client: &Client,
    url: &str,
    out_path: &Path,
    expected_sha256: Option<&str>,
    show_progress: bool,
) -> Result<String> {
    if let Some(p) = out_path.parent() { tokio::fs::create_dir_all(p).await?; }
    let resp = client.get(url).send().await.with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} for {}", resp.status(), url));
    }
    let total = resp.content_length().unwrap_or(0);
    let pb = if show_progress && total > 0 {
        let pb = ProgressBar::new(total);
        pb.set_style(ProgressStyle::with_template(
            "{spinner} {bytes}/{total_bytes} [{wide_bar}] {eta}"
        ).unwrap());
        Some(pb)
    } else { None };

    let mut file = File::create(out_path).await.with_context(|| format!("create {}", out_path.display()))?;
    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        if let Some(pb) = &pb { pb.inc(chunk.len() as u64); }
    }
    file.flush().await?;
    if let Some(pb) = pb { pb.finish_and_clear(); }
    let actual = hex::encode(hasher.finalize());
    if let Some(expected) = expected_sha256 {
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = tokio::fs::remove_file(out_path).await;
            return Err(DownloadError::ChecksumMismatch { expected: expected.into(), actual }.into());
        }
    }
    Ok(actual)
}
