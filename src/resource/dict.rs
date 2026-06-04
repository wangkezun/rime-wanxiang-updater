use super::{InstallReport, Resource};
use crate::config::Config;
use crate::safe_list::SafeList;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

pub struct DictResource;

#[async_trait]
impl Resource for DictResource {
    fn id(&self) -> &'static str { "dict" }
    // NOTE: implementer must verify the correct repo/asset at impl time.
    // Leaving the wanxiang repo here; the asset pattern below assumes the
    // cn_en mixed dict ships alongside scheme releases.
    fn repo(&self) -> &str { "amzxyz/rime_wanxiang" }
    fn asset_pattern(&self, _cfg: &Config) -> Result<Regex> {
        Ok(Regex::new(r"^cn_en_.*\.dict\.yaml$").unwrap())
    }
    async fn install(&self, _downloaded: &Path, _rime_dir: &Path, _safe: &SafeList) -> Result<InstallReport> {
        anyhow::bail!("dict install: implemented in Task 12")
    }
}
