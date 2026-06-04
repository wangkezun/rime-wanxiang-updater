use super::{InstallReport, Resource};
use crate::config::Config;
use crate::safe_list::SafeList;
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

pub struct SchemeResource;

#[async_trait]
impl Resource for SchemeResource {
    fn id(&self) -> &'static str { "scheme" }
    fn repo(&self) -> &str { "amzxyz/rime_wanxiang" }
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex> {
        let variant = if cfg.scheme.variant.is_empty() { "pinyin" } else { cfg.scheme.variant.as_str() };
        Regex::new(&format!(r"^wanxiang-{}-.*\.zip$", regex::escape(variant)))
            .context("invalid scheme variant regex")
    }
    async fn install(&self, _downloaded: &Path, _rime_dir: &Path, _safe: &SafeList) -> Result<InstallReport> {
        anyhow::bail!("scheme install: implemented in Task 10")
    }
}
