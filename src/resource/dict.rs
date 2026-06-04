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
    async fn install(&self, downloaded: &Path, rime_dir: &Path, safe: &SafeList) -> Result<InstallReport> {
        let file_name = downloaded.file_name()
            .ok_or_else(|| anyhow::anyhow!("dict source has no file name"))?
            .to_owned();
        let rel = std::path::PathBuf::from(&file_name);
        if safe.is_protected(&rel) {
            return Ok(InstallReport { files_written: vec![], files_skipped: vec![rel] });
        }
        let dst = rime_dir.join(&rel);
        if let Some(p) = dst.parent() { tokio::fs::create_dir_all(p).await?; }
        tokio::fs::copy(downloaded, &dst).await?;
        Ok(InstallReport { files_written: vec![rel], files_skipped: vec![] })
    }
}
