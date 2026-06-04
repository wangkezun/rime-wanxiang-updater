use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error(transparent)]
    Network(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

pub struct Github {
    client: Client,
    token: Option<String>,
    mirrors: Vec<String>,
}

impl Github {
    pub fn new(timeout_secs: u64, mirrors: Vec<String>, token: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(concat!("wxupd/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            client,
            token,
            mirrors,
        })
    }

    pub async fn latest_release(&self, repo: &str) -> Result<Release> {
        self.fetch_release(&format!("repos/{repo}/releases/latest"))
            .await
    }

    pub async fn release_by_tag(&self, repo: &str, tag: &str) -> Result<Release> {
        self.fetch_release(&format!("repos/{repo}/releases/tags/{tag}"))
            .await
    }

    async fn fetch_release(&self, url_suffix: &str) -> Result<Release> {
        let base = format!("https://api.github.com/{url_suffix}");
        let urls = if self.mirrors.is_empty() {
            vec![base.clone()]
        } else {
            let base_clone = base.clone();
            self.mirrors
                .iter()
                .map(move |m| format!("{}/{}", m.trim_end_matches('/'), base_clone))
                .chain(std::iter::once(base))
                .collect()
        };
        let mut last_err: Option<anyhow::Error> = None;
        for url in urls {
            let mut req = self
                .client
                .get(&url)
                .header("Accept", "application/vnd.github+json");
            if let Some(t) = &self.token {
                req = req.header("Authorization", format!("Bearer {t}"));
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp.json::<Release>().await.map_err(GithubError::from)?);
                    }
                    let body = resp.text().await.unwrap_or_default();
                    let err: anyhow::Error = match status.as_u16() {
                        403 if body.contains("rate limit") => GithubError::RateLimited.into(),
                        404 => GithubError::NotFound.into(),
                        s => GithubError::Http { status: s, body }.into(),
                    };
                    last_err = Some(err);
                }
                Err(e) => last_err = Some(anyhow!(GithubError::from(e))),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("no mirrors configured")))
    }
}

/// Mirror the download URL of an asset through the same chain.
pub fn rewrite_asset_url(url: &str, mirrors: &[String]) -> Vec<String> {
    let mut out: Vec<String> = mirrors
        .iter()
        .map(|m| format!("{}/{}", m.trim_end_matches('/'), url))
        .collect();
    out.push(url.to_string());
    out
}
