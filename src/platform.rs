use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::warn;

pub fn rime_user_dir(override_path: &str) -> Result<PathBuf> {
    if !override_path.is_empty() {
        return Ok(PathBuf::from(override_path));
    }
    let home = directories::BaseDirs::new()
        .context("no home dir")?
        .home_dir()
        .to_path_buf();
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
        if c.exists() {
            return Ok(c.clone());
        }
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
