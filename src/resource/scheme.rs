use super::{install_zip, InstallReport, Resource};
use crate::config::Config;
use crate::safe_list::SafeList;
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

pub struct SchemeResource;

#[async_trait]
impl Resource for SchemeResource {
    fn id(&self) -> &'static str {
        "scheme"
    }
    fn repo(&self) -> &str {
        "amzxyz/rime_wanxiang"
    }
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex> {
        let variant = if cfg.scheme.variant.is_empty() {
            "base"
        } else {
            cfg.scheme.variant.as_str()
        };
        Regex::new(&format!(r"^rime-wanxiang-{}\.zip$", regex::escape(variant)))
            .context("invalid scheme variant regex")
    }
    async fn install(
        &self,
        downloaded: &Path,
        rime_dir: &Path,
        safe: &SafeList,
    ) -> Result<InstallReport> {
        install_zip(downloaded, rime_dir, safe).await
    }
}
