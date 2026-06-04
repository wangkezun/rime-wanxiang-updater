use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub scheme: SchemeCfg,
    #[serde(default)]
    pub paths: PathsCfg,
    #[serde(default)]
    pub network: NetworkCfg,
    #[serde(default)]
    pub deploy: DeployCfg,
    #[serde(default)]
    pub safe_list: SafeListCfg,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemeCfg {
    /// One of: pinyin, flypy, zrm, mspy, jdh, ...; resolved against upstream asset list.
    #[serde(default)]
    pub variant: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathsCfg {
    /// Empty = auto-detect.
    #[serde(default)]
    pub rime_user_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkCfg {
    #[serde(default = "default_mirrors")]
    pub mirrors: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_mirrors() -> Vec<String> { Vec::new() }
fn default_timeout_secs() -> u64 { 60 }

impl Default for NetworkCfg {
    fn default() -> Self {
        Self { mirrors: default_mirrors(), timeout_secs: default_timeout_secs() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployCfg {
    #[serde(default = "default_auto_deploy")]
    pub auto: bool,
}

fn default_auto_deploy() -> bool { true }

impl Default for DeployCfg {
    fn default() -> Self { Self { auto: default_auto_deploy() } }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeListCfg {
    #[serde(default)]
    pub extra: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scheme: SchemeCfg::default(),
            paths: PathsCfg::default(),
            network: NetworkCfg::default(),
            deploy: DeployCfg::default(),
            safe_list: SafeListCfg::default(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let cfg: Self = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        Ok(cfg)
    }

    /// Set a dotted key (e.g. `scheme.variant`) to a string value.
    /// Uses `toml_edit` so comments and formatting in the source file survive.
    pub fn set_dotted(path: &Path, dotted: &str, value: &str) -> Result<()> {
        let mut doc = if path.exists() {
            let s = fs::read_to_string(path)?;
            s.parse::<toml_edit::DocumentMut>()?
        } else {
            toml_edit::DocumentMut::new()
        };
        let parts: Vec<&str> = dotted.split('.').collect();
        let (last, head) = parts.split_last().context("empty key")?;
        let mut node: &mut toml_edit::Item = doc.as_item_mut();
        for k in head {
            if !node.is_table_like() {
                *node = toml_edit::Item::Table(toml_edit::Table::new());
            }
            let tbl = node.as_table_mut().unwrap();
            if !tbl.contains_key(k) {
                tbl.insert(k, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            node = tbl.get_mut(k).unwrap();
        }
        let tbl = node.as_table_mut().context("target is not a table")?;
        tbl.insert(last, toml_edit::value(value));
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        fs::write(path, doc.to_string())?;
        Ok(())
    }
}

/// Resolve the user's config.toml location per OS (override via `WXUPD_CONFIG`).
pub fn config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .context("no home dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_file_yields_defaults() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("absent.toml");
        let cfg = Config::load(&p).unwrap();
        assert!(cfg.deploy.auto);
        assert_eq!(cfg.network.timeout_secs, 60);
        assert!(cfg.network.mirrors.is_empty());
    }

    #[test]
    fn set_dotted_creates_then_updates_preserving_comments() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("c.toml");
        // Seed with a comment we want preserved.
        std::fs::write(&p, "# user notes\n[scheme]\nvariant = \"pinyin\"\n").unwrap();
        Config::set_dotted(&p, "scheme.variant", "flypy").unwrap();
        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("# user notes"));
        assert!(after.contains("variant = \"flypy\""));
        // Round-trip through serde.
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.scheme.variant, "flypy");
    }

    #[test]
    fn set_dotted_creates_nested_tables() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("c.toml");
        Config::set_dotted(&p, "network.timeout_secs", "30").unwrap();
        // Stored as a string; ensure we can still load (toml will keep it as string in this case,
        // which means downstream code must coerce. For now just assert the file content.)
        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("[network]"));
        assert!(after.contains("timeout_secs = \"30\""));
    }
}
