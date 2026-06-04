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
