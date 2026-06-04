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
