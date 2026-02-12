use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mountaineer", about = "SMB share favorites manager", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List currently mounted SMB shares with connection details
    List,
    /// Show saved favorites with their current status
    Favorites,
    /// Add a currently mounted share to favorites
    Add {
        /// Share name (e.g. CORE-01)
        share: String,
        /// MAC address for Wake-on-LAN (e.g. d0:11:e5:13:af:1f)
        #[arg(long)]
        mac: Option<String>,
    },
    /// Remove a share from favorites
    Remove {
        /// Share name to remove
        share: String,
    },
    /// Mount all favorites (or a specific one)
    Mount {
        /// Specific share name to mount (omit for all)
        share: Option<String>,
    },
    /// Unmount a specific share
    Unmount {
        /// Share name to unmount
        share: String,
    },
    /// Show detailed status of all favorites
    Status,
    /// Send Wake-on-LAN packet to a server
    Wake {
        /// Share name (MAC address looked up from config)
        share: String,
    },
    /// Watch mode: auto-mount favorites, remount on network changes
    Watch,
    /// Install LaunchAgent to start Mountaineer at login
    Install,
    /// Uninstall LaunchAgent (stop starting at login)
    Uninstall,
}
