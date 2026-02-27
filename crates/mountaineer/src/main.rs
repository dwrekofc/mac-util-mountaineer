#![allow(unexpected_cfgs)]

use anyhow::{Result, anyhow};
use clap::Parser;

mod cli;
mod config;
mod discovery;
mod engine;
mod gui;
mod launchd;
mod logging;
mod mount;
mod network;
mod tray;

use cli::{AliasCommand, Cli, Command, FavoritesCommand, MultiShareTarget};
use config::{AliasConfig, Backend, Config, ShareConfig};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mode = if cli.command.is_none() {
        logging::LoggingMode::Gui
    } else {
        logging::LoggingMode::Cli
    };
    if let Err(err) = logging::init(mode) {
        eprintln!("mountaineer: {}", err);
    }

    match cli.command {
        None => {
            gui::run();
            Ok(())
        }
        Some(command) => run_cli(command),
    }
}

fn run_cli(command: Command) -> Result<()> {
    match command {
        Command::Reconcile { all } => {
            log::info!("cli: reconcile --all={}", all);
            cmd_reconcile(all)
        }
        Command::Monitor { interval } => {
            log::info!("cli: monitor --interval={:?}", interval);
            cmd_monitor(interval)
        }
        Command::Status { all, json } => {
            log::info!("cli: status --all={} --json={}", all, json);
            cmd_status(all, json)
        }
        Command::Switch { share, to } => {
            log::info!("cli: switch --share={} --to={}", share, to.short_label());
            cmd_switch(&share, to)
        }
        Command::MountBackends(target) => {
            log::info!(
                "cli: mount-backends --all={} --share={:?}",
                target.all,
                target.share
            );
            cmd_mount_backends(target)
        }
        Command::Verify { target, json } => {
            log::info!(
                "cli: verify --all={} --share={:?} --json={}",
                target.all,
                target.share,
                json
            );
            cmd_verify(target, json)
        }
        Command::Mount { all } => {
            log::info!("cli: mount --all={}", all);
            cmd_mount(all)
        }
        Command::Unmount { all } => {
            log::info!("cli: unmount --all={}", all);
            cmd_unmount(all)
        }
        Command::Folders {
            share,
            subpath,
            json,
        } => {
            log::info!(
                "cli: folders --share={} --subpath={:?} --json={}",
                share,
                subpath,
                json
            );
            cmd_folders(&share, subpath.as_deref(), json)
        }
        Command::Alias { command } => {
            log::info!("cli: alias command");
            cmd_alias(command)
        }
        Command::Favorites { command } => {
            log::info!("cli: favorites command");
            cmd_favorites(command)
        }
        Command::Install => {
            log::info!("cli: install");
            cmd_install()
        }
        Command::Uninstall => {
            log::info!("cli: uninstall");
            cmd_uninstall()
        }
    }
}

fn cmd_reconcile(all: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("reconcile currently requires --all"));
    }

    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::reconcile_all(&cfg, &mut state);
    engine::save_runtime_state(&state)?;

    print_status_table(&statuses);
    Ok(())
}

fn cmd_monitor(interval: Option<u64>) -> Result<()> {
    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let interval_secs = interval
        .or(Some(cfg.global.check_interval_secs))
        .unwrap_or(2)
        .max(1);

    println!(
        "monitoring {} share(s) every {}s (Ctrl+C to stop)",
        cfg.shares.len(),
        interval_secs
    );

    let mut state = engine::load_runtime_state().unwrap_or_default();
    loop {
        let statuses = engine::reconcile_all(&cfg, &mut state);
        print_status_table(&statuses);
        engine::save_runtime_state(&state)?;
        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
    }
}

fn cmd_status(all: bool, json: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("status currently requires --all"));
    }

    let cfg = config::load()?;
    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::share_statuses(&cfg, &mut state);
    engine::save_runtime_state(&state)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
    } else {
        print_status_table(&statuses);
    }
    Ok(())
}

fn cmd_switch(share: &str, to: Backend) -> Result<()> {
    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let status = engine::switch_share(&cfg, &mut state, share, to)?;
    engine::save_runtime_state(&state)?;

    print_status_table(&[status]);
    Ok(())
}

