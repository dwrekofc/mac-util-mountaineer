use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::config::{self, AliasConfig, Backend, Config, ShareConfig};
use crate::{discovery, mount};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    #[serde(default)]
    pub shares: HashMap<String, ShareRuntimeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShareRuntimeState {
    pub active_backend: Option<Backend>,
    pub last_switch_at: Option<DateTime<Utc>>,
    pub tb_reachable_since: Option<DateTime<Utc>>,
    pub tb_healthy_since: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// TB became available while on Fallback (awaiting user confirmation to switch).
    /// In single_mount_mode with auto_failback=false, the user must explicitly trigger the switch.
    #[serde(default)]
    pub tb_recovery_pending: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendStatus {
    pub host: String,
    pub mount_point: String,
    pub reachable: bool,
    pub mounted: bool,
    pub alive: bool,
    pub ready: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareStatus {
    pub name: String,
    pub stable_path: String,
    pub active_backend: Option<Backend>,
    pub desired_backend: Option<Backend>,
    pub tb: BackendStatus,
    pub fallback: BackendStatus,
    pub last_switch_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasStatus {
    pub name: String,
    pub path: String,
    pub share: String,
    pub target_subpath: String,
    pub target: String,
    pub current_target: Option<String>,
    pub target_exists: bool,
    pub healthy: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnmountResult {
    pub share: String,
    pub backend: Backend,
    pub mount_point: String,
    pub attempted: bool,
    pub unmounted: bool,
    pub busy: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
struct BackendProbe {
    status: BackendStatus,
}

pub fn load_runtime_state() -> Result<RuntimeState> {
    let path = config::state_path();
    if !path.exists() {
        return Ok(RuntimeState::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed reading runtime state {}", path.display()))?;
    let state: RuntimeState = serde_json::from_str(&text)
        .with_context(|| format!("failed parsing runtime state {}", path.display()))?;
    Ok(state)
}

pub fn save_runtime_state(state: &RuntimeState) -> Result<()> {
    let path = config::state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(state)?;
    fs::write(&path, text)
        .with_context(|| format!("failed writing runtime state {}", path.display()))?;
    Ok(())
}

pub fn verify_all(config: &Config, state: &mut RuntimeState) -> Vec<ShareStatus> {
    let now = Utc::now();
    config
        .shares
        .iter()
        .map(|share| reconcile_share(config, state, share, false, false, now))
        .collect()
}

pub fn reconcile_all(config: &Config, state: &mut RuntimeState) -> Vec<ShareStatus> {
    let now = Utc::now();
    let statuses: Vec<ShareStatus> = config
        .shares
        .iter()
        .map(|share| reconcile_share(config, state, share, true, true, now))
        .collect();
    let _ = reconcile_aliases(config);
    statuses
}

pub fn mount_backends_for_shares(
    config: &Config,
    state: &mut RuntimeState,
    share_names: &[String],
) -> Result<Vec<ShareStatus>> {
    let now = Utc::now();
    let shares = select_shares(config, share_names)?;
    let statuses = shares
        .iter()
        .map(|share| reconcile_share(config, state, share, true, false, now))
        .collect();
    Ok(statuses)
}

pub fn reconcile_selected(
    config: &Config,
    state: &mut RuntimeState,
    share_names: &[String],
) -> Result<Vec<ShareStatus>> {
    let now = Utc::now();
    let shares = select_shares(config, share_names)?;
    let statuses = shares
        .iter()
        .map(|share| reconcile_share(config, state, share, true, true, now))
        .collect();
    Ok(statuses)
}

pub fn verify_selected(
    config: &Config,
    state: &mut RuntimeState,
    share_names: &[String],
) -> Result<Vec<ShareStatus>> {
    let now = Utc::now();
    let shares = select_shares(config, share_names)?;
    let statuses = shares
        .iter()
        .map(|share| reconcile_share(config, state, share, false, false, now))
        .collect();
    Ok(statuses)
}

pub fn switch_share(
    config: &Config,
    state: &mut RuntimeState,
    share_name: &str,
    to: Backend,
) -> Result<ShareStatus> {
    let share = config::find_share(config, share_name)
        .ok_or_else(|| anyhow!("share '{}' is not configured", share_name))?;

    let mut status = reconcile_share(config, state, share, true, false, Utc::now());
    let target_probe = match to {
        Backend::Tb => &status.tb,
        Backend::Fallback => &status.fallback,
    };

    if !target_probe.ready {
        return Err(anyhow!(
            "cannot switch '{}' to {}: backend is not ready",
            share.name,
            to.short_label()
        ));
    }

    let mount_target = config::backend_mount_path(config, &share.name, to);
    set_symlink_atomically(
        &mount_target,
        &config::share_stable_path(config, &share.name),
    )?;

    let entry = state_entry_mut(state, &share.name);
    entry.active_backend = Some(to);
    entry.last_switch_at = Some(Utc::now());
    entry.last_error = None;

    status.active_backend = Some(to);
    status.desired_backend = Some(to);
    status.last_switch_at = entry.last_switch_at;
    status.last_error = None;
    Ok(status)
}

/// Result of a single-mount backend switch operation.
#[derive(Debug, Clone)]
pub enum SwitchResult {
    /// Switch completed successfully.
    Success,
    /// Cannot switch: open files detected on current mount.
    BusyOpenFiles,
    /// Failed to unmount the current backend.
    UnmountFailed(String),
    /// Failed to mount the new backend.
    MountFailed {
        /// True if we successfully rolled back to the previous backend.
        rolled_back: bool,
        /// The mount error message.
        error: String,
    },
}

/// Switch backends in single-mount mode: unmount old → mount new → update symlink.
/// Attempts rollback if the new mount fails.
pub fn switch_backend_single_mount(
    config: &Config,
    state: &mut RuntimeState,
    share: &ShareConfig,
    from: Backend,
    to: Backend,
    force: bool,
) -> SwitchResult {
    let from_mount = config::backend_mount_path(config, &share.name, from);
    let to_mount = config::backend_mount_path(config, &share.name, to);
    let to_host = backend_host(share, to);
    let stable_path = config::share_stable_path(config, &share.name);

    // Step 1: Check for open files (unless force)
    if !force && mount::smb::is_mounted(&from_mount) && has_open_handles(&from_mount) {
        return SwitchResult::BusyOpenFiles;
    }

    // Step 2: Unmount old backend (if mounted)
    if mount::smb::is_mounted(&from_mount) {
        let unmount_result = if force {
            mount::smb::unmount(&from_mount)
        } else {
            mount::smb::unmount_graceful(&from_mount)
        };

        if let Err(e) = unmount_result {
            return SwitchResult::UnmountFailed(e.to_string());
        }
        log::info!(
            "{}: unmounted {} backend at {}",
            share.name,
            from.short_label(),
            from_mount.display()
        );
    }

    // Step 3: Mount new backend
    let mount_result =
        mount::smb::mount_share(to_host, &share.share_name, &share.username, &to_mount);

    match mount_result {
        Ok(()) => {
            // Verify mount is alive
            if !mount::smb::is_mount_alive(&to_mount) {
                log::warn!(
                    "{}: {} mounted but not responding, will retry",
                    share.name,
                    to.short_label()
                );
            }

            // Step 4: Update symlink
            if let Err(e) = set_symlink_atomically(&to_mount, &stable_path) {
                log::error!("{}: mount succeeded but symlink failed: {}", share.name, e);
            }

            // Update state
            let entry = state_entry_mut(state, &share.name);
            entry.active_backend = Some(to);
            entry.last_switch_at = Some(Utc::now());
            entry.tb_recovery_pending = false;
            entry.last_error = None;

            log::info!(
                "{}: switched {} -> {}",
                share.name,
                from.short_label(),
                to.short_label()
            );

            SwitchResult::Success
        }
        Err(e) => {
            let error_msg = e.to_string();
            log::error!(
                "{}: failed to mount {} at {}: {}",
                share.name,
                to.short_label(),
                to_mount.display(),
                error_msg
            );

            // Step 5: Rollback - try to remount old backend
            let from_host = backend_host(share, from);
            let rollback_result =
                mount::smb::mount_share(from_host, &share.share_name, &share.username, &from_mount);

            let rolled_back = rollback_result.is_ok();
            if rolled_back {
                log::info!(
                    "{}: rolled back to {} after failed switch",
                    share.name,
                    from.short_label()
                );
                // Restore symlink to old backend
                let _ = set_symlink_atomically(&from_mount, &stable_path);
            } else {
                log::error!(
                    "{}: rollback to {} also failed!",
                    share.name,
                    from.short_label()
                );
            }

            SwitchResult::MountFailed {
                rolled_back,
                error: error_msg,
            }
        }
    }
}

pub fn unmount_all(config: &Config, state: &mut RuntimeState) -> Vec<UnmountResult> {
    let mut results = Vec::new();

    for share in &config.shares {
        let active_backend = current_active_backend(config, state, share);

        for backend in [Backend::Tb, Backend::Fallback] {
            let mount_point = config::backend_mount_path(config, &share.name, backend);
            let mounted = mount::smb::is_mounted(&mount_point);
            let mut result = UnmountResult {
                share: share.name.clone(),
                backend,
                mount_point: mount_point.display().to_string(),
                attempted: mounted,
                unmounted: false,
                busy: false,
                message: None,
            };

            if !mounted {
                results.push(result);
                continue;
            }

            if has_open_handles(&mount_point) {
                result.busy = true;
                result.message = Some("deferred: open files detected".to_string());
                results.push(result);
                continue;
            }

            if active_backend == Some(backend) {
                match mount::smb::unmount_graceful(&mount_point) {
                    Ok(()) => {
                        result.unmounted = true;
                        result.message = Some("active backend unmounted gracefully".to_string());
                    }
                    Err(err) => {
                        result.message =
                            Some(format!("active backend not force-unmounted: {}", err));
                    }
                }
            } else {
                match mount::smb::unmount(&mount_point) {
                    Ok(()) => {
                        result.unmounted = true;
                    }
                    Err(err) => {
                        result.message = Some(err.to_string());
                    }
                }
            }
            results.push(result);
        }

        let stable = config::share_stable_path(config, &share.name);
        if is_symlink(&stable) {
            let _ = fs::remove_file(&stable);
        }

        let entry = state_entry_mut(state, &share.name);
        entry.active_backend = None;
        entry.last_error = None;
    }

    results
}

pub fn list_folders(
    config: &Config,
    share_name: &str,
    subpath: Option<&str>,
) -> Result<Vec<FolderEntry>> {
    let share = config::find_share(config, share_name)
        .ok_or_else(|| anyhow!("share '{}' is not configured", share_name))?;

    let mut root = config::share_stable_path(config, &share.name);
    if let Some(sub) = subpath {
        let trimmed = sub.trim_matches('/');
        if !trimmed.is_empty() {
            root = root.join(trimmed);
        }
    }

    let dir =
        fs::read_dir(&root).with_context(|| format!("failed reading folder {}", root.display()))?;

    let mut entries = Vec::new();
    for entry in dir {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            let path = entry.path();
            entries.push(FolderEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.display().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

pub fn reconcile_aliases(config: &Config) -> Vec<AliasStatus> {
    config
        .aliases
        .iter()
        .map(|alias| reconcile_alias(config, alias))
        .collect()
}

pub fn reconcile_alias(config: &Config, alias: &AliasConfig) -> AliasStatus {
    let alias_path = config::expand_path(&alias.path);
    let target = config::alias_target_path(config, alias);

    let mut message = None;
    if let Err(err) = set_symlink_atomically(&target, &alias_path) {
        message = Some(err.to_string());
    }

    inspect_alias(config, alias, message)
}

pub fn inspect_aliases(config: &Config) -> Vec<AliasStatus> {
    config
        .aliases
        .iter()
        .map(|alias| inspect_alias(config, alias, None))
        .collect()
}

pub fn add_alias(config: &mut Config, alias: AliasConfig) -> Result<()> {
    if config
        .aliases
        .iter()
        .any(|existing| existing.name.eq_ignore_ascii_case(&alias.name))
    {
        return Err(anyhow!("alias '{}' already exists", alias.name));
    }

    if config::find_share(config, &alias.share).is_none() {
        return Err(anyhow!("share '{}' is not configured", alias.share));
    }

    config.aliases.push(alias);
    Ok(())
}

pub fn remove_alias(config: &mut Config, name: &str) -> Result<AliasConfig> {
    let idx = config
        .aliases
        .iter()
        .position(|alias| alias.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("alias '{}' was not found", name))?;
    Ok(config.aliases.remove(idx))
}

pub fn add_or_update_share(config: &mut Config, share: ShareConfig) -> bool {
    if let Some(existing) = config::find_share_mut(config, &share.name) {
        *existing = share;
        return true;
    }
    config.shares.push(share);
    false
}

pub fn remove_share(config: &mut Config, share_name: &str) -> Option<ShareConfig> {
    let idx = config
        .shares
        .iter()
        .position(|share| share.name.eq_ignore_ascii_case(share_name))?;
    Some(config.shares.remove(idx))
}

pub fn cleanup_removed_share(
    config: &Config,
    state: &mut RuntimeState,
    removed_share_name: &str,
) -> Result<(usize, Vec<UnmountResult>)> {
    let temp_share = ShareConfig {
        name: removed_share_name.to_string(),
        username: String::new(),
        thunderbolt_host: String::new(),
        fallback_host: String::new(),
        share_name: String::new(),
    };

    let mut temp_cfg = config.clone();
    temp_cfg.shares.push(temp_share);

    let unmount_results = unmount_all_for_share(&temp_cfg, state, removed_share_name);

    let stable = config::share_stable_path(&temp_cfg, removed_share_name);
    if is_symlink(&stable) {
        let _ = fs::remove_file(&stable);
    }

    let affected_aliases = config
        .aliases
        .iter()
        .filter(|alias| alias.share.eq_ignore_ascii_case(removed_share_name))
        .count();

    Ok((affected_aliases, unmount_results))
}

fn unmount_all_for_share(
    config: &Config,
    state: &mut RuntimeState,
    share_name: &str,
) -> Vec<UnmountResult> {
    let mut results = Vec::new();
    let active_backend = config::find_share(config, share_name)
        .and_then(|share| current_active_backend(config, state, share));

    for backend in [Backend::Tb, Backend::Fallback] {
        let mount_point = config::backend_mount_path(config, share_name, backend);
        let mounted = mount::smb::is_mounted(&mount_point);
        let mut result = UnmountResult {
            share: share_name.to_string(),
            backend,
            mount_point: mount_point.display().to_string(),
            attempted: mounted,
            unmounted: false,
            busy: false,
            message: None,
        };
        if mounted {
            if has_open_handles(&mount_point) {
                result.busy = true;
                result.message = Some("deferred: open files detected".to_string());
            } else if active_backend == Some(backend) {
                match mount::smb::unmount_graceful(&mount_point) {
                    Ok(()) => {
                        result.unmounted = true;
                        result.message = Some("active backend unmounted gracefully".to_string());
                    }
                    Err(err) => {
                        result.message =
                            Some(format!("active backend not force-unmounted: {}", err));
                    }
                }
            } else {
                match mount::smb::unmount(&mount_point) {
                    Ok(()) => result.unmounted = true,
                    Err(err) => result.message = Some(err.to_string()),
                }
            }
        }
        results.push(result);
    }
    state_entry_mut(state, share_name).active_backend = None;
    results
}

pub fn share_statuses(config: &Config, state: &mut RuntimeState) -> Vec<ShareStatus> {
    verify_all(config, state)
}

fn reconcile_share(
    config: &Config,
    state: &mut RuntimeState,
    share: &ShareConfig,
    attempt_mount: bool,
    auto_switch: bool,
    now: DateTime<Utc>,
) -> ShareStatus {
    let timeout = Duration::from_millis(config.global.connect_timeout_ms);
    let single_mount = config.global.single_mount_mode;

    let stable_path = config::share_stable_path(config, &share.name);
    let detected_active = detect_active_backend(config, share, &stable_path);
    let remembered_active = state
        .shares
        .get(&share.name.to_ascii_lowercase())
        .and_then(|entry| entry.active_backend);
    let active_hint = detected_active.or(remembered_active);

    // Probe both backends (always check reachability for status display)
    // In single_mount_mode, only the active backend will attempt to mount
    let tb = probe_backend(
        config,
        share,
        Backend::Tb,
        timeout,
        attempt_mount,
        active_hint,
        single_mount,
    );
    let fb = probe_backend(
        config,
        share,
        Backend::Fallback,
        timeout,
        attempt_mount,
        active_hint,
        single_mount,
    );

    // Update TB reachability/health tracking (scoped borrow)
    let (active_backend, tb_stability_since) = {
        let entry = state_entry_mut(state, &share.name);
        if tb.status.reachable {
            if entry.tb_reachable_since.is_none() {
                entry.tb_reachable_since = Some(now);
            }
        } else {
            entry.tb_reachable_since = None;
            entry.tb_recovery_pending = false;
        }

        if tb.status.ready {
            if entry.tb_healthy_since.is_none() {
                entry.tb_healthy_since = Some(now);
            }
        } else {
            entry.tb_healthy_since = None;
        }

        let active_backend = detected_active.or(entry.active_backend);
        let tb_stability_since = match (entry.tb_reachable_since, entry.tb_healthy_since) {
            (Some(reach), Some(healthy)) => Some(std::cmp::min(reach, healthy)),
            (Some(reach), None) => Some(reach),
            (None, Some(healthy)) => Some(healthy),
            (None, None) => None,
        };
        (active_backend, tb_stability_since)
    };

    // In single_mount_mode, we handle failover differently
    let desired_backend = if single_mount {
        choose_desired_backend_single_mount(
            active_backend,
            tb.status.reachable,
            fb.status.reachable,
            config.global.auto_failback,
            tb_stability_since,
            config.global.auto_failback_stable_secs,
            now,
        )
    } else {
        choose_desired_backend(
            active_backend,
            tb.status.ready,
            fb.status.ready,
            config.global.auto_failback,
            tb_stability_since,
            config.global.auto_failback_stable_secs,
            now,
        )
    };

    let mut last_error = None;

    if single_mount && auto_switch {
        // Single-mount mode switching logic
        if let Some(active) = active_backend {
            let active_ready = backend_ready(active, &tb.status, &fb.status);

            if !active_ready {
                // Active backend went offline - need to failover
                let other = match active {
                    Backend::Tb => Backend::Fallback,
                    Backend::Fallback => Backend::Tb,
                };
                let other_reachable = match other {
                    Backend::Tb => tb.status.reachable,
                    Backend::Fallback => fb.status.reachable,
                };

                if other_reachable {
                    log::info!(
                        "{}: active {} is offline, failing over to {}",
                        share.name,
                        active.short_label(),
                        other.short_label()
                    );
                    // switch_backend_single_mount updates state internally
                    match switch_backend_single_mount(config, state, share, active, other, false) {
                        SwitchResult::Success => {
                            // State already updated by switch function
                        }
                        SwitchResult::BusyOpenFiles => {
                            let msg = format!(
                                "{}: failover blocked - open files on {}",
                                share.name,
                                active.short_label()
                            );
                            log::warn!("{}", msg);
                            last_error = Some(msg.clone());
                            state_entry_mut(state, &share.name).last_error = Some(msg);
                        }
                        SwitchResult::UnmountFailed(e) => {
                            let msg = format!("{}: failover unmount failed: {}", share.name, e);
                            log::error!("{}", msg);
                            last_error = Some(msg.clone());
                            state_entry_mut(state, &share.name).last_error = Some(msg);
                        }
                        SwitchResult::MountFailed { error, .. } => {
                            let msg = format!("{}: failover mount failed: {}", share.name, error);
                            log::error!("{}", msg);
                            last_error = Some(msg.clone());
                            state_entry_mut(state, &share.name).last_error = Some(msg);
                        }
                    }
                }
            } else if active == Backend::Fallback && tb.status.reachable {
                // On Fallback but TB is reachable - set pending flag for manual switch
                if !config.global.auto_failback {
                    let entry = state_entry_mut(state, &share.name);
                    if !entry.tb_recovery_pending {
                        log::info!(
                            "{}: TB is available - awaiting user confirmation to switch",
                            share.name
                        );
                        entry.tb_recovery_pending = true;
                    }
                } else {
                    // Auto-failback is enabled - check stability window
                    if let Some(since) = tb_stability_since {
                        let stable_for = (now - since).num_seconds().max(0) as u64;
                        if stable_for >= config.global.auto_failback_stable_secs {
                            log::info!(
                                "{}: TB stable for {}s, auto-failing back",
                                share.name,
                                stable_for
                            );
                            match switch_backend_single_mount(
                                config,
                                state,
                                share,
                                Backend::Fallback,
                                Backend::Tb,
                                false,
                            ) {
                                SwitchResult::Success => {
                                    // State already updated by switch function
                                }
                                SwitchResult::BusyOpenFiles => {
                                    let msg = format!(
                                        "{}: auto-failback blocked - open files",
                                        share.name
                                    );
                                    log::warn!("{}", msg);
                                    // Don't set as error - just defer
                                    state_entry_mut(state, &share.name).tb_recovery_pending = true;
                                }
                                SwitchResult::UnmountFailed(e) => {
                                    let msg = format!(
                                        "{}: auto-failback unmount failed: {}",
                                        share.name, e
                                    );
                                    log::error!("{}", msg);
                                    last_error = Some(msg.clone());
                                    state_entry_mut(state, &share.name).last_error = Some(msg);
                                }
                                SwitchResult::MountFailed { error, .. } => {
                                    let msg = format!(
                                        "{}: auto-failback mount failed: {}",
                                        share.name, error
                                    );
                                    log::error!("{}", msg);
                                    last_error = Some(msg.clone());
                                    state_entry_mut(state, &share.name).last_error = Some(msg);
                                }
                            }
                        }
                    }
                }
            } else if active == Backend::Tb {
                // On TB and it's working - clear any pending flags
                state_entry_mut(state, &share.name).tb_recovery_pending = false;
            }
        } else if let Some(desired) = desired_backend {
            // No active backend - do initial mount
            let host = backend_host(share, desired);
            let mount_path = config::backend_mount_path(config, &share.name, desired);
            log::info!(
                "{}: initial mount to {} at {}",
                share.name,
                desired.short_label(),
                mount_path.display()
            );
            match mount::smb::mount_share(host, &share.share_name, &share.username, &mount_path) {
                Ok(()) => {
                    if let Err(e) = set_symlink_atomically(&mount_path, &stable_path) {
                        log::error!("{}: symlink failed: {}", share.name, e);
                    }
                    let entry = state_entry_mut(state, &share.name);
                    entry.active_backend = Some(desired);
                    entry.last_switch_at = Some(now);
                }
                Err(e) => {
                    let msg = format!("{}: initial mount failed: {}", share.name, e);
                    log::error!("{}", msg);
                    last_error = Some(msg.clone());
                    state_entry_mut(state, &share.name).last_error = Some(msg);
                }
            }
        }
    } else if !single_mount && auto_switch {
        // Original dual-mount mode logic (unchanged)
        if let Some(desired) = desired_backend {
            if backend_ready(desired, &tb.status, &fb.status) {
                let mount_target = config::backend_mount_path(config, &share.name, desired);
                match set_symlink_atomically(&mount_target, &stable_path) {
                    Ok(()) => {
                        let entry = state_entry_mut(state, &share.name);
                        if active_backend != Some(desired) {
                            entry.last_switch_at = Some(now);
                            log::info!(
                                "{}: switched active backend {} -> {}",
                                share.name,
                                active_backend.map(|b| b.short_label()).unwrap_or("none"),
                                desired.short_label()
                            );
                        }
                        entry.active_backend = Some(desired);
                        entry.last_error = None;
                    }
                    Err(err) => {
                        let msg = format!("failed switching stable path: {}", err);
                        last_error = Some(msg.clone());
                        state_entry_mut(state, &share.name).last_error = Some(msg);
                    }
                }
            } else {
                if active_backend != Some(desired) {
                    log::info!(
                        "{}: holding active={} desired={} (desired backend not ready)",
                        share.name,
                        active_backend.map(|b| b.short_label()).unwrap_or("none"),
                        desired.short_label()
                    );
                }
                state_entry_mut(state, &share.name).active_backend = active_backend;
            }
        } else {
            state_entry_mut(state, &share.name).active_backend = active_backend;
        }
    } else {
        state_entry_mut(state, &share.name).active_backend = active_backend;
    }

    if last_error.is_none() {
        if tb.status.last_error.is_some() {
            last_error = tb.status.last_error.clone();
        }
        if last_error.is_none() && fb.status.last_error.is_some() {
            last_error = fb.status.last_error.clone();
        }
    }

    // Build final status
    let entry = state_entry_mut(state, &share.name);
    ShareStatus {
        name: share.name.clone(),
        stable_path: stable_path.display().to_string(),
        active_backend: entry.active_backend.or(active_backend),
        desired_backend,
        tb: tb.status,
        fallback: fb.status,
        last_switch_at: entry.last_switch_at,
        last_error: last_error.or_else(|| entry.last_error.clone()),
    }
}

fn choose_desired_backend(
    active: Option<Backend>,
    tb_ready: bool,
    fb_ready: bool,
    auto_failback: bool,
    tb_stability_since: Option<DateTime<Utc>>,
    failback_stable_secs: u64,
    now: DateTime<Utc>,
) -> Option<Backend> {
    match active {
        Some(Backend::Tb) => {
            if tb_ready {
                Some(Backend::Tb)
            } else {
                // Prefer fallback intent even before fallback is fully ready, so
                // status output remains consistent during disconnect transitions.
                Some(Backend::Fallback)
            }
        }
        Some(Backend::Fallback) => {
            if !fb_ready {
                if tb_ready {
                    return Some(Backend::Tb);
                }
                return Some(Backend::Fallback);
            }

            if tb_ready && auto_failback {
                if let Some(since) = tb_stability_since {
                    let stable_for = (now - since).num_seconds().max(0) as u64;
                    if stable_for >= failback_stable_secs {
                        return Some(Backend::Tb);
                    }
                }
            }
            Some(Backend::Fallback)
        }
        None => {
            if tb_ready {
                Some(Backend::Tb)
            } else if fb_ready {
                Some(Backend::Fallback)
            } else {
                None
            }
        }
    }
}

/// Choose desired backend in single-mount mode.
/// Uses reachability instead of ready (since only active backend is mounted).
fn choose_desired_backend_single_mount(
    active: Option<Backend>,
    tb_reachable: bool,
    fb_reachable: bool,
    auto_failback: bool,
    tb_stability_since: Option<DateTime<Utc>>,
    failback_stable_secs: u64,
    now: DateTime<Utc>,
) -> Option<Backend> {
    match active {
        Some(Backend::Tb) => {
            // Stay on TB if reachable
            if tb_reachable {
                Some(Backend::Tb)
            } else {
                Some(Backend::Fallback)
            }
        }
        Some(Backend::Fallback) => {
            if !fb_reachable {
                // Fallback is down - switch to TB if available
                if tb_reachable {
                    return Some(Backend::Tb);
                }
                return Some(Backend::Fallback);
            }

            // Both available - check auto_failback
            if tb_reachable && auto_failback {
                if let Some(since) = tb_stability_since {
                    let stable_for = (now - since).num_seconds().max(0) as u64;
                    if stable_for >= failback_stable_secs {
                        return Some(Backend::Tb);
                    }
                }
            }
            Some(Backend::Fallback)
        }
        None => {
            // No active backend - prefer TB
            if tb_reachable {
                Some(Backend::Tb)
            } else if fb_reachable {
                Some(Backend::Fallback)
            } else {
                None
            }
        }
    }
}

fn probe_backend(
    config: &Config,
    share: &ShareConfig,
    backend: Backend,
    timeout: Duration,
    attempt_mount: bool,
    active_backend: Option<Backend>,
    single_mount_mode: bool,
) -> BackendProbe {
    let host = backend_host(share, backend).to_string();
    let mount_path = config::backend_mount_path(config, &share.name, backend);

    let mut last_error = None;
    let reachable = discovery::is_smb_reachable_with_timeout(&host, timeout);

    let mut mounted = mount::smb::is_mounted(&mount_path);
    let mut alive = mounted && mount::smb::is_mount_alive(&mount_path);

    if mounted && !alive {
        let unmount_result = if active_backend == Some(backend) {
            mount::smb::unmount_graceful(&mount_path)
        } else {
            mount::smb::unmount(&mount_path)
        };

        match unmount_result {
            Ok(()) => {
                mounted = false;
                log::info!(
                    "{} {}: removed stale mount {}",
                    share.name,
                    backend.short_label(),
                    mount_path.display()
                );
            }
            Err(err) => {
                if active_backend == Some(backend) {
                    let msg = format!(
                        "{} stale active backend not force-unmounted: {}",
                        share.name, err
                    );
                    log::warn!("{}", msg);
                    last_error = Some(msg);
                } else {
                    let msg = format!("{} stale mount cleanup failed: {}", share.name, err);
                    log::warn!("{}", msg);
                    last_error = Some(msg);
                }
            }
        }
    }

    // In single_mount_mode, only mount if this is the active backend (or no backend is active yet)
    let should_mount = if single_mount_mode {
        attempt_mount && (active_backend.is_none() || active_backend == Some(backend))
    } else {
        attempt_mount
    };

    if should_mount && reachable && !mounted {
        log::info!(
            "{} {}: mount attempt host={} path={}",
            share.name,
            backend.short_label(),
            host,
            mount_path.display()
        );
        match mount::smb::mount_share(&host, &share.share_name, &share.username, &mount_path) {
            Ok(()) => {
                mounted = mount::smb::is_mounted(&mount_path);
                alive = mounted && mount::smb::is_mount_alive(&mount_path);
                if mounted && alive {
                    log::info!(
                        "{} {}: mount ready host={} path={}",
                        share.name,
                        backend.short_label(),
                        host,
                        mount_path.display()
                    );
                } else if mounted {
                    log::info!(
                        "{} {}: mounted but not yet alive host={} path={}",
                        share.name,
                        backend.short_label(),
                        host,
                        mount_path.display()
                    );
                }
            }
            Err(err) => {
                let message = err.to_string();
                if is_benign_mount_collision(&message) {
                    log::info!(
                        "{} {}: mount collision (non-fatal): {}",
                        share.name,
                        backend.short_label(),
                        message
                    );
                } else {
                    let msg = format!(
                        "{} {} mount failed: {}",
                        share.name,
                        backend.short_label(),
                        message
                    );
                    log::warn!("{}", msg);
                    last_error = Some(msg);
                }
            }
        }
    }

    if mounted && !alive {
        alive = mount::smb::is_mount_alive(&mount_path);
    }

    let ready = reachable && mounted && alive;
    BackendProbe {
        status: BackendStatus {
            host,
            mount_point: mount_path.display().to_string(),
            reachable,
            mounted,
            alive,
            ready,
            last_error,
        },
    }
}

fn select_shares<'a>(config: &'a Config, share_names: &[String]) -> Result<Vec<&'a ShareConfig>> {
    if share_names.is_empty() {
        return Ok(config.shares.iter().collect());
    }

    let mut out = Vec::new();
    for name in share_names {
        let share = config::find_share(config, name)
            .ok_or_else(|| anyhow!("share '{}' is not configured", name))?;
        out.push(share);
    }
    Ok(out)
}

fn backend_host(share: &ShareConfig, backend: Backend) -> &str {
    match backend {
        Backend::Tb => &share.thunderbolt_host,
        Backend::Fallback => &share.fallback_host,
    }
}

fn backend_ready(desired: Backend, tb: &BackendStatus, fb: &BackendStatus) -> bool {
    match desired {
        Backend::Tb => tb.ready,
        Backend::Fallback => fb.ready,
    }
}

fn is_benign_mount_collision(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("file exists")
        && (lower.contains("no detectable share path")
            || lower.contains("an error of type -5014")
            || lower.contains("execution error"))
}

fn state_entry_mut<'a>(state: &'a mut RuntimeState, share_name: &str) -> &'a mut ShareRuntimeState {
    state
        .shares
        .entry(share_name.to_ascii_lowercase())
        .or_default()
}

fn current_active_backend(
    config: &Config,
    state: &RuntimeState,
    share: &ShareConfig,
) -> Option<Backend> {
    let stable_path = config::share_stable_path(config, &share.name);
    detect_active_backend(config, share, &stable_path).or_else(|| {
        state
            .shares
            .get(&share.name.to_ascii_lowercase())
            .and_then(|entry| entry.active_backend)
    })
}

fn detect_active_backend(
    config: &Config,
    share: &ShareConfig,
    stable_path: &Path,
) -> Option<Backend> {
    let link_target = resolve_symlink_target(stable_path)?;

    let tb_target = config::backend_mount_path(config, &share.name, Backend::Tb);
    let fb_target = config::backend_mount_path(config, &share.name, Backend::Fallback);

    if path_eq(&link_target, &tb_target) {
        Some(Backend::Tb)
    } else if path_eq(&link_target, &fb_target) {
        Some(Backend::Fallback)
    } else {
        None
    }
}

fn inspect_alias(
    config: &Config,
    alias: &AliasConfig,
    seed_message: Option<String>,
) -> AliasStatus {
    let path = config::expand_path(&alias.path);
    let target = config::alias_target_path(config, alias);
    let current_target = resolve_symlink_target(&path).map(|p| p.display().to_string());
    let target_exists = target.exists();

    let healthy = current_target
        .as_deref()
        .map(PathBuf::from)
        .map(|resolved| path_eq(&resolved, &target) && target_exists)
        .unwrap_or(false);

    let message = if healthy || seed_message.is_some() {
        seed_message
    } else if current_target.is_none() {
        Some("alias path is missing or not a symlink".to_string())
    } else if !target_exists {
        Some("alias target does not currently exist".to_string())
    } else {
        Some("alias points to an unexpected target".to_string())
    };

    AliasStatus {
        name: alias.name.clone(),
        path: path.display().to_string(),
        share: alias.share.clone(),
        target_subpath: alias.target_subpath.clone(),
        target: target.display().to_string(),
        current_target,
        target_exists,
        healthy,
        message,
    }
}

fn set_symlink_atomically(target: &Path, link_path: &Path) -> Result<()> {
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }

    if link_path.exists() {
        let meta = fs::symlink_metadata(link_path)
            .with_context(|| format!("failed reading {}", link_path.display()))?;
        if !meta.file_type().is_symlink() {
            return Err(anyhow!(
                "{} exists and is not a symlink",
                link_path.display()
            ));
        }
        fs::remove_file(link_path)
            .with_context(|| format!("failed removing {}", link_path.display()))?;
    }

    let file_name = link_path
        .file_name()
        .ok_or_else(|| anyhow!("invalid symlink path {}", link_path.display()))?
        .to_string_lossy();
    let tmp_name = format!(".{}.tmp-{}", file_name, std::process::id());
    let tmp_path = link_path
        .parent()
        .ok_or_else(|| anyhow!("invalid symlink parent {}", link_path.display()))?
        .join(tmp_name);

    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    std::os::unix::fs::symlink(target, &tmp_path).with_context(|| {
        format!(
            "failed creating symlink {} -> {}",
            tmp_path.display(),
            target.display()
        )
    })?;

    fs::rename(&tmp_path, link_path).with_context(|| {
        format!(
            "failed replacing symlink {} -> {}",
            link_path.display(),
            target.display()
        )
    })?;

    Ok(())
}

fn resolve_symlink_target(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_link(path).ok()?;
    if raw.is_absolute() {
        return Some(raw);
    }
    Some(path.parent()?.join(raw))
}

fn path_eq(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }

    let ca = a.canonicalize().ok();
    let cb = b.canonicalize().ok();
    match (ca, cb) {
        (Some(ca), Some(cb)) => ca == cb,
        _ => false,
    }
}

fn has_open_handles(path: &Path) -> bool {
    let output = Command::new("lsof").arg("+D").arg(path).output();
    match output {
        Ok(output) => !output.stdout.is_empty(),
        Err(_) => false,
    }
}

fn is_symlink(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(err) => err.kind() != ErrorKind::NotFound && path.is_symlink(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    #[test]
    fn desired_backend_prefers_fallback_intent_when_tb_drops() {
        let now = Utc::now();
        let desired = choose_desired_backend(Some(Backend::Tb), false, false, true, None, 20, now);
        assert_eq!(desired, Some(Backend::Fallback));
    }

    #[test]
    fn desired_backend_stays_fallback_intent_when_both_down() {
        let now = Utc::now();
        let desired =
            choose_desired_backend(Some(Backend::Fallback), false, false, true, None, 20, now);
        assert_eq!(desired, Some(Backend::Fallback));
    }

    #[test]
    fn desired_backend_failbacks_after_stability_window() {
        let now = Utc::now();
        let healthy_since = now - ChronoDuration::seconds(31);
        let desired = choose_desired_backend(
            Some(Backend::Fallback),
            true,
            true,
            true,
            Some(healthy_since),
            30,
            now,
        );
        assert_eq!(desired, Some(Backend::Tb));
    }

    #[test]
    fn desired_backend_failbacks_when_tb_reachable_window_elapsed() {
        let now = Utc::now();
        let reachable_since = now - ChronoDuration::seconds(45);
        let desired = choose_desired_backend(
            Some(Backend::Fallback),
            true,
            true,
            true,
            Some(reachable_since),
            30,
            now,
        );
        assert_eq!(desired, Some(Backend::Tb));
    }

    #[test]
    fn benign_mount_collision_detection_matches_expected_patterns() {
        let benign = "mount_smbfs failed (exit 64): //u@macmini.local/CORE: File exists; osascript fallback mounted no detectable share path";
        assert!(is_benign_mount_collision(benign));

        let fatal = "mount_smbfs failed (exit 64): permission denied";
        assert!(!is_benign_mount_collision(fatal));
    }
}
