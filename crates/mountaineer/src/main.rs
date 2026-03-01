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

use cli::{AliasCommand, Cli, Command, ConfigCommand, FavoritesCommand, MultiShareTarget};
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
        Command::Switch { share, to, force } => {
            log::info!(
                "cli: switch --share={} --to={} --force={}",
                share,
                to.short_label(),
                force
            );
            cmd_switch(&share, to, force)
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
        Command::Unmount { all, force } => {
            log::info!("cli: unmount --all={} --force={}", all, force);
            cmd_unmount(all, force)
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
        Command::Config { command } => {
            log::info!("cli: config command");
            cmd_config(command)
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
    let initial_cfg = config::load()?;
    ensure_has_shares(&initial_cfg)?;

    let interval_secs = interval
        .or(Some(initial_cfg.global.check_interval_secs))
        .unwrap_or(2)
        .max(1);

    println!(
        "monitoring {} share(s) every {}s (Ctrl+C to stop)",
        initial_cfg.shares.len(),
        interval_secs
    );

    // Start SCDynamicStore network change monitor (spec 11)
    let network_rx = network::monitor::start();
    log::info!("Network change monitor started for cmd_monitor");

    let mut state = engine::load_runtime_state().unwrap_or_default();
    loop {
        // Hot-reload config each cycle per spec 11
        let cfg = config::load().unwrap_or(initial_cfg.clone());
        let statuses = engine::reconcile_all(&cfg, &mut state);
        print_status_table(&statuses);
        engine::save_runtime_state(&state)?;

        // Wait for either: timer expiry OR network change event (spec 11).
        // On network event, debounce 500ms then immediately reconcile (spec 11).
        match network_rx.recv_timeout(std::time::Duration::from_secs(interval_secs)) {
            Ok(event) => {
                log::info!("Network change detected: {:?}", event.changed_keys);
                // Debounce: drain any further events arriving within 500ms (spec 11)
                let debounce = std::time::Duration::from_millis(500);
                while network_rx.recv_timeout(debounce).is_ok() {}
                log::info!("Network debounce complete, triggering immediate reconcile");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Normal timer-based reconcile — continue loop
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("Network monitor channel disconnected, falling back to timer-only");
                std::thread::sleep(std::time::Duration::from_secs(interval_secs));
            }
        }
    }
}

fn cmd_status(all: bool, json: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("status currently requires --all"));
    }

    let cfg = config::load()?;
    let mut state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::verify_all(&cfg, &mut state);
    engine::save_runtime_state(&state)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
    } else {
        print_status_table(&statuses);
    }
    Ok(())
}

fn cmd_switch(share_name: &str, to: Backend, force: bool) -> Result<()> {
    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let share = config::find_share(&cfg, share_name)
        .ok_or_else(|| anyhow!("share '{}' is not configured", share_name))?
        .clone();

    let mut state = engine::load_runtime_state().unwrap_or_default();

    let from = state
        .shares
        .get(&share_name.to_ascii_lowercase())
        .and_then(|e| e.active_backend)
        .ok_or_else(|| {
            anyhow!(
                "share '{}' has no active backend to switch from",
                share_name
            )
        })?;

    if from == to {
        println!("{} is already on {}", share_name, to.short_label());
        return Ok(());
    }

    match engine::switch_backend_single_mount(&cfg, &mut state, &share, from, to, force) {
        engine::SwitchResult::Success => {
            engine::save_runtime_state(&state)?;
            let statuses = engine::verify_all(&cfg, &mut state);
            print_status_table(&statuses);
            Ok(())
        }
        engine::SwitchResult::BusyOpenFiles => Err(anyhow!(
            "cannot switch '{}': open files detected. Close files and retry, or use --force",
            share_name
        )),
        engine::SwitchResult::UnmountFailed(e) => Err(anyhow!(
            "cannot switch '{}': unmount failed: {}",
            share_name,
            e
        )),
        engine::SwitchResult::MountFailed { error, rolled_back } => {
            if rolled_back {
                engine::save_runtime_state(&state)?;
            }
            Err(anyhow!(
                "cannot switch '{}': mount failed: {} (rolled back: {})",
                share_name,
                error,
                rolled_back
            ))
        }
    }
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
    // Use mount_all (not reconcile_all) so already-mounted shares are left
    // untouched — no failover or recovery is triggered. Per spec 08.
    let statuses = engine::mount_all(&cfg, &mut state);
    engine::save_runtime_state(&state)?;
    print_status_table(&statuses);
    Ok(())
}

