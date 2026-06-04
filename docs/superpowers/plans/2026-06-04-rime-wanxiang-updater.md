# rime-wanxiang-updater (`wxupd`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a single-binary Rust CLI `wxupd` that keeps `rime-wanxiang` scheme, gram model, and extra dictionaries up to date on macOS/Windows/Linux, with safe-list protection, atomic per-resource install, and rollback.

**Architecture:** Async runtime (tokio + reqwest) for concurrent downloads. A `Resource` trait abstracts the three asset types; outer `ops::{check,update,rollback}` orchestrators handle download/verify/backup/manifest. Config in `config.toml`, state + history in `manifest.json`, backups as `tar.zst`. Platform module isolates Rime user-dir detection and deploy commands behind `cfg!(target_os)` branches.

**Tech Stack:** Rust 1.78+, clap (derive), tokio, reqwest (rustls), serde / serde_json, toml + toml_edit, zip, tar + zstd, sha2, regex, globset, semver, directories, indicatif, anyhow + thiserror, tracing, chrono. Dev: wiremock, assert_cmd, tempfile, predicates, insta.

**Spec reference:** `docs/superpowers/specs/2026-06-04-rime-wanxiang-updater-design.md`

---

## File Structure

```
.
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── .github/workflows/
│   ├── test.yml
│   └── release.yml
├── docs/superpowers/{specs,plans}/...
├── src/
│   ├── main.rs               # clap entry, top-level error → exit code mapping
│   ├── cli.rs                # clap derive structs
│   ├── config.rs             # Config struct + load/save (toml_edit)
│   ├── manifest.rs           # Manifest + ResourceEntry + HistoryEntry + load/save (serde_json)
│   ├── safe_list.rs          # SafeList wrapping globset::GlobSet
│   ├── backup.rs             # write_tar_zst / extract_tar_zst
│   ├── platform.rs           # rime_user_dir() + deploy()
│   ├── github.rs             # Github client + Release struct + mirror fallback
│   ├── http.rs               # download_to_file_with_sha256
│   ├── resource/
│   │   ├── mod.rs            # Resource trait + RemoteRef + InstallReport + registry()
│   │   ├── scheme.rs         # SchemeResource (variant-aware zip)
│   │   ├── gram.rs           # GramResource (single .gram file)
│   │   └── dict.rs           # DictResource (yaml files)
│   └── ops/
│       ├── mod.rs            # re-exports
│       ├── check.rs          # Status enum + run() + JSON output
│       ├── update.rs         # plan → download (parallel) → install (serial) → deploy
│       └── rollback.rs       # restore from history[0]
└── tests/
    ├── common/mod.rs         # tempdir, wiremock helper, fixture builder
    ├── check_test.rs
    ├── update_test.rs
    ├── rollback_test.rs
    ├── config_test.rs
    └── fixtures/
        ├── scheme-fake.zip
        ├── gram-fake.gram
        └── dict-fake.dict.yaml
```

One file = one responsibility. Tests live alongside the integration boundary (`tests/`) for end-to-end; pure logic gets `#[cfg(test)] mod tests` inline in each `src/*.rs`.

---

