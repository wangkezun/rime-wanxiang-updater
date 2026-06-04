use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_HISTORY_KEEP: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: u32,
    pub resources: BTreeMap<String, ResourceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceEntry {
    pub tag: String,
    pub asset_name: String,
    pub sha256: String,
    pub installed_at: DateTime<Utc>,
    pub files_installed: Vec<String>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub tag: String,
    pub asset_name: String,
    pub sha256: String,
    pub backup: PathBuf,
    pub installed_at: DateTime<Utc>,
    pub files_installed: Vec<String>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, resources: BTreeMap::new() }
    }
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let m: Self = serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        Ok(m)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Push current state of `id` into history, then set new current state.
    /// Caller is responsible for creating the backup tar.zst at `backup_path`.
    pub fn promote(
        &mut self,
        id: &str,
        new: ResourceEntry,
        backup_path_for_old: Option<PathBuf>,
        keep: usize,
    ) -> Vec<PathBuf> {
        let mut pruned_backups = Vec::new();
        if let (Some(old), Some(backup)) = (self.resources.get(id).cloned(), backup_path_for_old) {
            let mut history = old.history.clone();
            history.insert(
                0,
                HistoryEntry {
                    tag: old.tag,
                    asset_name: old.asset_name,
                    sha256: old.sha256,
                    backup,
                    installed_at: old.installed_at,
                    files_installed: old.files_installed,
                },
            );
            while history.len() > keep {
                if let Some(dropped) = history.pop() {
                    pruned_backups.push(dropped.backup);
                }
            }
            let mut new = new;
            new.history = history;
            self.resources.insert(id.to_string(), new);
        } else {
            self.resources.insert(id.to_string(), new);
        }
        pruned_backups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn entry(tag: &str) -> ResourceEntry {
        ResourceEntry {
            tag: tag.into(),
            asset_name: format!("scheme-{tag}.zip"),
            sha256: "deadbeef".into(),
            installed_at: Utc::now(),
            files_installed: vec!["a.yaml".into()],
            history: vec![],
        }
    }

    #[test]
    fn load_missing_returns_default() {
        let d = TempDir::new().unwrap();
        let m = Manifest::load(&d.path().join("absent.json")).unwrap();
        assert_eq!(m, Manifest::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("m.json");
        let mut m = Manifest::default();
        m.resources.insert("scheme".into(), entry("v1"));
        m.save(&p).unwrap();
        let back = Manifest::load(&p).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn promote_pushes_old_into_history_and_prunes() {
        let mut m = Manifest::default();
        m.resources.insert("scheme".into(), entry("v1"));
        let pruned = m.promote("scheme", entry("v2"), Some(PathBuf::from("b/v1.tar.zst")), 3);
        assert!(pruned.is_empty());
        assert_eq!(m.resources["scheme"].tag, "v2");
        assert_eq!(m.resources["scheme"].history[0].tag, "v1");

        let pruned = m.promote("scheme", entry("v3"), Some(PathBuf::from("b/v2.tar.zst")), 3);
        assert!(pruned.is_empty());
        let pruned = m.promote("scheme", entry("v4"), Some(PathBuf::from("b/v3.tar.zst")), 3);
        assert!(pruned.is_empty());
        // 4th promotion forces the oldest (v1) out.
        let pruned = m.promote("scheme", entry("v5"), Some(PathBuf::from("b/v4.tar.zst")), 3);
        assert_eq!(pruned, vec![PathBuf::from("b/v1.tar.zst")]);
        assert_eq!(m.resources["scheme"].history.len(), 3);
    }
}
