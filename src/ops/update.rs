use crate::backup::{extract_tar_zst, write_tar_zst};
use crate::config::Config;
use crate::github::{rewrite_asset_url, Github};
use crate::http::download;
use crate::manifest::{Manifest, ResourceEntry, DEFAULT_HISTORY_KEEP};
use crate::ops::check::{self, Status};
use crate::platform;
use crate::resource::{registry, InstallReport, RemoteRef, Resource};
use crate::safe_list::SafeList;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use futures_util::future::join_all;
use std::path::{Path, PathBuf};
use tracing::warn;

pub struct UpdateArgs {
    pub only: Vec<String>,
    pub force: bool,
    pub no_deploy: bool,
}

pub struct UpdateResult {
    pub installed: Vec<(String, String, String)>, // (id, old_tag, new_tag)
    pub skipped_protected: Vec<(String, Vec<PathBuf>)>,
    pub failures: Vec<(String, String)>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    cfg: &Config,
    gh: &Github,
    manifest: &mut Manifest,
    manifest_path: &Path,
    cache_dir: &Path,
    data_dir: &Path,
    rime_dir: &Path,
    args: UpdateArgs,
) -> Result<UpdateResult> {
    let mut all = registry();
    if !args.only.is_empty() {
        all.retain(|r| args.only.iter().any(|s| s.as_str() == r.id()));
        if all.is_empty() {
            anyhow::bail!("no matching resources for {:?}", args.only);
        }
    }

    // Sweep any temp/sidecar files left behind by a previous run whose
    // mmap-blocked deletes have since been released (see crate::fsutil).
    crate::fsutil::purge_stale(rime_dir);

    // 1. Discover remotes.
    let check_report = check::run(cfg, gh, manifest).await?;

    // 2. Filter to has-update / not-installed (unless --force).
    let mut targets: Vec<(Box<dyn Resource>, RemoteRef)> = Vec::new();
    let mut pre_failures: Vec<(String, String)> = Vec::new();
    for res in all {
        let rep = check_report.resources.iter().find(|r| r.id == res.id());
        let Some(rep) = rep else { continue };
        // Surface check-time errors so the user sees them in the failure summary,
        // regardless of --force. --force cannot recover from a missing remote ref.
        if rep.status == Status::Error {
            pre_failures.push((
                rep.id.clone(),
                rep.error
                    .clone()
                    .unwrap_or_else(|| "remote lookup failed".into()),
            ));
            continue;
        }
        let needs = matches!(rep.status, Status::HasUpdate | Status::NotInstalled) || args.force;
        if !needs {
            continue;
        }
        // Re-fetch remote ref to get the full struct (check returned a summary).
        let rr = res.latest_remote(gh, cfg).await?;
        targets.push((res, rr));
    }
    if targets.is_empty() {
        return Ok(UpdateResult {
            installed: vec![],
            skipped_protected: vec![],
            failures: pre_failures,
        });
    }

    // 3. Parallel download.
    let staging = cache_dir.join("staging");
    std::fs::create_dir_all(&staging)?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("wxupd/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let dl_futs = targets.iter().map(|(res, rr)| {
        let client = client.clone();
        let staging = staging.clone();
        let mirrors = cfg.network.mirrors.clone();
        let id = res.id().to_string();
        let rr = rr.clone();
        async move {
            let dst = staging.join(&id).join(&rr.asset_name);
            let mut last_err: Option<anyhow::Error> = None;
            for url in rewrite_asset_url(&rr.asset_url, &mirrors) {
                match download(&client, &url, &dst, rr.sha256.as_deref(), true).await {
                    Ok(sha) => return Ok::<_, anyhow::Error>((id, dst, sha, rr.clone())),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.unwrap_or_else(|| anyhow!("no urls to try")))
        }
    });
    // A failed download only fails that resource; the rest still install.
    let downloads = join_all(dl_futs).await;

    // 4. Serial install.
    let safe = SafeList::defaults_plus(&cfg.safe_list.extra)?;
    let backups_dir = data_dir.join("backups");
    let mut result = UpdateResult {
        installed: vec![],
        skipped_protected: vec![],
        failures: vec![],
    };
    result.failures.extend(pre_failures);

    for ((res, _rr_target), dl) in targets.iter().zip(downloads) {
        let (id, downloaded, sha, rr) = match dl {
            Ok(v) => v,
            Err(e) => {
                result.failures.push((res.id().to_string(), e.to_string()));
                continue;
            }
        };
        // Backup the prior state of files this install will touch (if any).
        let prior_files = manifest
            .resources
            .get(&id)
            .map(|e| e.files_installed.clone())
            .unwrap_or_default();
        let prior_paths: Vec<PathBuf> = prior_files.iter().map(PathBuf::from).collect();
        let backup_path = if !prior_paths.is_empty() {
            let prior_tag = manifest.resources[&id].tag.clone();
            let p = backups_dir.join(&id).join(format!("{}.tar.zst", prior_tag));
            write_tar_zst(rime_dir, &prior_paths, &p)?;
            Some(p)
        } else {
            None
        };

        // Install.
        match res.install(&downloaded, rime_dir, &safe).await {
            Ok(InstallReport {
                files_written,
                files_skipped,
            }) => {
                if !files_skipped.is_empty() {
                    result.skipped_protected.push((id.clone(), files_skipped));
                }
                let old_tag = manifest
                    .resources
                    .get(&id)
                    .map(|e| e.tag.clone())
                    .unwrap_or_else(|| "-".to_string());
                let entry = ResourceEntry {
                    tag: rr.tag.clone(),
                    asset_name: rr.asset_name.clone(),
                    sha256: sha.clone(),
                    installed_at: Utc::now(),
                    files_installed: files_written
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                    history: vec![],
                };
                let pruned = manifest.promote(&id, entry, backup_path, DEFAULT_HISTORY_KEEP);
                for p in pruned {
                    let _ = std::fs::remove_file(p);
                }
                result.installed.push((id.clone(), old_tag, rr.tag.clone()));
            }
            Err(e) => {
                warn!("install failed for {}: {e}; attempting rollback", id);
                if let Some(p) = &backup_path {
                    if let Err(re) = extract_tar_zst(p, rime_dir) {
                        warn!("rollback of {} also failed: {re}", id);
                    }
                }
                result.failures.push((id.clone(), e.to_string()));
            }
        }

        manifest.save(manifest_path).context("persist manifest")?;
    }

    // 5. Deploy.
    if cfg.deploy.auto && !args.no_deploy && result.failures.is_empty() {
        platform::deploy(rime_dir)?;
    }

    Ok(result)
}
