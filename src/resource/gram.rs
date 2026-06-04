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
    async fn install(&self, downloaded: &Path, rime_dir: &Path, safe: &SafeList) -> Result<InstallReport> {
        let rel = std::path::PathBuf::from("wanxiang-lts-zh-hans.gram");
        if safe.is_protected(&rel) {
            return Ok(InstallReport { files_written: vec![], files_skipped: vec![rel] });
        }
        let dst = rime_dir.join(&rel);
        if let Some(p) = dst.parent() { tokio::fs::create_dir_all(p).await?; }
        tokio::fs::copy(downloaded, &dst).await?;
        Ok(InstallReport { files_written: vec![rel], files_skipped: vec![] })
    }
}
