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
    fn id(&self) -> &'static str {
        "gram"
    }
    fn repo(&self) -> &str {
        "amzxyz/RIME-LMDG"
    }
    fn release_tag(&self) -> Option<&str> {
        Some("LTS")
    }
    fn asset_pattern(&self, _cfg: &Config) -> Result<Regex> {
        Ok(Regex::new(r"^wanxiang-lts-zh-hans\.gram$").unwrap())
    }
    async fn install(
        &self,
        downloaded: &Path,
        rime_dir: &Path,
        safe: &SafeList,
    ) -> Result<InstallReport> {
        let rel = std::path::PathBuf::from("wanxiang-lts-zh-hans.gram");
        if safe.is_protected(&rel) {
            return Ok(InstallReport {
                files_written: vec![],
                files_skipped: vec![rel],
            });
        }
        let dst = rime_dir.join(&rel);
        // Replace via a temp-file rename so an existing .gram that the running
        // Rime engine holds memory-mapped does not trigger os error 1224.
        let downloaded = downloaded.to_path_buf();
        tokio::task::spawn_blocking(move || crate::fsutil::replace_copy(&downloaded, &dst))
            .await??;
        Ok(InstallReport {
            files_written: vec![rel],
            files_skipped: vec![],
        })
    }
}
