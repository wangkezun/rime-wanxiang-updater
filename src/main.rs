use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = wxupd::cli::Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&args.log))
        .with_writer(std::io::stderr)
        .init();
    match args.command {
        wxupd::cli::Command::Check { json } => {
            let cfg_path = wxupd::config::config_path()?;
            let cfg = wxupd::config::Config::load(&cfg_path)?;
            let manifest_path = manifest_path()?;
            let manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let gh = wxupd::github::Github::new(
                cfg.network.timeout_secs,
                cfg.network.mirrors.clone(),
                token,
            )?;
            let report = wxupd::ops::check::run(&cfg, &gh, &manifest).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", wxupd::ops::check::render_text(&report));
            }
            if report.any_update() {
                std::process::exit(10);
            }
            if report.any_error() {
                std::process::exit(1);
            }
        }
        wxupd::cli::Command::Update {
            resources,
            no_deploy,
            force,
        } => {
            let cfg_path = wxupd::config::config_path()?;
            let cfg = wxupd::config::Config::load(&cfg_path)?;
            let manifest_path = manifest_path()?;
            let mut manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let gh = wxupd::github::Github::new(
                cfg.network.timeout_secs,
                cfg.network.mirrors.clone(),
                token,
            )?;
            let cache_dir = cache_dir()?;
            let data_dir = data_dir()?;
            let rime_dir = wxupd::platform::rime_user_dir(&cfg.paths.rime_user_dir)?;
            let result = wxupd::ops::update::run(
                &cfg,
                &gh,
                &mut manifest,
                &manifest_path,
                &cache_dir,
                &data_dir,
                &rime_dir,
                wxupd::ops::update::UpdateArgs {
                    only: resources,
                    force,
                    no_deploy,
                },
            )
            .await?;
            if result.installed.is_empty() && result.failures.is_empty() {
                println!("all up-to-date");
            } else {
                for (id, old, new) in &result.installed {
                    println!("{id}: {old} -> {new}");
                }
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
        wxupd::cli::Command::Rollback {
            resources,
            no_deploy,
        } => {
            let cfg_path = wxupd::config::config_path()?;
            let cfg = wxupd::config::Config::load(&cfg_path)?;
            let manifest_path = manifest_path()?;
            let mut manifest = wxupd::manifest::Manifest::load(&manifest_path)?;
            let data_dir = data_dir()?;
            let rime_dir = wxupd::platform::rime_user_dir(&cfg.paths.rime_user_dir)?;
            let outcome = wxupd::ops::rollback::run(
                &cfg,
                &mut manifest,
                &manifest_path,
                &data_dir,
                &rime_dir,
                wxupd::ops::rollback::RollbackArgs {
                    only: resources,
                    no_deploy,
                },
            )
            .await?;
            for (id, from, to) in &outcome.rolled_back {
                println!("{id}: {from} -> {to}");
            }
            for (id, reason) in &outcome.skipped {
                println!("{id}: skipped ({reason})");
            }
        }
        wxupd::cli::Command::Config { action } => match action {
            wxupd::cli::ConfigAction::Show => {
                let cfg_path = wxupd::config::config_path()?;
                if cfg_path.exists() {
                    print!("{}", std::fs::read_to_string(&cfg_path)?);
                } else {
                    println!("# no config.toml yet at {}", cfg_path.display());
                }
            }
            wxupd::cli::ConfigAction::Set { kv } => {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE"))?;
                let cfg_path = wxupd::config::config_path()?;
                wxupd::config::Config::set_dotted(&cfg_path, k.trim(), v.trim())?;
                println!("set {k} = {v}");
            }
        },
    }
    Ok(())
}

fn cache_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_CACHE") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .ok_or_else(|| anyhow::anyhow!("no home"))?;
    Ok(dirs.cache_dir().to_path_buf())
}

fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_DATA") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .ok_or_else(|| anyhow::anyhow!("no home"))?;
    Ok(dirs.data_dir().to_path_buf())
}

fn manifest_path() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_MANIFEST") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    Ok(dirs.data_dir().join("manifest.json"))
}
