use crate::backup::{extract_tar_zst, write_tar_zst};
use crate::config::Config;
use crate::manifest::{HistoryEntry, Manifest, ResourceEntry};
use crate::platform;
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct RollbackArgs {
    pub only: Vec<String>,
    pub no_deploy: bool,
}

pub struct RollbackOutcome {
    pub rolled_back: Vec<(String, String, String)>, // (id, from_tag, to_tag)
    pub skipped: Vec<(String, &'static str)>,        // (id, reason)
}

pub async fn run(
    cfg: &Config,
    manifest: &mut Manifest,
    manifest_path: &Path,
    data_dir: &Path,
    rime_dir: &Path,
    args: RollbackArgs,
) -> Result<RollbackOutcome> {
    let explicit = !args.only.is_empty();
    let ids: Vec<String> = if explicit {
        args.only.clone()
    } else {
        manifest.resources.keys().cloned().collect()
    };

    let mut outcome = RollbackOutcome { rolled_back: vec![], skipped: vec![] };

    for id in ids {
        let current = match manifest.resources.get(&id) {
            Some(c) => c.clone(),
            None => {
                if explicit { return Err(anyhow!("resource {id} is not installed")); }
                outcome.skipped.push((id, "not installed"));
                continue;
            }
        };
        let Some(prev) = current.history.first().cloned() else {
            if explicit { return Err(anyhow!("resource {id} has no rollback history")); }
            outcome.skipped.push((id, "no history"));
            continue;
        };

        // Capture current state BEFORE modifying the rime_dir, so a second rollback can redo.
        let current_backup_path = data_dir
            .join("backups")
            .join(&id)
            .join(format!("{}.tar.zst", current.tag));
        let current_paths: Vec<PathBuf> =
            current.files_installed.iter().map(PathBuf::from).collect();
        write_tar_zst(rime_dir, &current_paths, &current_backup_path)?;

        // Delete files present in current but not in prev (these were *added* by the latest install).
        let prev_files: HashSet<&String> = prev.files_installed.iter().collect();
        for rel in &current.files_installed {
            if !prev_files.contains(rel) {
                let p = rime_dir.join(rel);
                let _ = std::fs::remove_file(&p);
            }
        }
        // Restore overlapping files from the prior tar.zst.
        extract_tar_zst(&prev.backup, rime_dir)?;

        // Swap: push current onto history, restore prev as current. Make this reversible.
        let from_tag = current.tag.clone();
        let to_tag = prev.tag.clone();
        let mut new_history = current.history.clone();
        new_history.remove(0); // drop prev (it's becoming current)
        new_history.insert(0, HistoryEntry {
            tag: current.tag.clone(),
            asset_name: current.asset_name.clone(),
            sha256: current.sha256.clone(),
            backup: current_backup_path, // freshly captured snapshot of the state being rolled back from
            installed_at: current.installed_at,
            files_installed: current.files_installed.clone(),
        });
        let restored = ResourceEntry {
            tag: prev.tag.clone(),
            asset_name: prev.asset_name.clone(),
            sha256: prev.sha256.clone(),
            installed_at: Utc::now(),
            files_installed: prev.files_installed.clone(),
            history: new_history,
        };
        manifest.resources.insert(id.clone(), restored);
        manifest.save(manifest_path)?;
        outcome.rolled_back.push((id, from_tag, to_tag));
    }

    if cfg.deploy.auto && !args.no_deploy && !outcome.rolled_back.is_empty() {
        platform::deploy(rime_dir)?;
    }
    Ok(outcome)
}
