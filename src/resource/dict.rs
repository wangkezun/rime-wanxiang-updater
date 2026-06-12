use super::{install_zip, InstallReport, Resource};
use crate::config::Config;
use crate::safe_list::SafeList;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

pub struct DictResource;

#[async_trait]
impl Resource for DictResource {
    fn id(&self) -> &'static str {
        "dict"
    }
    fn repo(&self) -> &str {
        "amzxyz/RIME-LMDG"
    }
    fn release_tag(&self) -> Option<&str> {
        Some("dict-nightly")
    }
    fn asset_pattern(&self, _cfg: &Config) -> Result<Regex> {
        Ok(Regex::new(r"^dicts\.zip$").unwrap())
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
