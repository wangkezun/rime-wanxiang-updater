pub mod dict;
pub mod gram;
pub mod scheme;

use crate::config::Config;
use crate::github::{Asset, Github, Release};
use crate::safe_list::SafeList;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RemoteRef {
    pub tag: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
    /// Hex sha256 from the GitHub `asset.digest` field; `None` for assets
    /// uploaded before GitHub started exposing digests (2025-06-03).
    pub sha256: Option<String>,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    pub files_written: Vec<PathBuf>,
    pub files_skipped: Vec<PathBuf>,
}

#[async_trait]
pub trait Resource: Send + Sync {
    fn id(&self) -> &'static str;
    fn repo(&self) -> &str;
    /// Optional pinned release tag. None means "latest".
    fn release_tag(&self) -> Option<&str> {
        None
    }
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex>;
    async fn latest_remote(&self, gh: &Github, cfg: &Config) -> Result<RemoteRef> {
        let rel = match self.release_tag() {
            Some(tag) => gh.release_by_tag(self.repo(), tag).await?,
            None => gh.latest_release(self.repo()).await?,
        };
        select_asset(&rel, &self.asset_pattern(cfg)?)
    }
    async fn install(
        &self,
        downloaded: &Path,
        rime_dir: &Path,
        safe: &SafeList,
    ) -> Result<InstallReport>;
}

pub fn select_asset(rel: &Release, pat: &Regex) -> Result<RemoteRef> {
    let asset: &Asset = rel
        .assets
        .iter()
        .find(|a| pat.is_match(&a.name))
        .ok_or_else(|| anyhow!("no asset in tag {} matches {}", rel.tag_name, pat.as_str()))?;
    Ok(RemoteRef {
        tag: rel.tag_name.clone(),
        asset_name: asset.name.clone(),
        asset_url: asset.browser_download_url.clone(),
        asset_size: asset.size,
        sha256: asset.sha256().map(|s| s.to_string()),
        published_at: rel.published_at,
    })
}

/// Shared install path for zip-packaged resources (scheme, dict): extract every
/// file entry into `rime_dir`, skipping protected paths. The blocking closure
/// rebuilds the SafeList from its patterns because the borrowed one cannot move
/// into `spawn_blocking`.
pub(crate) async fn install_zip(
    downloaded: &Path,
    rime_dir: &Path,
    safe: &SafeList,
) -> Result<InstallReport> {
    let downloaded = downloaded.to_path_buf();
    let rime_dir = rime_dir.to_path_buf();
    let patterns = safe.patterns().to_vec();
    tokio::task::spawn_blocking(move || -> Result<InstallReport> {
        let safe = SafeList::new(&patterns)?;
        let file = std::fs::File::open(&downloaded)
            .with_context(|| format!("open {}", downloaded.display()))?;
        let mut zip = zip::ZipArchive::new(file)?;
        let mut report = InstallReport::default();
        std::fs::create_dir_all(&rime_dir)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
                continue;
            };
            if entry.is_dir() {
                continue;
            }
            if safe.is_protected(&rel) {
                report.files_skipped.push(rel);
                continue;
            }
            let out = rime_dir.join(&rel);
            crate::fsutil::replace_with_reader(&out, &mut entry)?;
            report.files_written.push(rel);
        }
        Ok(report)
    })
    .await?
}

pub fn registry() -> Vec<Box<dyn Resource>> {
    vec![
        Box::new(scheme::SchemeResource),
        Box::new(gram::GramResource),
        Box::new(dict::DictResource),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{Asset, Release};
    use chrono::Utc;

    fn rel(assets: Vec<(&str, u64)>) -> Release {
        Release {
            tag_name: "v1.0".into(),
            published_at: Utc::now(),
            assets: assets
                .into_iter()
                .map(|(n, s)| Asset {
                    name: n.into(),
                    browser_download_url: format!("https://example.com/{n}"),
                    size: s,
                    digest: None,
                })
                .collect(),
        }
    }

    #[test]
    fn select_asset_matches_pattern() {
        let r = rel(vec![("readme.txt", 10), ("rime-wanxiang-base.zip", 5000)]);
        let pat = Regex::new(r"^rime-wanxiang-base\.zip$").unwrap();
        let rr = select_asset(&r, &pat).unwrap();
        assert_eq!(rr.asset_name, "rime-wanxiang-base.zip");
        assert_eq!(rr.asset_size, 5000);
    }

    #[test]
    fn select_asset_fails_when_no_match() {
        let r = rel(vec![("readme.txt", 10)]);
        let pat = Regex::new(r"^rime-wanxiang-base\.zip$").unwrap();
        assert!(select_asset(&r, &pat).is_err());
    }

    #[test]
    fn select_asset_extracts_sha256_from_digest() {
        let mut r = rel(vec![("rime-wanxiang-base.zip", 5000)]);
        r.assets[0].digest = Some("sha256:deadbeef".into());
        let pat = Regex::new(r"^rime-wanxiang-base\.zip$").unwrap();
        let rr = select_asset(&r, &pat).unwrap();
        assert_eq!(rr.sha256.as_deref(), Some("deadbeef"));
    }
}
