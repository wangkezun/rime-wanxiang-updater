pub mod scheme;
pub mod gram;
pub mod dict;

use crate::config::Config;
use crate::github::{Asset, Github, Release};
use crate::safe_list::SafeList;
use anyhow::{anyhow, Result};
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
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex>;
    async fn latest_remote(&self, gh: &Github, cfg: &Config) -> Result<RemoteRef> {
        let rel = gh.latest_release(self.repo()).await?;
        select_asset(&rel, &self.asset_pattern(cfg)?)
    }
    async fn install(&self, downloaded: &Path, rime_dir: &Path, safe: &SafeList) -> Result<InstallReport>;
}

pub fn select_asset(rel: &Release, pat: &Regex) -> Result<RemoteRef> {
    let asset: &Asset = rel.assets.iter().find(|a| pat.is_match(&a.name))
        .ok_or_else(|| anyhow!("no asset in tag {} matches {}", rel.tag_name, pat.as_str()))?;
    Ok(RemoteRef {
        tag: rel.tag_name.clone(),
        asset_name: asset.name.clone(),
        asset_url: asset.browser_download_url.clone(),
        asset_size: asset.size,
        sha256: None, // upstream rarely publishes; left None unless a sibling .sha256 asset is found
        published_at: rel.published_at,
    })
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
            assets: assets.into_iter().map(|(n, s)| Asset {
                name: n.into(),
                browser_download_url: format!("https://example.com/{n}"),
                size: s,
            }).collect(),
        }
    }

    #[test]
    fn select_asset_matches_pattern() {
        let r = rel(vec![("readme.txt", 10), ("wanxiang-pinyin-v1.0.zip", 5000)]);
        let pat = Regex::new(r"^wanxiang-pinyin-.*\.zip$").unwrap();
        let rr = select_asset(&r, &pat).unwrap();
        assert_eq!(rr.asset_name, "wanxiang-pinyin-v1.0.zip");
        assert_eq!(rr.asset_size, 5000);
    }

    #[test]
    fn select_asset_fails_when_no_match() {
        let r = rel(vec![("readme.txt", 10)]);
        let pat = Regex::new(r"^wanxiang-pinyin-.*\.zip$").unwrap();
        assert!(select_asset(&r, &pat).is_err());
    }
}