## Task 1: Cargo scaffold, clap skeleton, .gitignore

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/cli.rs`, `.gitignore`

- [ ] **Step 1: Write `.gitignore`**

```
/target
**/*.rs.bk
Cargo.lock.bak
.DS_Store
.claude/settings.local.json
```

- [ ] **Step 2: Write `Cargo.toml`**

```toml
[package]
name = "wxupd"
version = "0.1.0"
edition = "2021"
rust-version = "1.78"
description = "rime-wanxiang scheme + gram + dict updater"
license = "MIT"

[[bin]]
name = "wxupd"
path = "src/main.rs"

[dependencies]
anyhow = "1"
thiserror = "1"
clap = { version = "4", features = ["derive", "env"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs", "signal", "process"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
toml_edit = "0.22"
zip = { version = "2", default-features = false, features = ["deflate"] }
tar = "0.4"
zstd = "0.13"
sha2 = "0.10"
hex = "0.4"
regex = "1"
globset = "0.4"
semver = "1"
directories = "5"
indicatif = "0.17"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
wiremock = "0.6"
assert_cmd = "2"
predicates = "3"
tempfile = "3"
insta = { version = "1", features = ["json"] }
```

- [ ] **Step 3: Write `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "wxupd", version, about = "rime-wanxiang updater")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[arg(long, global = true, env = "WXUPD_LOG", default_value = "info")]
    pub log: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Check for upstream updates without downloading
    Check {
        #[arg(long)]
        json: bool,
    },
    /// Download and install available updates
    Update {
        /// Specific resources to update (default: all)
        resources: Vec<String>,
        #[arg(long)]
        no_deploy: bool,
        #[arg(long)]
        force: bool,
    },
    /// Roll back to the previous installed version
    Rollback {
        resources: Vec<String>,
        #[arg(long)]
        no_deploy: bool,
    },
    /// Show or modify config.toml
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Show,
    Set { kv: String },
}
```

- [ ] **Step 4: Write `src/main.rs`**

```rust
mod cli;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&args.log))
        .with_writer(std::io::stderr)
        .init();
    match args.command {
        cli::Command::Check { .. } => println!("check: not implemented yet"),
        cli::Command::Update { .. } => println!("update: not implemented yet"),
        cli::Command::Rollback { .. } => println!("rollback: not implemented yet"),
        cli::Command::Config { .. } => println!("config: not implemented yet"),
    }
    Ok(())
}
```

- [ ] **Step 5: Build and smoke-test**

Run: `cargo build`
Expected: compiles cleanly.

Run: `cargo run -- --help`
Expected: usage prints `Commands: check, update, rollback, config`.

Run: `cargo run -- check`
Expected: prints `check: not implemented yet`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/ .gitignore
git commit -m "Task 1: scaffold cargo project with clap skeleton"
```

---

## Task 2: SafeList module (glob matcher)

**Files:**
- Create: `src/safe_list.rs`
- Modify: `src/main.rs` (add `mod safe_list;`)

- [ ] **Step 1: Write failing tests**

Append to a fresh `src/safe_list.rs`:

```rust
use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

pub struct SafeList {
    set: GlobSet,
}

impl SafeList {
    pub fn new(patterns: &[String]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            b.add(Glob::new(p)?);
        }
        Ok(Self { set: b.build()? })
    }

    pub fn defaults_plus(extra: &[String]) -> Result<Self> {
        let defaults = [
            "*.custom.yaml",
            "installation.yaml",
            "user.yaml",
            "*.userdb*",
            "*.userdb.txt",
            "sync/**",
            "build/**",
        ];
        let merged: Vec<String> = defaults.iter().map(|s| s.to_string()).chain(extra.iter().cloned()).collect();
        Self::new(&merged)
    }

    pub fn is_protected<P: AsRef<Path>>(&self, rel: P) -> bool {
        self.set.is_match(rel.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_custom_yaml() {
        let s = SafeList::defaults_plus(&[]).unwrap();
        assert!(s.is_protected("wanxiang.custom.yaml"));
        assert!(s.is_protected("user.yaml"));
        assert!(!s.is_protected("wanxiang.schema.yaml"));
    }

    #[test]
    fn protects_userdb_recursive() {
        let s = SafeList::defaults_plus(&[]).unwrap();
        assert!(s.is_protected("pinyin.userdb.txt"));
        assert!(s.is_protected("sync/foo/bar.yaml"));
        assert!(s.is_protected("build/anything"));
    }

    #[test]
    fn extra_patterns_merge() {
        let s = SafeList::defaults_plus(&["mine.yaml".to_string()]).unwrap();
        assert!(s.is_protected("mine.yaml"));
        assert!(s.is_protected("user.yaml"));
    }
}
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod safe_list;` near the top with the other `mod` lines.

- [ ] **Step 3: Run tests**

Run: `cargo test safe_list`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/safe_list.rs src/main.rs
git commit -m "Task 2: SafeList with glob-based protection patterns"
```

---

## Task 3: Manifest module

**Files:**
- Create: `src/manifest.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/manifest.rs`**

```rust
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
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod manifest;`.

- [ ] **Step 3: Run tests**

Run: `cargo test manifest`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/manifest.rs src/main.rs
git commit -m "Task 3: manifest with history pruning"
```

---

## Task 4: Config module

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/config.rs`**

```rust
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
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod config;`.

- [ ] **Step 3: Run tests**

Run: `cargo test config`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "Task 4: config with toml_edit-preserving set_dotted"
```

---

## Task 5: Backup module (tar.zst round-trip)

**Files:**
- Create: `src/backup.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/backup.rs`**

```rust
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Pack `rel_paths` (relative to `root`) into a zstd-compressed tar at `out`.
/// Missing files are skipped silently — they may have been deleted between
/// the manifest write and the backup call.
pub fn write_tar_zst(root: &Path, rel_paths: &[PathBuf], out: &Path) -> Result<()> {
    if let Some(p) = out.parent() { fs::create_dir_all(p)?; }
    let file = File::create(out).with_context(|| format!("create {}", out.display()))?;
    let enc = zstd::Encoder::new(BufWriter::new(file), 3)?.auto_finish();
    let mut tar = tar::Builder::new(enc);
    for rel in rel_paths {
        let abs = root.join(rel);
        if !abs.exists() { continue; }
        tar.append_path_with_name(&abs, rel)
            .with_context(|| format!("append {}", rel.display()))?;
    }
    tar.finish()?;
    Ok(())
}

/// Extract `archive` over `root`, overwriting any colliding files.
pub fn extract_tar_zst(archive: &Path, root: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let dec = zstd::Decoder::new(BufReader::new(file))?;
    let mut tar = tar::Archive::new(dec);
    let mut written = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let rel = entry.path()?.to_path_buf();
        let dst = root.join(&rel);
        if let Some(p) = dst.parent() { fs::create_dir_all(p)?; }
        entry.unpack(&dst)?;
        written.push(rel);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_two_files_with_subdir() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        fs::create_dir_all(src.path().join("sub")).unwrap();
        fs::write(src.path().join("a.yaml"), b"hello").unwrap();
        fs::write(src.path().join("sub/b.yaml"), b"world").unwrap();
        let archive = src.path().join("backup.tar.zst");
        let files = vec![PathBuf::from("a.yaml"), PathBuf::from("sub/b.yaml")];
        write_tar_zst(src.path(), &files, &archive).unwrap();

        let written = extract_tar_zst(&archive, dst.path()).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(fs::read(dst.path().join("a.yaml")).unwrap(), b"hello");
        assert_eq!(fs::read(dst.path().join("sub/b.yaml")).unwrap(), b"world");
    }

    #[test]
    fn missing_files_skipped_silently() {
        let src = TempDir::new().unwrap();
        let archive = src.path().join("backup.tar.zst");
        let files = vec![PathBuf::from("does-not-exist.yaml")];
        // Should NOT error.
        write_tar_zst(src.path(), &files, &archive).unwrap();
        let dst = TempDir::new().unwrap();
        let written = extract_tar_zst(&archive, dst.path()).unwrap();
        assert!(written.is_empty());
    }
}
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod backup;`.

- [ ] **Step 3: Run tests**

Run: `cargo test backup`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/backup.rs src/main.rs
git commit -m "Task 5: tar.zst backup write/extract with missing-file tolerance"
```

---

## Task 6: Platform module (Rime user dir + deploy)

**Files:**
- Create: `src/platform.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/platform.rs`**

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::warn;

pub fn rime_user_dir(override_path: &str) -> Result<PathBuf> {
    if !override_path.is_empty() {
        return Ok(PathBuf::from(override_path));
    }
    let home = directories::BaseDirs::new().context("no home dir")?.home_dir().to_path_buf();
    if cfg!(target_os = "macos") {
        return Ok(home.join("Library/Rime"));
    }
    if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").context("APPDATA not set")?;
        return Ok(PathBuf::from(appdata).join("Rime"));
    }
    // Linux
    if let Ok(p) = std::env::var("IBUS_RIME_USER_DATA_DIR") {
        return Ok(PathBuf::from(p));
    }
    let candidates: Vec<PathBuf> = vec![
        home.join(".config/ibus/rime"),
        home.join(".local/share/fcitx5/rime"),
    ];
    for c in &candidates {
        if c.exists() { return Ok(c.clone()); }
    }
    anyhow::bail!(
        "could not locate Rime user dir; set [paths].rime_user_dir in config.toml. \
         Tried: {:?}",
        candidates
    );
}

/// Best-effort deploy. Returns Ok even on failure but logs a warning, since
/// the user can always deploy manually.
pub fn deploy(rime_user_dir: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let result = Command::new("/Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel")
        .arg("--reload")
        .status();
    #[cfg(target_os = "windows")]
    let result = Command::new("WeaselDeployer.exe").arg("/deploy").status();
    #[cfg(target_os = "linux")]
    let result = Command::new("rime_deployer")
        .arg("--build")
        .arg(rime_user_dir)
        .status();
    // Silence unused variable on macOS/Windows where we don't use rime_user_dir.
    let _ = rime_user_dir;
    match result {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            warn!("deploy command exited with status {s}; please redeploy manually");
            Ok(())
        }
        Err(e) => {
            warn!("could not invoke deploy command: {e}; please redeploy manually");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_path_wins() {
        let p = rime_user_dir("/explicit/path").unwrap();
        assert_eq!(p, PathBuf::from("/explicit/path"));
    }
}
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod platform;`.

- [ ] **Step 3: Run tests**

Run: `cargo test platform`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src/platform.rs src/main.rs
git commit -m "Task 6: platform module with rime_user_dir + best-effort deploy"
```

---

## Task 7: GitHub release lookup + mirror fallback

**Files:**
- Create: `src/github.rs`
- Create: `tests/github_test.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/github.rs`**

```rust
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error(transparent)]
    Network(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

pub struct Github {
    client: Client,
    token: Option<String>,
    mirrors: Vec<String>,
}

impl Github {
    pub fn new(timeout_secs: u64, mirrors: Vec<String>, token: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(concat!("wxupd/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client, token, mirrors })
    }

    pub async fn latest_release(&self, repo: &str) -> Result<Release> {
        let base = format!("https://api.github.com/repos/{repo}/releases/latest");
        let urls = if self.mirrors.is_empty() {
            vec![base.clone()]
        } else {
            self.mirrors.iter().map(|m| format!("{}/{}", m.trim_end_matches('/'), base)).chain(std::iter::once(base)).collect()
        };
        let mut last_err: Option<anyhow::Error> = None;
        for url in urls {
            let mut req = self.client.get(&url).header("Accept", "application/vnd.github+json");
            if let Some(t) = &self.token {
                req = req.header("Authorization", format!("Bearer {t}"));
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp.json::<Release>().await.map_err(GithubError::from)?);
                    }
                    let body = resp.text().await.unwrap_or_default();
                    let err: anyhow::Error = match status.as_u16() {
                        403 if body.contains("rate limit") => GithubError::RateLimited.into(),
                        404 => GithubError::NotFound.into(),
                        s => GithubError::Http { status: s, body }.into(),
                    };
                    last_err = Some(err);
                }
                Err(e) => last_err = Some(anyhow!(GithubError::from(e))),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("no mirrors configured")))
    }
}

/// Mirror the download URL of an asset through the same chain.
pub fn rewrite_asset_url(url: &str, mirrors: &[String]) -> Vec<String> {
    let mut out: Vec<String> = mirrors.iter().map(|m| format!("{}/{}", m.trim_end_matches('/'), url)).collect();
    out.push(url.to_string());
    out
}
```

- [ ] **Step 2: Wire module**

Edit `src/main.rs` — add `mod github;`.

- [ ] **Step 3: Write integration test `tests/github_test.rs`**

```rust
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wxupd::github::Github;

#[tokio::test]
async fn picks_first_successful_mirror() {
    // Mirror server returns the release; api.github.com is unreachable in test
    // (we only point mirrors at the mock and never include the real base URL).
    let mirror = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v9.9",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{
            "name": "wanxiang-base-v9.9.zip",
            "browser_download_url": "https://example.com/a.zip",
            "size": 1234
        }]
    });
    Mock::given(method("GET"))
        .and(path("/https://api.github.com/repos/amzxyz/rime_wanxiang/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mirror)
        .await;

    let gh = Github::new(5, vec![mirror.uri()], None).unwrap();
    let rel = gh.latest_release("amzxyz/rime_wanxiang").await.unwrap();
    assert_eq!(rel.tag_name, "v9.9");
    assert_eq!(rel.assets.len(), 1);
}

#[tokio::test]
async fn falls_through_to_next_mirror_on_500() {
    let bad = MockServer::start().await;
    let good = MockServer::start().await;
    let body = serde_json::json!({
        "tag_name": "v1.0",
        "published_at": "2026-01-01T00:00:00Z",
        "assets": []
    });
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&bad).await;
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(&body)).mount(&good).await;

    let gh = Github::new(5, vec![bad.uri(), good.uri()], None).unwrap();
    let rel = gh.latest_release("foo/bar").await.unwrap();
    assert_eq!(rel.tag_name, "v1.0");
}
```

- [ ] **Step 4: Expose lib for integration tests**

The integration test imports `wxupd::github::Github`. Cargo's `tests/` consume the binary crate only via a library target. Add to `Cargo.toml`:

```toml
[lib]
path = "src/lib.rs"
```

Create `src/lib.rs`:

```rust
pub mod backup;
pub mod config;
pub mod github;
pub mod http;
pub mod manifest;
pub mod platform;
pub mod safe_list;
pub mod resource;
pub mod ops;
pub mod cli;
```

(We'll create `http.rs`, `resource/`, `ops/` in later tasks — for now the lib won't compile yet, so the next sub-step adds stubs.)

- [ ] **Step 5: Add empty module stubs so `src/lib.rs` compiles**

Create empty files (or `pub mod` declarations only, with no bodies that fail to compile):

- `src/http.rs` — write a single line: `// http module — implemented in Task 8`. Then remove the `pub mod http;` line from `src/lib.rs` for now to keep this task atomic; we'll re-add when http.rs gains content.

Cleaner: only declare modules in `lib.rs` that already exist with code:

```rust
pub mod backup;
pub mod cli;
pub mod config;
pub mod github;
pub mod manifest;
pub mod platform;
pub mod safe_list;
```

And `src/main.rs` becomes a thin wrapper that uses the lib:

```rust
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = wxupd::cli::Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&args.log))
        .with_writer(std::io::stderr)
        .init();
    match args.command {
        wxupd::cli::Command::Check { .. } => println!("check: not implemented yet"),
        wxupd::cli::Command::Update { .. } => println!("update: not implemented yet"),
        wxupd::cli::Command::Rollback { .. } => println!("rollback: not implemented yet"),
        wxupd::cli::Command::Config { .. } => println!("config: not implemented yet"),
    }
    Ok(())
}
```

Remove all `mod xyz;` lines from `main.rs`.

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: every test from Tasks 2-6 still passes, plus the two new `github_test.rs` tests.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs src/github.rs tests/github_test.rs
git commit -m "Task 7: GitHub client with mirror fallback + lib target"
```

---

## Task 8: HTTP download with streaming sha256

**Files:**
- Create: `src/http.rs`
- Create: `tests/http_test.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/http.rs`**

```rust
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

/// Download `url` to `out_path`. Streams body, computing sha256 in the same pass.
/// If `expected_sha256` is Some, the file is removed on mismatch and we return ChecksumMismatch.
pub async fn download(
    client: &Client,
    url: &str,
    out_path: &Path,
    expected_sha256: Option<&str>,
    show_progress: bool,
) -> Result<String> {
    if let Some(p) = out_path.parent() { tokio::fs::create_dir_all(p).await?; }
    let resp = client.get(url).send().await.with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} for {}", resp.status(), url));
    }
    let total = resp.content_length().unwrap_or(0);
    let pb = if show_progress && total > 0 {
        let pb = ProgressBar::new(total);
        pb.set_style(ProgressStyle::with_template(
            "{spinner} {bytes}/{total_bytes} [{wide_bar}] {eta}"
        ).unwrap());
        Some(pb)
    } else { None };

    let mut file = File::create(out_path).await.with_context(|| format!("create {}", out_path.display()))?;
    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        if let Some(pb) = &pb { pb.inc(chunk.len() as u64); }
    }
    file.flush().await?;
    if let Some(pb) = pb { pb.finish_and_clear(); }
    let actual = hex::encode(hasher.finalize());
    if let Some(expected) = expected_sha256 {
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = tokio::fs::remove_file(out_path).await;
            return Err(DownloadError::ChecksumMismatch { expected: expected.into(), actual }.into());
        }
    }
    Ok(actual)
}
```

- [ ] **Step 2: Add `pub mod http;` to `src/lib.rs`**

- [ ] **Step 3: Write `tests/http_test.rs`**

```rust
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wxupd::http::download;

#[tokio::test]
async fn downloads_and_returns_sha256() {
    let body = b"hello wxupd";
    let expected = "8b6a0a44c81e85e22ca9d4b13d4b18b9e1c97c5b6e6b29f6e7e6e3a45c3c3c40"; // recomputed below
    let srv = MockServer::start().await;
    Mock::given(method("GET")).and(path("/file.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&srv).await;

    // Compute the real expected hash so the test isn't brittle to my hand-typed value.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new(); h.update(body);
    let real_expected = hex::encode(h.finalize());

    let d = TempDir::new().unwrap();
    let out = d.path().join("dl.bin");
    let client = reqwest::Client::new();
    let actual = download(&client, &format!("{}/file.bin", srv.uri()), &out, None, false).await.unwrap();
    assert_eq!(actual, real_expected);
    assert_eq!(std::fs::read(&out).unwrap(), body);
    // Silence the unused placeholder.
    let _ = expected;
}

#[tokio::test]
async fn checksum_mismatch_deletes_file_and_errors() {
    let body = b"some bytes";
    let srv = MockServer::start().await;
    Mock::given(method("GET")).and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&srv).await;
    let d = TempDir::new().unwrap();
    let out = d.path().join("x");
    let client = reqwest::Client::new();
    let err = download(&client, &format!("{}/x", srv.uri()), &out, Some("00".repeat(32).as_str()), false).await.unwrap_err();
    assert!(err.to_string().contains("sha256 mismatch"));
    assert!(!out.exists());
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test http`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/http.rs src/lib.rs tests/http_test.rs
git commit -m "Task 8: streaming download with sha256 verification"
```

---

## Task 9: Resource trait + RemoteRef + InstallReport

**Files:**
- Create: `src/resource/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/resource/mod.rs`**

```rust
pub mod scheme;
pub mod gram;
pub mod dict;

use crate::config::Config;
use crate::github::{Asset, Github, Release};
use crate::safe_list::SafeList;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RemoteRef {
    pub tag: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
    pub sha256: Option<String>,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    pub files_written: Vec<PathBuf>,
    pub files_skipped: Vec<PathBuf>,
}

#[async_trait]
pub trait Resource: Send + Sync {
    fn id(&self) -> &'static str;
    fn repo(&self) -> &str;
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex>;
    async fn latest_remote(&self, gh: &Github, cfg: &Config) -> Result<RemoteRef> {
        let rel = gh.latest_release(self.repo()).await?;
        select_asset(&rel, &self.asset_pattern(cfg)?)
    }
    async fn install(&self, downloaded: &Path, rime_dir: &Path, safe: &SafeList) -> Result<InstallReport>;
}

pub fn select_asset(rel: &Release, pat: &Regex) -> Result<RemoteRef> {
    let asset: &Asset = rel.assets.iter().find(|a| pat.is_match(&a.name))
        .ok_or_else(|| anyhow!("no asset in tag {} matches {}", rel.tag_name, pat.as_str()))?;
    Ok(RemoteRef {
        tag: rel.tag_name.clone(),
        asset_name: asset.name.clone(),
        asset_url: asset.browser_download_url.clone(),
        asset_size: asset.size,
        sha256: None, // upstream rarely publishes; left None unless a sibling .sha256 asset is found
        published_at: rel.published_at,
    })
}

pub fn registry() -> Vec<Box<dyn Resource>> {
    vec![
        Box::new(scheme::SchemeResource),
        Box::new(gram::GramResource),
        Box::new(dict::DictResource),
    ]
}
```

- [ ] **Step 2: Add `async-trait` to deps**

Edit `Cargo.toml` `[dependencies]`:

```toml
async-trait = "0.1"
```

- [ ] **Step 3: Create stub resource impls so the module compiles**

`src/resource/scheme.rs`:

```rust
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
    fn id(&self) -> &'static str { "scheme" }
    fn repo(&self) -> &str { "amzxyz/rime_wanxiang" }
    fn asset_pattern(&self, cfg: &Config) -> Result<Regex> {
        let variant = if cfg.scheme.variant.is_empty() { "base" } else { cfg.scheme.variant.as_str() };
        Regex::new(&format!(r"^wanxiang-{}-.*\.zip$", regex::escape(variant)))
            .context("invalid scheme variant regex")
    }
    async fn install(&self, _downloaded: &Path, _rime_dir: &Path, _safe: &SafeList) -> Result<InstallReport> {
        anyhow::bail!("scheme install: implemented in Task 10")
    }
}
```

`src/resource/gram.rs`:

```rust
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
```

`src/resource/dict.rs`:

```rust
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
```

- [ ] **Step 4: Wire `pub mod resource;` in `src/lib.rs`**

- [ ] **Step 5: Write a unit test for `select_asset`**

Append at the bottom of `src/resource/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{Asset, Release};
    use chrono::Utc;

    fn rel(assets: Vec<(&str, u64)>) -> Release {
        Release {
            tag_name: "v1.0".into(),
            published_at: Utc::now(),
            assets: assets.into_iter().map(|(n, s)| Asset {
                name: n.into(),
                browser_download_url: format!("https://example.com/{n}"),
                size: s,
            }).collect(),
        }
    }

    #[test]
    fn select_asset_matches_pattern() {
        let r = rel(vec![("readme.txt", 10), ("wanxiang-base-v1.0.zip", 5000)]);
        let pat = Regex::new(r"^wanxiang-base-.*\.zip$").unwrap();
        let rr = select_asset(&r, &pat).unwrap();
        assert_eq!(rr.asset_name, "wanxiang-base-v1.0.zip");
        assert_eq!(rr.asset_size, 5000);
    }

    #[test]
    fn select_asset_fails_when_no_match() {
        let r = rel(vec![("readme.txt", 10)]);
        let pat = Regex::new(r"^wanxiang-base-.*\.zip$").unwrap();
        assert!(select_asset(&r, &pat).is_err());
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test resource`
Expected: 2 tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/lib.rs src/resource/
git commit -m "Task 9: Resource trait + stub impls + asset selection"
```

---

## Task 10: SchemeResource install (zip + SafeList)

**Files:**
- Modify: `src/resource/scheme.rs`
- Create: `tests/scheme_install_test.rs`
- Create: `tests/fixtures/scheme-fake.zip` (built by the test itself, no binary fixture committed)

- [ ] **Step 1: Implement install in `src/resource/scheme.rs`**

Replace the `install` body:

```rust
async fn install(&self, downloaded: &Path, rime_dir: &Path, safe: &SafeList) -> Result<InstallReport> {
    let downloaded = downloaded.to_path_buf();
    let rime_dir = rime_dir.to_path_buf();
    // Provide a clone of the GlobSet by re-asking the SafeList. Keep it cheap.
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
            let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
            if entry.is_dir() { continue; }
            if safe.is_protected(&rel) {
                report.files_skipped.push(rel);
                continue;
            }
            let out = rime_dir.join(&rel);
            if let Some(p) = out.parent() { std::fs::create_dir_all(p)?; }
            let mut out_f = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut out_f)?;
            report.files_written.push(rel);
        }
        Ok(report)
    }).await?
}
```

- [ ] **Step 2: Expose `patterns()` on `SafeList`**

Edit `src/safe_list.rs` — store the patterns alongside the GlobSet:

```rust
pub struct SafeList {
    patterns: Vec<String>,
    set: GlobSet,
}

impl SafeList {
    pub fn new(patterns: &[String]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        for p in patterns { b.add(Glob::new(p)?); }
        Ok(Self { patterns: patterns.to_vec(), set: b.build()? })
    }

    pub fn patterns(&self) -> &[String] { &self.patterns }
    // (keep defaults_plus and is_protected as before)
```

Existing tests still pass — the field is additive.

- [ ] **Step 3: Update scheme.rs imports**

Add at the top of `src/resource/scheme.rs`:

```rust
use anyhow::Context;
use crate::resource::InstallReport;
```

- [ ] **Step 4: Write `tests/scheme_install_test.rs`**

```rust
use std::fs;
use std::io::Write;
use tempfile::TempDir;
use wxupd::resource::scheme::SchemeResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

fn build_fake_zip(out: &std::path::Path) {
    let f = fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
    z.start_file("wanxiang.schema.yaml", opts).unwrap();
    z.write_all(b"schema: hi").unwrap();
    z.start_file("wanxiang.custom.yaml", opts).unwrap();
    z.write_all(b"keep me").unwrap();
    z.start_file("lua/sub.lua", opts).unwrap();
    z.write_all(b"-- lua").unwrap();
    z.finish().unwrap();
}

#[tokio::test]
async fn install_writes_files_and_skips_safelist() {
    let d = TempDir::new().unwrap();
    let zip = d.path().join("scheme.zip");
    build_fake_zip(&zip);
    let rime = d.path().join("rime");
    // Pre-existing user-customised file we must not clobber.
    fs::create_dir_all(&rime).unwrap();
    fs::write(rime.join("wanxiang.custom.yaml"), b"USER VERSION").unwrap();

    let safe = SafeList::defaults_plus(&[]).unwrap();
    let res = SchemeResource;
    let report = res.install(&zip, &rime, &safe).await.unwrap();

    assert!(report.files_written.iter().any(|p| p == std::path::Path::new("wanxiang.schema.yaml")));
    assert!(report.files_written.iter().any(|p| p == std::path::Path::new("lua/sub.lua")));
    assert!(report.files_skipped.iter().any(|p| p == std::path::Path::new("wanxiang.custom.yaml")));
    assert_eq!(fs::read(rime.join("wanxiang.custom.yaml")).unwrap(), b"USER VERSION");
    assert_eq!(fs::read(rime.join("wanxiang.schema.yaml")).unwrap(), b"schema: hi");
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test scheme`
Expected: 3 tests pass (2 from Task 9 + 1 new).

- [ ] **Step 6: Commit**

```bash
git add src/safe_list.rs src/resource/scheme.rs tests/scheme_install_test.rs
git commit -m "Task 10: scheme install via unzip with SafeList protection"
```

---

## Task 11: GramResource install (single file copy)

**Files:**
- Modify: `src/resource/gram.rs`
- Create: `tests/gram_install_test.rs`

- [ ] **Step 1: Implement install**

Replace `install` in `src/resource/gram.rs`:

```rust
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
```

- [ ] **Step 2: Write `tests/gram_install_test.rs`**

```rust
use std::fs;
use tempfile::TempDir;
use wxupd::resource::gram::GramResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

#[tokio::test]
async fn install_copies_single_file() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("dl.gram");
    fs::write(&src, b"\x00\x01\x02gram bytes").unwrap();
    let rime = d.path().join("rime");
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let report = GramResource.install(&src, &rime, &safe).await.unwrap();
    assert_eq!(report.files_written, vec![std::path::PathBuf::from("wanxiang-lts-zh-hans.gram")]);
    assert!(report.files_skipped.is_empty());
    assert_eq!(fs::read(rime.join("wanxiang-lts-zh-hans.gram")).unwrap(), b"\x00\x01\x02gram bytes");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test gram`
Expected: 1 test passes (plus Task-9 tests still pass).

- [ ] **Step 4: Commit**

```bash
git add src/resource/gram.rs tests/gram_install_test.rs
git commit -m "Task 11: gram install via single-file copy"
```

---

## Task 12: DictResource install (single yaml copy)

**Files:**
- Modify: `src/resource/dict.rs`
- Create: `tests/dict_install_test.rs`

- [ ] **Step 1: Implement install**

Replace `install` in `src/resource/dict.rs`. The dict asset is a single yaml — the asset's *name* on disk is the install path.

```rust
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
```

- [ ] **Step 2: Write `tests/dict_install_test.rs`**

```rust
use std::fs;
use tempfile::TempDir;
use wxupd::resource::dict::DictResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

#[tokio::test]
async fn install_copies_yaml() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("cn_en_mix.dict.yaml");
    fs::write(&src, b"# yaml").unwrap();
    let rime = d.path().join("rime");
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let r = DictResource.install(&src, &rime, &safe).await.unwrap();
    assert_eq!(r.files_written, vec![std::path::PathBuf::from("cn_en_mix.dict.yaml")]);
    assert_eq!(fs::read(rime.join("cn_en_mix.dict.yaml")).unwrap(), b"# yaml");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test dict`
Expected: 1 new test passes.

- [ ] **Step 4: Commit**

```bash
git add src/resource/dict.rs tests/dict_install_test.rs
git commit -m "Task 12: dict install via yaml copy"
```

---

## Task 13: `check` op + JSON output (with insta snapshot)

**Files:**
- Create: `src/ops/mod.rs`, `src/ops/check.rs`
- Modify: `src/lib.rs`, `src/main.rs`
- Create: `tests/check_test.rs`

- [ ] **Step 1: Write `src/ops/mod.rs`**

```rust
pub mod check;
pub mod update;
pub mod rollback;
```

- [ ] **Step 2: Write `src/ops/update.rs` and `src/ops/rollback.rs` as stubs**

`src/ops/update.rs`:

```rust
// Implemented in Task 14.
```

`src/ops/rollback.rs`:

```rust
// Implemented in Task 15.
```

- [ ] **Step 3: Write `src/ops/check.rs`**

```rust
use crate::config::Config;
use crate::github::Github;
use crate::manifest::Manifest;
use crate::resource::{registry, RemoteRef};
use anyhow::Result;
use futures_util::future::join_all;
use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    NotInstalled,
    UpToDate,
    HasUpdate,
    Error,
}

#[derive(Debug, Serialize, Clone)]
pub struct ResourceCheck {
    pub id: String,
    pub local_tag: Option<String>,
    pub remote_tag: Option<String>,
    pub status: Status,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CheckReport {
    pub resources: Vec<ResourceCheck>,
}

impl CheckReport {
    pub fn any_update(&self) -> bool {
        self.resources.iter().any(|r| r.status == Status::HasUpdate)
    }
    pub fn any_error(&self) -> bool {
        self.resources.iter().any(|r| r.status == Status::Error)
    }
}

pub async fn run(cfg: &Config, gh: &Github, manifest: &Manifest) -> Result<CheckReport> {
    let resources = registry();
    let futs = resources.into_iter().map(|res| async move {
        let id = res.id().to_string();
        let local_tag = manifest.resources.get(&id).map(|e| e.tag.clone());
        match res.latest_remote(gh, cfg).await {
            Ok(rr) => {
                let status = match &local_tag {
                    None => Status::NotInstalled,
                    Some(t) if t == &rr.tag => Status::UpToDate,
                    Some(_) => Status::HasUpdate,
                };
                let local_for_report = match status {
                    Status::NotInstalled => Some("-".to_string()),
                    _ => local_tag.clone(),
                };
                ResourceCheck { id, local_tag: local_for_report, remote_tag: Some(rr.tag), status, error: None }
            }
            Err(e) => ResourceCheck { id, local_tag, remote_tag: None, status: Status::Error, error: Some(e.to_string()) },
        }
    });
    let resources = join_all(futs).await;
    Ok(CheckReport { resources })
}

/// Stable text rendering used by the CLI when --json is not set.
pub fn render_text(rep: &CheckReport) -> String {
    let mut s = String::new();
    s.push_str("resource | local | remote | status\n");
    s.push_str("---------+-------+--------+--------\n");
    for r in &rep.resources {
        s.push_str(&format!(
            "{:<8} | {:<5} | {:<6} | {:?}\n",
            r.id,
            r.local_tag.as_deref().unwrap_or("-"),
            r.remote_tag.as_deref().unwrap_or("-"),
            r.status
        ));
        if let Some(e) = &r.error { s.push_str(&format!("  error: {e}\n")); }
    }
    s
}
```

- [ ] **Step 4: Add `pub mod ops;` to `src/lib.rs`**

- [ ] **Step 5: Wire `check` in `src/main.rs`**

Replace the `Command::Check` arm:

```rust
cli::Command::Check { json } => {
    let cfg_path = wxupd::config::config_path()?;
    let cfg = wxupd::config::Config::load(&cfg_path)?;
    let manifest_path = manifest_path()?;
    let manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
    let token = std::env::var("GITHUB_TOKEN").ok();
    let gh = wxupd::github::Github::new(cfg.network.timeout_secs, cfg.network.mirrors.clone(), token)?;
    let report = wxupd::ops::check::run(&cfg, &gh, &manifest).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", wxupd::ops::check::render_text(&report));
    }
    if report.any_update() { std::process::exit(10); }
    if report.any_error() { std::process::exit(1); }
}
```

And add a helper to the bottom of `main.rs`:

```rust
fn manifest_path() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_MANIFEST") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    Ok(dirs.data_dir().join("manifest.json"))
}
```

Add `directories` to main's imports.

- [ ] **Step 6: Write `tests/check_test.rs`** (end-to-end with wiremock + assert_cmd)

```rust
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn release_json(tag: &str, asset: &str) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [{ "name": asset, "browser_download_url": format!("https://x/{asset}"), "size": 100 }]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn check_reports_not_installed_for_empty_manifest() {
    let mirror = MockServer::start().await;
    // The catch-all matcher returns the same body for every release request;
    // good enough for this assertion since we only check exit code + status text.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("v1.0", "wanxiang-base-v1.0.zip")))
        .mount(&mirror)
        .await;

    let d = TempDir::new().unwrap();
    let cfg_path = d.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[scheme]\nvariant = \"pinyin\"\n\n[network]\nmirrors = [\"{}\"]\ntimeout_secs = 5\n",
            mirror.uri()
        ),
    )
    .unwrap();
    let manifest_path = d.path().join("manifest.json");

    let assert = Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg_path)
        .env("WXUPD_MANIFEST", &manifest_path)
        .args(["check", "--json"])
        .assert();

    // Exit 10 because at least one resource has-update (well, not-installed counts as
    // "newer than nothing" — but `any_update()` only flags has-update. With empty
    // manifest the status is not-installed for all 3, so exit code is 0.)
    assert.success().stdout(contains("\"id\": \"scheme\""));
}
```

(The exit-code semantics are deliberately documented inline: `not-installed` doesn't trigger exit 10. If desired, the CLI can be extended later. Keep this simple for now.)

- [ ] **Step 7: Run tests**

Run: `cargo test check`
Expected: tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/ops/ src/lib.rs src/main.rs tests/check_test.rs
git commit -m "Task 13: check op with text + JSON output"
```

---

## Task 14: `update` op (parallel download + serial install + atomicity)

**Files:**
- Modify: `src/ops/update.rs`
- Modify: `src/main.rs`
- Create: `tests/update_test.rs`

- [ ] **Step 1: Write `src/ops/update.rs`**

```rust
use crate::backup::{extract_tar_zst, write_tar_zst};
use crate::config::Config;
use crate::github::{rewrite_asset_url, Github};
use crate::http::download;
use crate::manifest::{HistoryEntry, Manifest, ResourceEntry, DEFAULT_HISTORY_KEEP};
use crate::ops::check::{self, Status};
use crate::platform;
use crate::resource::{registry, InstallReport, RemoteRef, Resource};
use crate::safe_list::SafeList;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use futures_util::future::try_join_all;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

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
        if all.is_empty() { anyhow::bail!("no matching resources for {:?}", args.only); }
    }

    // 1. Discover remotes.
    let check_report = check::run(cfg, gh, manifest).await?;

    // 2. Filter to has-update / not-installed (unless --force).
    let mut targets: Vec<(Box<dyn Resource>, RemoteRef)> = Vec::new();
    for res in all {
        let rep = check_report.resources.iter().find(|r| r.id == res.id());
        let Some(rep) = rep else { continue };
        let needs = matches!(rep.status, Status::HasUpdate | Status::NotInstalled) || args.force;
        if !needs { continue; }
        if rep.status == Status::Error {
            return Err(anyhow!("cannot update {}: {}", rep.id, rep.error.clone().unwrap_or_default()));
        }
        // Re-fetch remote ref to get the full struct (check returned a summary).
        let rr = res.latest_remote(gh, cfg).await?;
        targets.push((res, rr));
    }
    if targets.is_empty() {
        return Ok(UpdateResult { installed: vec![], skipped_protected: vec![], failures: vec![] });
    }

    // 3. Parallel download.
    let staging = cache_dir.join("staging");
    std::fs::create_dir_all(&staging)?;
    let client = reqwest::Client::builder().user_agent(concat!("wxupd/", env!("CARGO_PKG_VERSION"))).build()?;
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
    let downloads = try_join_all(dl_futs).await?;

    // 4. Serial install.
    let safe = SafeList::defaults_plus(&cfg.safe_list.extra)?;
    let backups_dir = data_dir.join("backups");
    let mut result = UpdateResult { installed: vec![], skipped_protected: vec![], failures: vec![] };

    for ((res, _rr_target), (id, downloaded, sha, rr)) in targets.iter().zip(downloads.iter()) {
        let id = id.clone();
        // Backup the prior state of files this install will touch (if any).
        let prior_files = manifest.resources.get(&id).map(|e| e.files_installed.clone()).unwrap_or_default();
        let prior_paths: Vec<PathBuf> = prior_files.iter().map(PathBuf::from).collect();
        let backup_path = if !prior_paths.is_empty() {
            let prior_tag = manifest.resources[&id].tag.clone();
            let p = backups_dir.join(&id).join(format!("{}.tar.zst", prior_tag));
            write_tar_zst(rime_dir, &prior_paths, &p)?;
            Some(p)
        } else { None };

        // Install.
        match res.install(downloaded, rime_dir, &safe).await {
            Ok(InstallReport { files_written, files_skipped }) => {
                if !files_skipped.is_empty() {
                    result.skipped_protected.push((id.clone(), files_skipped));
                }
                let old_tag = manifest.resources.get(&id).map(|e| e.tag.clone()).unwrap_or_else(|| "-".to_string());
                let entry = ResourceEntry {
                    tag: rr.tag.clone(),
                    asset_name: rr.asset_name.clone(),
                    sha256: sha.clone(),
                    installed_at: Utc::now(),
                    files_installed: files_written.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
                    history: vec![],
                };
                let pruned = manifest.promote(&id, entry, backup_path, DEFAULT_HISTORY_KEEP);
                for p in pruned { let _ = std::fs::remove_file(p); }
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
```

- [ ] **Step 2: Wire in `src/main.rs`**

Add an `Update` arm:

```rust
cli::Command::Update { resources, no_deploy, force } => {
    let cfg_path = wxupd::config::config_path()?;
    let cfg = wxupd::config::Config::load(&cfg_path)?;
    let manifest_path = manifest_path()?;
    let mut manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
    let token = std::env::var("GITHUB_TOKEN").ok();
    let gh = wxupd::github::Github::new(cfg.network.timeout_secs, cfg.network.mirrors.clone(), token)?;
    let cache_dir = cache_dir()?;
    let data_dir = data_dir()?;
    let rime_dir = wxupd::platform::rime_user_dir(&cfg.paths.rime_user_dir)?;
    let result = wxupd::ops::update::run(
        &cfg, &gh, &mut manifest, &manifest_path,
        &cache_dir, &data_dir, &rime_dir,
        wxupd::ops::update::UpdateArgs { only: resources, force, no_deploy }
    ).await?;
    if result.installed.is_empty() && result.failures.is_empty() {
        println!("all up-to-date");
    } else {
        for (id, old, new) in &result.installed { println!("{id}: {old} -> {new}"); }
        for (id, skipped) in &result.skipped_protected {
            println!("{id}: skipped {} protected file(s)", skipped.len());
        }
        for (id, err) in &result.failures {
            eprintln!("{id}: FAILED ({err})");
        }
    }
    if !result.failures.is_empty() {
        std::process::exit(3);
    }
}
```

Add helpers next to `manifest_path()`:

```rust
fn cache_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_CACHE") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd").ok_or_else(|| anyhow::anyhow!("no home"))?;
    Ok(dirs.cache_dir().to_path_buf())
}
fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_DATA") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd").ok_or_else(|| anyhow::anyhow!("no home"))?;
    Ok(dirs.data_dir().to_path_buf())
}
```

- [ ] **Step 3: Write `tests/update_test.rs`**

```rust
use assert_cmd::Command;
use std::io::Write;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn release_json(tag: &str, assets: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": assets.iter().map(|(n, u)| serde_json::json!({
            "name": n, "browser_download_url": u, "size": 100
        })).collect::<Vec<_>>()
    })
}

fn build_fake_zip(out: &std::path::Path) {
    let f = std::fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
    z.start_file("wanxiang.schema.yaml", opts).unwrap();
    z.write_all(b"v1").unwrap();
    z.finish().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn update_installs_scheme_and_writes_manifest() {
    let mirror = MockServer::start().await;
    // Build a fake zip we'll serve as the asset.
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("wanxiang-base-v1.zip");
    build_fake_zip(&zip_path);
    let zip_bytes = std::fs::read(&zip_path).unwrap();

    // Mock release endpoints (one per repo); also serve the asset bytes.
    let asset_url = format!("{}/dl/wanxiang-base-v1.zip", mirror.uri());
    Mock::given(method("GET"))
        .and(path_regex(r".*amzxyz/rime_wanxiang/releases/latest$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("v1", &[("wanxiang-base-v1.zip", &asset_url)])))
        .mount(&mirror).await;
    Mock::given(method("GET"))
        .and(path_regex(r".*amzxyz/RIME-LMDG/releases/latest$"))
        .respond_with(ResponseTemplate::new(500))  // gram release "missing" — surfaces as error
        .mount(&mirror).await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/dl/wanxiang-base-v1\.zip$"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_bytes))
        .mount(&mirror).await;

    let d = TempDir::new().unwrap();
    let cfg_path = d.path().join("config.toml");
    std::fs::write(&cfg_path, format!(
        "[scheme]\nvariant = \"pinyin\"\n[paths]\nrime_user_dir = \"{}\"\n\
         [network]\nmirrors = [\"{}\"]\ntimeout_secs = 5\n[deploy]\nauto = false\n",
        d.path().join("rime").display(), mirror.uri()
    )).unwrap();
    let manifest_path = d.path().join("manifest.json");

    // We only ask for the scheme to side-step the failing gram mock.
    let assert = Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg_path)
        .env("WXUPD_MANIFEST", &manifest_path)
        .env("WXUPD_CACHE", d.path().join("cache"))
        .env("WXUPD_DATA", d.path().join("data"))
        .args(["update", "scheme"])
        .assert();
    assert.success();

    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(m["resources"]["scheme"]["tag"], "v1");
    let installed = d.path().join("rime/wanxiang.schema.yaml");
    assert_eq!(std::fs::read(&installed).unwrap(), b"v1");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test update`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/ops/update.rs src/main.rs tests/update_test.rs
git commit -m "Task 14: update op with parallel download + serial install + per-resource backup"
```

---

## Task 15: `rollback` op

**Files:**
- Modify: `src/ops/rollback.rs`
- Modify: `src/main.rs`
- Create: `tests/rollback_test.rs`

- [ ] **Step 1: Write `src/ops/rollback.rs`**

```rust
use crate::backup::extract_tar_zst;
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
            backup: prev.backup.clone(), // reuse the same tar — it represents the *old* (now-current) state
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
```

- [ ] **Step 2: Wire in `src/main.rs`**

```rust
cli::Command::Rollback { resources, no_deploy } => {
    let cfg_path = wxupd::config::config_path()?;
    let cfg = wxupd::config::Config::load(&cfg_path)?;
    let manifest_path = manifest_path()?;
    let mut manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
    let rime_dir = wxupd::platform::rime_user_dir(&cfg.paths.rime_user_dir)?;
    let outcome = wxupd::ops::rollback::run(
        &cfg, &mut manifest, &manifest_path, &rime_dir,
        wxupd::ops::rollback::RollbackArgs { only: resources, no_deploy },
    ).await?;
    for (id, from, to) in &outcome.rolled_back { println!("{id}: {from} -> {to}"); }
    for (id, reason) in &outcome.skipped { println!("{id}: skipped ({reason})"); }
}
```

- [ ] **Step 3: Write `tests/rollback_test.rs`**

```rust
use chrono::Utc;
use std::collections::BTreeMap;
use tempfile::TempDir;
use wxupd::backup::write_tar_zst;
use wxupd::config::Config;
use wxupd::manifest::{HistoryEntry, Manifest, ResourceEntry};
use wxupd::ops::rollback::{run, RollbackArgs};

#[tokio::test]
async fn rolls_back_to_previous_and_removes_new_files() {
    let d = TempDir::new().unwrap();
    let rime = d.path().join("rime");
    std::fs::create_dir_all(&rime).unwrap();
    // Pretend v2 is currently installed: it added "v2-only.yaml" and modified "shared.yaml".
    std::fs::write(rime.join("v2-only.yaml"), b"V2 added me").unwrap();
    std::fs::write(rime.join("shared.yaml"), b"V2 contents").unwrap();

    // Backup tar.zst captures the v1 state of shared.yaml.
    let stash = d.path().join("stash"); std::fs::create_dir_all(&stash).unwrap();
    std::fs::write(stash.join("shared.yaml"), b"V1 contents").unwrap();
    let backup_path = d.path().join("backups/scheme/v1.tar.zst");
    write_tar_zst(&stash, &[std::path::PathBuf::from("shared.yaml")], &backup_path).unwrap();

    let mut resources = BTreeMap::new();
    resources.insert("scheme".into(), ResourceEntry {
        tag: "v2".into(),
        asset_name: "wanxiang-base-v2.zip".into(),
        sha256: "x".into(),
        installed_at: Utc::now(),
        files_installed: vec!["v2-only.yaml".into(), "shared.yaml".into()],
        history: vec![HistoryEntry {
            tag: "v1".into(),
            asset_name: "wanxiang-base-v1.zip".into(),
            sha256: "y".into(),
            backup: backup_path.clone(),
            installed_at: Utc::now(),
            files_installed: vec!["shared.yaml".into()],
        }],
    });
    let mut manifest = Manifest { schema_version: 1, resources };
    let manifest_path = d.path().join("manifest.json");
    manifest.save(&manifest_path).unwrap();

    let cfg = Config { deploy: wxupd::config::DeployCfg { auto: false }, ..Config::default() };
    let outcome = run(&cfg, &mut manifest, &manifest_path, &rime, RollbackArgs { only: vec!["scheme".into()], no_deploy: true }).await.unwrap();

    assert_eq!(outcome.rolled_back, vec![("scheme".into(), "v2".into(), "v1".into())]);
    assert!(!rime.join("v2-only.yaml").exists(), "v2-only.yaml should be deleted");
    assert_eq!(std::fs::read(rime.join("shared.yaml")).unwrap(), b"V1 contents");
    assert_eq!(manifest.resources["scheme"].tag, "v1");
    assert_eq!(manifest.resources["scheme"].history[0].tag, "v2");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test rollback`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/ops/rollback.rs src/main.rs tests/rollback_test.rs
git commit -m "Task 15: rollback op with added-file cleanup and reversibility"
```

---

## Task 16: `config` subcommand

**Files:**
- Modify: `src/main.rs`
- Create: `tests/config_cmd_test.rs`

- [ ] **Step 1: Wire `Command::Config` in `src/main.rs`**

```rust
cli::Command::Config { action } => match action {
    cli::ConfigAction::Show => {
        let cfg_path = wxupd::config::config_path()?;
        if cfg_path.exists() {
            print!("{}", std::fs::read_to_string(&cfg_path)?);
        } else {
            println!("# no config.toml yet at {}", cfg_path.display());
        }
    }
    cli::ConfigAction::Set { kv } => {
        let (k, v) = kv.split_once('=').ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE"))?;
        let cfg_path = wxupd::config::config_path()?;
        wxupd::config::Config::set_dotted(&cfg_path, k.trim(), v.trim())?;
        println!("set {k} = {v}");
    }
},
```

- [ ] **Step 2: Write `tests/config_cmd_test.rs`**

```rust
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

#[test]
fn config_set_then_show() {
    let d = TempDir::new().unwrap();
    let cfg = d.path().join("config.toml");

    Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg)
        .args(["config", "set", "scheme.variant=flypy"])
        .assert().success().stdout(contains("set scheme.variant = flypy"));

    Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg)
        .args(["config", "show"])
        .assert().success().stdout(contains("variant = \"flypy\""));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test config_cmd`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs tests/config_cmd_test.rs
git commit -m "Task 16: config show/set subcommand"
```

---

## Task 17: GitHub Actions CI (`test.yml`)

**Files:**
- Create: `.github/workflows/test.yml`

- [ ] **Step 1: Write `.github/workflows/test.yml`**

```yaml
name: test
on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: fmt
        run: cargo fmt --check
      - name: clippy
        run: cargo clippy --all-targets -- -D warnings
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - name: test
        run: cargo test --all-targets
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          RUST_BACKTRACE: 1
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/test.yml
git commit -m "Task 17: CI workflow for fmt + clippy + test on 3 platforms"
```

---

## Task 18: Release workflow (`release.yml`)

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Write `.github/workflows/release.yml`**

```yaml
name: release
on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            ext: ''
          - target: x86_64-apple-darwin
            os: macos-13
            ext: ''
          - target: aarch64-apple-darwin
            os: macos-latest
            ext: ''
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            ext: '.exe'
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: install musl tooling
        if: matrix.target == 'x86_64-unknown-linux-musl'
        run: sudo apt-get update && sudo apt-get install -y musl-tools
      - uses: Swatinem/rust-cache@v2
      - name: build
        run: cargo build --release --target ${{ matrix.target }}
      - name: package
        shell: bash
        run: |
          mkdir -p dist
          name="wxupd-${{ github.ref_name }}-${{ matrix.target }}"
          cp target/${{ matrix.target }}/release/wxupd${{ matrix.ext }} "dist/${name}${{ matrix.ext }}"
      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target }}
          path: dist/*

  publish:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true
      - name: release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "${{ github.ref_name }}" \
            --repo "${{ github.repository }}" \
            --title "${{ github.ref_name }}" \
            --notes "Auto-built release for ${{ github.ref_name }}" \
            dist/*
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "Task 18: tag-driven release workflow for 4 targets"
```

---

## Task 19: README + LICENSE (minimal)

**Files:**
- Create: `README.md`
- Create: `LICENSE` (MIT)

- [ ] **Step 1: Write `README.md`**

```markdown
# wxupd — rime-wanxiang updater

A cross-platform CLI to keep [rime-wanxiang](https://github.com/amzxyz/rime_wanxiang) scheme files, the LTS gram model, and supporting dictionaries up to date.

## Install

Download a binary from the [latest release](../../releases/latest), or build from source:

    cargo install --path .

## Quick start

    wxupd config set scheme.variant=pinyin   # or flypy, zrm, mspy, ...
    wxupd check
    wxupd update
    wxupd rollback           # if something looks off

## Network

If `github.com` is slow or blocked, configure mirrors:

    wxupd config set network.mirrors="https://ghfast.top,https://ghproxy.com"

A personal access token (PAT) lifts the anonymous rate limit:

    export GITHUB_TOKEN=ghp_...
    wxupd check

## Subcommands

| Command | Description |
|---|---|
| `wxupd check [--json]` | Compare local manifest vs upstream releases |
| `wxupd update [--no-deploy] [--force] [scheme\|gram\|dict ...]` | Download + install + (optional) deploy |
| `wxupd rollback [scheme\|gram\|dict ...]` | Restore the previous installed version |
| `wxupd config show / set KEY=VALUE` | Inspect or modify `config.toml` |

## Exit codes

`0` success • `1` generic error • `2` network/download • `3` install • `10` `check` found updates • `130` interrupted.

## Safe list

Files matching the safe list are never overwritten by `update`. Defaults:

```
*.custom.yaml, installation.yaml, user.yaml,
*.userdb*, *.userdb.txt, sync/**, build/**
```

Extend via `[safe_list].extra` in `config.toml`.

## License

MIT
```

- [ ] **Step 2: Write `LICENSE`**

```
MIT License

Copyright (c) 2026 wangkezun

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Commit**

```bash
git add README.md LICENSE
git commit -m "Task 19: README + MIT license"
```

---

## Open implementation-time decisions (carried from spec §12)

Before Task 10/11/12 actually go live against real upstream, the implementer must:

1. **Open `https://github.com/amzxyz/rime_wanxiang/releases/latest`** and read the asset list. Codify the exact regex per variant in `SchemeResource::asset_pattern` and add a `match` over a closed set of known variants if the actual asset names don't share a common prefix.
2. **Locate the canonical gram release repo.** The spec assumes `amzxyz/RIME-LMDG`; if that's wrong, update `GramResource::repo()` and the test mocks.
3. **Locate the canonical dict release.** It may be a separate yaml in the same release or live in its own repo; update `DictResource::repo()` and `asset_pattern()` accordingly.
4. **Verify Squirrel's reload CLI on the current macOS.** The current spec uses `Squirrel.app/Contents/MacOS/Squirrel --reload`; if that's not correct on the user's macOS version, the right invocation may be via `osascript` calling Squirrel's URL handler, or `pkill -USR1 Squirrel` to nudge a redeploy.

These don't block earlier tasks — Tasks 1-9 build the foundation, Tasks 10-12 codify the upstream contract.

---

## Plan self-review

- **Spec coverage**: §3 CLI surface → Tasks 1, 13-16 + §4 architecture (modules) → Tasks 2-12 + §5 state files → Tasks 3-4 + §6 ops → Tasks 13-15 + §7 platform → Task 6 + §8 error handling → woven in (anyhow/thiserror, exit codes wired in Task 13/14) + §9 testing → wiremock+assert_cmd throughout + §10 CI/Release → Tasks 17-18.
- **Placeholders**: none — every code block is concrete; the open questions are explicitly carried to a dedicated section, not hidden as `TODO` in code.
- **Type consistency**:
  - `ResourceEntry.files_installed` is `Vec<String>` (manifest) but `InstallReport.files_written` is `Vec<PathBuf>`. Task 14 explicitly converts via `to_string_lossy().into_owned()`. ✅
  - `HistoryEntry` carries `files_installed` — confirmed used in Task 15 rollback to diff added files. ✅
  - `Resource` trait method names are stable across tasks (`id`, `repo`, `asset_pattern`, `latest_remote`, `install`). ✅
  - `Github::latest_release` returns `Release` (not `RemoteRef`); `Resource::latest_remote` adapts via `select_asset`. ✅
- **Exit codes**: Task 13 emits `10` for has-update; Task 14 emits `3` for install failure. Spec §3 also names `2` for network — that exit code is currently only implicit (any reqwest error bubbles up as generic error → 1). If desired this can be added later by mapping `GithubError` / `DownloadError` to a `2` exit explicitly; intentionally deferred to keep the initial cut small.
