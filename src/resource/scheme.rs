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
    fn id(&self) -> &'static str {
        "scheme"
    }
    fn repo(&self) -> &str {
        "amzxyz/rime_wanxiang"
    }
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex> {
        let variant = if cfg.scheme.variant.is_empty() {
            "pinyin"
        } else {
            cfg.scheme.variant.as_str()
        };
        Regex::new(&format!(r"^wanxiang-{}-.*\.zip$", regex::escape(variant)))
            .context("invalid scheme variant regex")
    }
    async fn install(
        &self,
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
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut out_f = std::fs::File::create(&out)?;
                std::io::copy(&mut entry, &mut out_f)?;
                report.files_written.push(rel);
            }
            Ok(report)
        })
        .await?
    }
}
