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
        wxupd::cli::Command::Update { .. } => println!("update: not implemented yet"),
        wxupd::cli::Command::Rollback { .. } => println!("rollback: not implemented yet"),
        wxupd::cli::Command::Config { .. } => println!("config: not implemented yet"),
    }
    Ok(())
}

fn manifest_path() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("WXUPD_MANIFEST") {
        return Ok(std::path::PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("io", "wkz", "wxupd")
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    Ok(dirs.data_dir().join("manifest.json"))
}
