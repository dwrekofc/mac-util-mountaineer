use clap::{Args, Parser, Subcommand};

use crate::config::Backend;

#[derive(Debug, Parser)]
#[command(
    name = "mountaineer",
    about = "Mountaineer V2 SMB failover manager",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
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
        /// Force switch even if files are open on the current mount
        #[arg(long)]
        force: bool,
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
        /// Force unmount even if files are open
        #[arg(long)]
        force: bool,
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
    /// View or modify configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Install LaunchAgent to start Mountaineer at login
    Install,
    /// Remove LaunchAgent
    Uninstall,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Set a configuration value
    Set {
        /// Configuration key (lsof-recheck, auto-failback, check-interval, connect-timeout)
        key: String,
        /// Configuration value (on/off for toggles, number for intervals)
        value: String,
    },
    /// Show current configuration
    Show,
}

#[derive(Debug, Clone, Args)]
pub struct MultiShareTarget {
    #[arg(long, conflicts_with = "share")]
    pub all: bool,
    #[arg(long)]
    pub share: Option<String>,
}

#[derive(Debug, Subcommand)]
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

#[derive(Debug, Subcommand)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Helper to parse CLI args, prefixing with the binary name.
    fn parse(args: &[&str]) -> Cli {
        let mut full_args = vec!["mountaineer"];
        full_args.extend(args);
        Cli::try_parse_from(full_args).unwrap()
    }

    fn parse_err(args: &[&str]) -> clap::Error {
        let mut full_args = vec!["mountaineer"];
        full_args.extend(args);
        Cli::try_parse_from(full_args).unwrap_err()
    }

    // --- No subcommand (GUI mode) ---

    #[test]
    fn no_subcommand_yields_none() {
        let cli = parse(&[]);
        assert!(cli.command.is_none());
    }

    // --- Reconcile ---

    #[test]
    fn reconcile_all() {
        let cli = parse(&["reconcile", "--all"]);
        match cli.command.unwrap() {
            Command::Reconcile { all } => assert!(all),
            other => panic!("expected Reconcile, got {:?}", other),
        }
    }

    // --- Monitor ---

    #[test]
    fn monitor_with_interval() {
        let cli = parse(&["monitor", "--interval", "5"]);
        match cli.command.unwrap() {
            Command::Monitor { interval } => assert_eq!(interval, Some(5)),
            other => panic!("expected Monitor, got {:?}", other),
        }
    }

    #[test]
    fn monitor_without_interval() {
        let cli = parse(&["monitor"]);
        match cli.command.unwrap() {
            Command::Monitor { interval } => assert_eq!(interval, None),
            other => panic!("expected Monitor, got {:?}", other),
        }
    }

    // --- Status ---

    #[test]
    fn status_all_json() {
        let cli = parse(&["status", "--all", "--json"]);
        match cli.command.unwrap() {
            Command::Status { all, json } => {
                assert!(all);
                assert!(json);
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn status_defaults() {
        let cli = parse(&["status"]);
        match cli.command.unwrap() {
            Command::Status { all, json } => {
                assert!(!all);
                assert!(!json);
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    // --- Switch ---

    #[test]
    fn switch_to_tb() {
        let cli = parse(&["switch", "--share", "CORE", "--to", "tb"]);
        match cli.command.unwrap() {
            Command::Switch { share, to, force } => {
                assert_eq!(share, "CORE");
                assert_eq!(to, Backend::Tb);
                assert!(!force);
            }
            other => panic!("expected Switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_to_fallback_force() {
        let cli = parse(&["switch", "--share", "DATA", "--to", "fallback", "--force"]);
        match cli.command.unwrap() {
            Command::Switch { share, to, force } => {
                assert_eq!(share, "DATA");
                assert_eq!(to, Backend::Fallback);
                assert!(force);
            }
            other => panic!("expected Switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_requires_share_and_to() {
        // Missing --share should fail
        let _ = parse_err(&["switch", "--to", "tb"]);
    }

    // --- Verify ---

    #[test]
    fn verify_all() {
        let cli = parse(&["verify", "--all"]);
        match cli.command.unwrap() {
            Command::Verify { target, json } => {
                assert!(target.all);
                assert!(target.share.is_none());
                assert!(!json);
            }
            other => panic!("expected Verify, got {:?}", other),
        }
    }

    #[test]
    fn verify_single_share() {
        let cli = parse(&["verify", "--share", "CORE", "--json"]);
        match cli.command.unwrap() {
            Command::Verify { target, json } => {
                assert!(!target.all);
                assert_eq!(target.share.as_deref(), Some("CORE"));
                assert!(json);
            }
            other => panic!("expected Verify, got {:?}", other),
        }
    }

    #[test]
    fn verify_all_and_share_conflict() {
        // --all and --share are mutually exclusive per MultiShareTarget
        let _ = parse_err(&["verify", "--all", "--share", "CORE"]);
    }

    // --- Mount ---

    #[test]
    fn mount_all() {
        let cli = parse(&["mount", "--all"]);
        match cli.command.unwrap() {
            Command::Mount { all } => assert!(all),
            other => panic!("expected Mount, got {:?}", other),
        }
    }

    // --- Unmount ---

    #[test]
    fn unmount_all_force() {
        let cli = parse(&["unmount", "--all", "--force"]);
        match cli.command.unwrap() {
            Command::Unmount { all, force } => {
                assert!(all);
                assert!(force);
            }
            other => panic!("expected Unmount, got {:?}", other),
        }
    }

    #[test]
    fn unmount_no_force() {
        let cli = parse(&["unmount", "--all"]);
        match cli.command.unwrap() {
            Command::Unmount { all, force } => {
                assert!(all);
                assert!(!force);
            }
            other => panic!("expected Unmount, got {:?}", other),
        }
    }

    // --- Folders ---

    #[test]
    fn folders_with_subpath() {
        let cli = parse(&["folders", "--share", "CORE", "--subpath", "dev", "--json"]);
        match cli.command.unwrap() {
            Command::Folders {
                share,
                subpath,
                json,
            } => {
                assert_eq!(share, "CORE");
                assert_eq!(subpath.as_deref(), Some("dev"));
                assert!(json);
            }
            other => panic!("expected Folders, got {:?}", other),
        }
    }

    #[test]
    fn folders_without_subpath() {
        let cli = parse(&["folders", "--share", "CORE"]);
        match cli.command.unwrap() {
            Command::Folders {
                share,
                subpath,
                json,
            } => {
                assert_eq!(share, "CORE");
                assert!(subpath.is_none());
                assert!(!json);
            }
            other => panic!("expected Folders, got {:?}", other),
        }
    }

    // --- Alias subcommands ---

    #[test]
    fn alias_add() {
        let cli = parse(&[
            "alias",
            "add",
            "--name",
            "proj",
            "--share",
            "CORE",
            "--target-subpath",
            "dev/projects",
        ]);
        match cli.command.unwrap() {
            Command::Alias {
                command:
                    AliasCommand::Add {
                        name,
                        share,
                        target_subpath,
                        alias_path,
                    },
            } => {
                assert_eq!(name, "proj");
                assert_eq!(share, "CORE");
                assert_eq!(target_subpath, "dev/projects");
                assert!(alias_path.is_none());
            }
            other => panic!("expected Alias Add, got {:?}", other),
        }
    }

    #[test]
    fn alias_add_with_custom_path() {
        let cli = parse(&[
            "alias",
            "add",
            "--name",
            "proj",
            "--share",
            "CORE",
            "--target-subpath",
            "dev",
            "--alias-path",
            "/custom/link",
        ]);
        match cli.command.unwrap() {
            Command::Alias {
                command: AliasCommand::Add { alias_path, .. },
            } => {
                assert_eq!(alias_path.as_deref(), Some("/custom/link"));
            }
            other => panic!("expected Alias Add, got {:?}", other),
        }
    }

    #[test]
    fn alias_list_json() {
        let cli = parse(&["alias", "list", "--json"]);
        match cli.command.unwrap() {
            Command::Alias {
                command: AliasCommand::List { json },
            } => assert!(json),
            other => panic!("expected Alias List, got {:?}", other),
        }
    }

    #[test]
    fn alias_remove() {
        let cli = parse(&["alias", "remove", "--name", "proj"]);
        match cli.command.unwrap() {
            Command::Alias {
                command: AliasCommand::Remove { name },
            } => assert_eq!(name, "proj"),
            other => panic!("expected Alias Remove, got {:?}", other),
        }
    }

    #[test]
    fn alias_reconcile() {
        let cli = parse(&["alias", "reconcile", "--all"]);
        match cli.command.unwrap() {
            Command::Alias {
                command: AliasCommand::Reconcile { all },
            } => assert!(all),
            other => panic!("expected Alias Reconcile, got {:?}", other),
        }
    }

    // --- Favorites subcommands ---

    #[test]
    fn favorites_add() {
        let cli = parse(&[
            "favorites",
            "add",
            "--share",
            "NAS",
            "--tb-host",
            "10.0.0.1",
            "--fallback-host",
            "192.168.1.1",
            "--username",
            "admin",
        ]);
        match cli.command.unwrap() {
            Command::Favorites {
                command:
                    FavoritesCommand::Add {
                        share,
                        tb_host,
                        fallback_host,
                        username,
                        remote_share,
                    },
            } => {
                assert_eq!(share, "NAS");
                assert_eq!(tb_host, "10.0.0.1");
                assert_eq!(fallback_host, "192.168.1.1");
                assert_eq!(username, "admin");
                assert!(remote_share.is_none());
            }
            other => panic!("expected Favorites Add, got {:?}", other),
        }
    }

    #[test]
    fn favorites_add_with_remote_share() {
        let cli = parse(&[
            "favorites",
            "add",
            "--share",
            "NAS",
            "--tb-host",
            "10.0.0.1",
            "--fallback-host",
            "192.168.1.1",
            "--username",
            "admin",
            "--remote-share",
            "DATA$",
        ]);
        match cli.command.unwrap() {
            Command::Favorites {
                command: FavoritesCommand::Add { remote_share, .. },
            } => {
                assert_eq!(remote_share.as_deref(), Some("DATA$"));
            }
            other => panic!("expected Favorites Add, got {:?}", other),
        }
    }

    #[test]
    fn favorites_remove_with_cleanup() {
        let cli = parse(&["favorites", "remove", "--share", "NAS", "--cleanup"]);
        match cli.command.unwrap() {
            Command::Favorites {
                command: FavoritesCommand::Remove { share, cleanup },
            } => {
                assert_eq!(share, "NAS");
                assert!(cleanup);
            }
            other => panic!("expected Favorites Remove, got {:?}", other),
        }
    }

    #[test]
    fn favorites_list_json() {
        let cli = parse(&["favorites", "list", "--json"]);
        match cli.command.unwrap() {
            Command::Favorites {
                command: FavoritesCommand::List { json },
            } => assert!(json),
            other => panic!("expected Favorites List, got {:?}", other),
        }
    }

    // --- Config subcommands ---

    #[test]
    fn config_set() {
        let cli = parse(&["config", "set", "lsof-recheck", "on"]);
        match cli.command.unwrap() {
            Command::Config {
                command: ConfigCommand::Set { key, value },
            } => {
                assert_eq!(key, "lsof-recheck");
                assert_eq!(value, "on");
            }
            other => panic!("expected Config Set, got {:?}", other),
        }
    }

    #[test]
    fn config_show() {
        let cli = parse(&["config", "show"]);
        match cli.command.unwrap() {
            Command::Config {
                command: ConfigCommand::Show,
            } => {}
            other => panic!("expected Config Show, got {:?}", other),
        }
    }

    // --- Install / Uninstall ---

    #[test]
    fn install_command() {
        let cli = parse(&["install"]);
        assert!(matches!(cli.command.unwrap(), Command::Install));
    }

    #[test]
    fn uninstall_command() {
        let cli = parse(&["uninstall"]);
        assert!(matches!(cli.command.unwrap(), Command::Uninstall));
    }

    // --- Invalid input ---

    #[test]
    fn invalid_backend_rejected() {
        // --to only accepts "tb" or "fallback"
        let _ = parse_err(&["switch", "--share", "CORE", "--to", "invalid"]);
    }

    #[test]
    fn unknown_subcommand_rejected() {
        let _ = parse_err(&["nonexistent"]);
    }
}
