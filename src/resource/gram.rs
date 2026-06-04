use super::{InstallReport, Resource};
use crate::config::Config;
use crate::safe_list::SafeList;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

pub struct GramResource;

#[async_trait]
impl Resource for GramResource {
    fn id(&self) -> &'static str { "gram" }
    // NOTE: implementer must verify this repo name against the upstream
    // wanxiang release ecosystem at impl time. amzxyz/RIME-LMDG was the
    // dedicated gram release repo as of the spec date.
    fn repo(&self) -> &str { "amzxyz/RIME-LMDG" }
    fn asset_pattern(&self, _cfg: &Config) -> Result<Regex> {
        Ok(Regex::new(r"^wanxiang-lts-zh-hans\.gram$").unwrap())
    }
    async fn install(&self, _downloaded: &Path, _rime_dir: &Path, _safe: &SafeList) -> Result<InstallReport> {
        anyhow::bail!("gram install: implemented in Task 11")
    }
}