fn cmd_unmount(all: bool, force: bool) -> Result<()> {
    if !all {
        return Err(anyhow!("unmount currently requires --all"));
    }

    let cfg = config::load()?;
    ensure_has_shares(&cfg)?;

    let mut state = engine::load_runtime_state().unwrap_or_default();
    let results = engine::unmount_all(&cfg, &mut state, force);
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

            engine::add_share(&mut cfg, share_cfg)?;
            config::save(&cfg)?;
            println!("Added favorite '{}'.", share);

            // Attempt immediate mount — non-fatal if it fails, since the monitor
            // loop will retry. Config and symlink are already persisted.
            let mut state = engine::load_runtime_state().unwrap_or_default();
            match engine::reconcile_selected(&cfg, &mut state, std::slice::from_ref(&share)) {
                Ok(statuses) => {
                    engine::save_runtime_state(&state)?;
                    for status in &statuses {
                        if let Some(err) = &status.last_error {
                            eprintln!("warning: initial mount for '{}' failed: {}", share, err);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("warning: initial mount for '{}' failed: {}", share, e);
                }
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

fn cmd_config(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Set { key, value } => {
            let mut cfg = config::load()?;
            match key.as_str() {
                "lsof-recheck" => {
                    cfg.global.lsof_recheck = parse_on_off(&value)?;
                    println!(
                        "lsof-recheck = {}",
                        if cfg.global.lsof_recheck { "on" } else { "off" }
                    );
                }
                "auto-failback" => {
                    cfg.global.auto_failback = parse_on_off(&value)?;
                    println!(
                        "auto-failback = {}",
                        if cfg.global.auto_failback {
                            "on"
                        } else {
                            "off"
                        }
                    );
                }
                "check-interval" => {
                    let secs: u64 = value
                        .parse()
                        .map_err(|_| anyhow!("invalid number: {}", value))?;
                    if secs == 0 {
                        return Err(anyhow!("check-interval must be >= 1"));
                    }
                    cfg.global.check_interval_secs = secs;
                    println!("check-interval = {}s", secs);
                }
                "connect-timeout" => {
                    let ms: u64 = value
                        .parse()
                        .map_err(|_| anyhow!("invalid number: {}", value))?;
                    if ms == 0 {
                        return Err(anyhow!("connect-timeout must be >= 1"));
                    }
                    cfg.global.connect_timeout_ms = ms;
                    println!("connect-timeout = {}ms", ms);
                }
                _ => {
                    return Err(anyhow!(
                        "unknown config key '{}'. valid keys: lsof-recheck, auto-failback, check-interval, connect-timeout",
                        key
                    ));
                }
            }
            config::save(&cfg)?;
            Ok(())
        }
        ConfigCommand::Show => {
            let cfg = config::load()?;
            println!("shares_root = {}", cfg.global.shares_root);
            println!("check_interval_secs = {}", cfg.global.check_interval_secs);
            println!("auto_failback = {}", cfg.global.auto_failback);
            println!(
                "auto_failback_stable_secs = {}",
                cfg.global.auto_failback_stable_secs
            );
            println!("connect_timeout_ms = {}", cfg.global.connect_timeout_ms);
            println!("lsof_recheck = {}", cfg.global.lsof_recheck);
            Ok(())
        }
    }
}

fn parse_on_off(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        _ => Err(anyhow!("invalid value '{}': expected on|off", value)),
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
        "{:<16} {:<11} {:<10} {:<8} {:<8} {:<8} {:<8} STABLE PATH",
        "SHARE", "ACTIVE", "TB READY", "TB NET", "TB MNT", "FB NET", "FB MNT"
    );

    for status in statuses {
        let tb_ready_label = if status.tb_recovery_pending {
            "YES"
        } else {
            ""
        };
        println!(
            "{:<16} {:<11} {:<10} {:<8} {:<8} {:<8} {:<8} {}",
            status.name,
            status
                .active_backend
                .map(|b| b.short_label().to_string())
                .unwrap_or_else(|| "none".to_string()),
            tb_ready_label,
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