fn cmd_mount_backends(target: MultiShareTarget) -> Result<()> {
    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let names = resolve_target_shares(&cfg, &target)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::mount_backends_for_shares(&cfg, &mut state, &names)?;
    engine::save_runtime_state(&state)?;

    print_status_table(&statuses);
    Ok(())
}

fn cmd_verify(target: MultiShareTarget, json: bool) -> Result<()> {
    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = if target.all || target.share.is_none() {
        engine::verify_all(&cfg, &mut state)
    } else {
        let names = resolve_target_shares(&cfg, &target)?;
        engine::verify_selected(&cfg, &mut state, &names)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
    } else {
        print_status_table(&statuses);
    }
    Ok(())
}

fn cmd_mount(all: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("mount currently requires --all"));
    }

    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::reconcile_all(&cfg, &mut state);
    engine::save_runtime_state(&state)?;
    print_status_table(&statuses);
    Ok(())
}

fn cmd_unmount(all: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("unmount currently requires --all"));
    }

    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let results = engine::unmount_all(&cfg, &mut state);
    engine::save_runtime_state(&state)?;

    println!(
        "{:<16} {:<10} {:<8} {:<8} {:<8} MESSAGE",
        "SHARE", "BACKEND", "TRY", "BUSY", "OK"
    );
    for item in results {
        println!(
            "{:<16} {:<10} {:<8} {:<8} {:<8} {}",
            item.share,
            item.backend.short_label(),
            yes_no(item.attempted),
            yes_no(item.busy),
            yes_no(item.unmounted),
            item.message.unwrap_or_default()
        );
    }
    Ok(())
}

fn cmd_folders(share: &str, subpath: Option<&str>, json: bool) -> Result<()> {
    let cfg = config::load()?;
    let entries = engine::list_folders(&cfg, share, subpath)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("No folders found.");
    } else {
        for entry in entries {
            println!("{}", entry.path);
        }
    }

    Ok(())
}

fn cmd_alias(command: AliasCommand) -> Result<()> {
    match command {
        AliasCommand::Add {
            name,
            share,
            target_subpath,
            alias_path,
        } => {
            let mut cfg = config::load()?;
            let path_buf = alias_path
                .as_deref()
                .map(config::expand_path)
                .unwrap_or_else(|| config::default_alias_path(&cfg, &name));

            let alias = AliasConfig {
                name,
                path: config::normalize_alias_path(&path_buf),
                share,
                target_subpath,
            };

            engine::add_alias(&mut cfg, alias.clone())?;
            config::save(&cfg)?;

            let status = engine::reconcile_alias(&cfg, &alias);
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
        AliasCommand::List { json } => {
            let cfg = config::load()?;
            let aliases = engine::inspect_aliases(&cfg);
            if json {
                println!("{}", serde_json::to_string_pretty(&aliases)?);
            } else {
                println!("{:<20} {:<40} {:<8} MESSAGE", "ALIAS", "PATH", "HEALTH");
                for alias in aliases {
                    println!(
                        "{:<20} {:<40} {:<8} {}",
                        alias.name,
                        alias.path,
                        yes_no(alias.healthy),
                        alias.message.unwrap_or_default()
                    );
                }
            }
            Ok(())
        }
        AliasCommand::Remove { name } => {
            let mut cfg = config::load()?;
            let alias = engine::remove_alias(&mut cfg, &name)?;
            config::save(&cfg)?;

            let alias_path = config::expand_path(&alias.path);
            if alias_path.exists() && alias_path.is_symlink() {
                let _ = std::fs::remove_file(alias_path);
            }

            println!("Removed alias '{}'", alias.name);
            Ok(())
        }
        AliasCommand::Reconcile { all } => {
            let cfg = config::load()?;
            let aliases = engine::reconcile_aliases(&cfg);
            if !all {
                log::debug!("alias reconcile invoked without --all; reconciling all aliases");
            }
            println!("{}", serde_json::to_string_pretty(&aliases)?);
            Ok(())
        }
    }
}

fn cmd_favorites(command: FavoritesCommand) -> Result<()> {
    match command {
        FavoritesCommand::Add {
            share,
            tb_host,
            fallback_host,
            username,
            remote_share,
        } => {
            let mut cfg = config::load()?;
            let share_cfg = ShareConfig {
                name: share.clone(),
                username,
                thunderbolt_host: tb_host,
                fallback_host,
                share_name: remote_share.unwrap_or_else(|| share.clone()),
            };

            let updated = engine::add_or_update_share(&mut cfg, share_cfg);
            config::save(&cfg)?;

            let mut state = engine::load_runtime_state().unwrap_or_default();
            let _ = engine::reconcile_selected(&cfg, &mut state, std::slice::from_ref(&share))?;
            engine::save_runtime_state(&state)?;

            if updated {
                println!("Updated favorite '{}'.", share);
            } else {
                println!("Added favorite '{}'.", share);
            }
            Ok(())
        }
        FavoritesCommand::Remove { share, cleanup } => {
            let mut cfg = config::load()?;
            let removed = engine::remove_share(&mut cfg, &share)
                .ok_or_else(|| anyhow!("favorite '{}' was not found", share))?;
            config::save(&cfg)?;

            if cleanup {
                let mut state = engine::load_runtime_state().unwrap_or_default();
                let (affected_aliases, unmount_results) =
                    engine::cleanup_removed_share(&cfg, &mut state, &removed.name)?;
                engine::save_runtime_state(&state)?;

                println!("Removed '{}' from favorites with cleanup.", removed.name);
                if affected_aliases > 0 {
                    println!(
                        "{} alias(es) still reference this share and should be removed or updated.",
                        affected_aliases
                    );
                }
                for item in unmount_results {
                    println!(
                        "cleanup {} {}: attempted={} busy={} unmounted={} {}",
                        item.share,
                        item.backend.short_label(),
                        item.attempted,
                        item.busy,
                        item.unmounted,
                        item.message.unwrap_or_default()
                    );
                }
            } else {
                println!("Removed '{}' from favorites.", removed.name);
            }

            Ok(())
        }
        FavoritesCommand::List { json } => {
            let cfg = config::load()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&cfg.shares)?);
            } else {
                if cfg.shares.is_empty() {
                    println!("No favorites configured.");
                    return Ok(());
                }

                println!(
                    "{:<16} {:<16} {:<24} {:<24} REMOTE",
                    "SHARE", "USER", "TB HOST", "FALLBACK HOST"
                );
                for share in cfg.shares {
                    println!(
                        "{:<16} {:<16} {:<24} {:<24} {}",
                        share.name,
                        share.username,
                        share.thunderbolt_host,
                        share.fallback_host,
                        share.share_name
                    );
                }
            }
            Ok(())
        }
    }
}

