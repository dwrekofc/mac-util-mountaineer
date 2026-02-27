use clap::{Args, Parser, Subcommand};

use crate::config::Backend;

#[derive(Parser)]
#[command(
    name = "mountaineer",
    about = "Mountaineer V2 SMB failover manager",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Single reconciliation pass for all configured shares
    Reconcile {
        #[arg(long)]
        all: bool,
    },
    /// Continuous monitor loop with periodic reconcile
    Monitor {
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Show share status and active backend
    Status {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Manual backend switch for one share
    Switch {
        #[arg(long)]
        share: String,
        #[arg(long)]
        to: Backend,
    },
    /// Health and mountpoint checks only
    Verify {
        #[command(flatten)]
        target: MultiShareTarget,
        #[arg(long)]
        json: bool,
    },
    /// Mount/load all managed favorite drives
    Mount {
        #[arg(long)]
        all: bool,
    },
    /// Unmount/unload all managed favorite drives
    Unmount {
        #[arg(long)]
        all: bool,
    },
    /// List folders via stable share path
    Folders {
        #[arg(long)]
        share: String,
        #[arg(long)]
        subpath: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Alias {
        #[command(subcommand)]
        command: AliasCommand,
    },
    Favorites {
        #[command(subcommand)]
        command: FavoritesCommand,
    },
    /// Install LaunchAgent to start Mountaineer at login
    Install,
    /// Remove LaunchAgent
    Uninstall,
}

#[derive(Debug, Clone, Args)]
pub struct MultiShareTarget {
    #[arg(long, conflicts_with = "share")]
    pub all: bool,
    #[arg(long)]
    pub share: Option<String>,
}

#[derive(Subcommand)]
pub enum AliasCommand {
    /// Create a managed alias for a subfolder under a stable share path
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        share: String,
        #[arg(long)]
        target_subpath: String,
        #[arg(long)]
        alias_path: Option<String>,
    },
    /// List configured aliases and their health
    List {
        #[arg(long)]
        json: bool,
    },
    /// Remove managed alias by name
    Remove {
        #[arg(long)]
        name: String,
    },
    /// Validate and repair aliases
    Reconcile {
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum FavoritesCommand {
    /// Add or update a managed drive favorite
    Add {
        #[arg(long)]
        share: String,
        #[arg(long = "tb-host")]
        tb_host: String,
        #[arg(long = "fallback-host")]
        fallback_host: String,
        #[arg(long)]
        username: String,
        #[arg(long = "remote-share")]
        remote_share: Option<String>,
    },
    /// Remove a managed favorite
    Remove {
        #[arg(long)]
        share: String,
        #[arg(long)]
        cleanup: bool,
    },
    /// List managed favorites
    List {
        #[arg(long)]
        json: bool,
    },
}
