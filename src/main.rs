mod cli;
mod config;
mod manifest;
mod safe_list;

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