fn cmd_install() -> Result<()> {
    if launchd::is_installed() {
        println!("LaunchAgent already exists. Reinstalling...");
    }
    launchd::install()?;
    println!("LaunchAgent installed.");
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    launchd::uninstall()?;
    println!("LaunchAgent removed.");
    Ok(())
}

fn resolve_target_shares(cfg: &Config, target: &MultiShareTarget) -> Result<Vec<String>> {
    if target.all || target.share.is_none() {
        return Ok(cfg.shares.iter().map(|share| share.name.clone()).collect());
    }

    let share = target
        .share
        .as_ref()
        .ok_or_else(|| anyhow!("missing --share or --all"))?;

    if config::find_share(cfg, share).is_none() {
        return Err(anyhow!("share '{}' is not configured", share));
    }

    Ok(vec![share.clone()])
}

fn ensure_has_shares(cfg: &Config) -> Result<()> {
    if cfg.shares.is_empty() {
        Err(anyhow!(
            "no favorites configured. use `mountaineer favorites add ...` first"
        ))
    } else {
        Ok(())
    }
}

fn print_status_table(statuses: &[engine::ShareStatus]) {
    if statuses.is_empty() {
        println!("No shares configured.");
        return;
    }

    println!(
        "{:<16} {:<11} {:<8} {:<8} {:<8} {:<8} STABLE PATH",
        "SHARE", "ACTIVE", "TB NET", "TB MNT", "FB NET", "FB MNT"
    );

    for status in statuses {
        println!(
            "{:<16} {:<11} {:<8} {:<8} {:<8} {:<8} {}",
            status.name,
            status
                .active_backend
                .map(|b| b.short_label().to_string())
                .unwrap_or_else(|| "none".to_string()),
            yes_no(status.tb.reachable),
            yes_no(status.tb.ready),
            yes_no(status.fallback.reachable),
            yes_no(status.fallback.ready),
            status.stable_path
        );

        if let Some(error) = &status.last_error {
            println!("  ! {}", error);
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
