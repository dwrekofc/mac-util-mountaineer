use anyhow::{Context, Result, anyhow};
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
    /// With auto_failback=false, the user must explicitly trigger the switch.
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
    pub tb_recovery_pending: bool,
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

    // Atomic write: write to .tmp then rename, so a crash mid-write won't corrupt state.json
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, text)
        .with_context(|| format!("failed writing temp state {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed renaming temp state to {}", path.display()))?;
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

/// Mount-only reconciliation: attempts to mount unmounted shares but does NOT
/// trigger failover or recovery on already-mounted shares (auto_switch=false).
/// Per spec 08: "Skip shares that are already mounted — do not unmount and remount."
pub fn mount_all(config: &Config, state: &mut RuntimeState) -> Vec<ShareStatus> {
    let now = Utc::now();
    let statuses: Vec<ShareStatus> = config
        .shares
        .iter()
        .map(|share| reconcile_share(config, state, share, true, false, now))
        .collect();
    let _ = reconcile_aliases(config);
    statuses
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

/// Result of a backend switch operation.
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

/// Switch backends: unmount old → mount new → update symlink.
/// Both backends mount at the same `/Volumes/<SHARE>` path under single-mount architecture.
/// Attempts rollback if the new mount fails.
pub fn switch_backend_single_mount(
    config: &Config,
    state: &mut RuntimeState,
    share: &ShareConfig,
    from: Backend,
    to: Backend,
    force: bool,
) -> SwitchResult {
    let mount_point = config::volume_mount_path(&share.share_name);
    let to_host = backend_host(share, to);
    let stable_path = config::share_stable_path(config, &share.name);

    // Step 1: Check for open files (unless force)
    if !force && mount::smb::is_mounted(&mount_point) && has_open_handles(&mount_point) {
        return SwitchResult::BusyOpenFiles;
    }

    // Step 2: Unmount old backend (if mounted)
    if mount::smb::is_mounted(&mount_point) {
        let unmount_result = if force {
            mount::smb::unmount(&mount_point)
        } else {
            mount::smb::unmount_graceful(&mount_point)
        };

        if let Err(e) = unmount_result {
            return SwitchResult::UnmountFailed(e.to_string());
        }
        log::info!(
            "{}: unmounted {} backend at {}",
            share.name,
            from.short_label(),
            mount_point.display()
        );
    }

    // Step 2.5: Pre-cleanup — detect and force-unmount stale mounts at the target path
    // per spec 03. A stale mount is one that is reported as mounted but is not alive
    // (hung/unresponsive). Without cleanup, mount_share would fail on an occupied path.
    if mount::smb::is_mounted(&mount_point) && !mount::smb::is_mount_alive(&mount_point) {
        log::warn!(
            "{}: stale mount detected at {}, force-unmounting before remount",
            share.name,
            mount_point.display()
        );
        if let Err(e) = mount::smb::unmount(&mount_point) {
            log::error!(
                "{}: failed to clean up stale mount at {}: {}",
                share.name,
                mount_point.display(),
                e
            );
        }
    }

    // Step 3: Mount new backend at the same /Volumes/<SHARE> path
    // Per spec 03: if mount fails, retry once before rolling back.
    let mount_result =
        mount::smb::mount_share(to_host, &share.share_name, &share.username, &mount_point);

    let mount_result = match mount_result {
        Err(first_err) => {
            log::warn!(
                "{}: first mount attempt for {} failed: {}, retrying once",
                share.name,
                to.short_label(),
                first_err
            );
            mount::smb::mount_share(to_host, &share.share_name, &share.username, &mount_point)
                .map_err(|retry_err| {
                    log::error!(
                        "{}: retry mount for {} also failed: {}",
                        share.name,
                        to.short_label(),
                        retry_err
                    );
                    retry_err
                })
        }
        ok => ok,
    };

    match mount_result {
        Ok(()) => {
            // Verify mount is alive
            if !mount::smb::is_mount_alive(&mount_point) {
                log::warn!(
                    "{}: {} mounted but not responding, will retry",
                    share.name,
                    to.short_label()
                );
            }

            // Step 4: Update symlink (~/Shares/<SHARE> -> /Volumes/<SHARE>)
            if let Err(e) = set_symlink_atomically(&mount_point, &stable_path) {
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
                mount_point.display(),
                error_msg
            );

            // Step 5: Rollback - try to remount old backend
            let from_host = backend_host(share, from);
            let rollback_result = mount::smb::mount_share(
                from_host,
                &share.share_name,
                &share.username,
                &mount_point,
            );

            let rolled_back = rollback_result.is_ok();
            if rolled_back {
                log::info!(
                    "{}: rolled back to {} after failed switch",
                    share.name,
                    from.short_label()
                );
                // Restore symlink (target unchanged since both use /Volumes/<SHARE>)
                let _ = set_symlink_atomically(&mount_point, &stable_path);
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

pub fn unmount_all(config: &Config, state: &mut RuntimeState, force: bool) -> Vec<UnmountResult> {
    let mut results = Vec::new();

    for share in &config.shares {
        let active_backend = current_active_backend(config, state, share);
        let mount_point = config::volume_mount_path(&share.share_name);
        let mounted = mount::smb::is_mounted(&mount_point);
        let mut result = UnmountResult {
            share: share.name.clone(),
            backend: active_backend.unwrap_or(Backend::Tb),
            mount_point: mount_point.display().to_string(),
            attempted: mounted,
            unmounted: false,
            busy: false,
            message: None,
        };

        if !mounted {
            // not mounted, nothing to do
        } else if !force && has_open_handles(&mount_point) {
            result.busy = true;
            result.message = Some("deferred: open files detected".to_string());
        } else {
            let unmount_result = if force {
                mount::smb::unmount(&mount_point)
            } else {
                mount::smb::unmount_graceful(&mount_point)
            };
            match unmount_result {
                Ok(()) => {
                    result.unmounted = true;
                    let method = if force { "forcefully" } else { "gracefully" };
                    result.message = Some(format!("unmounted {}", method));
                }
                Err(err) => {
                    result.message = Some(format!("unmount failed: {}", err));
                }
            }
        }

        // Stable symlinks are preserved across unmount per spec 05/08.
        // They are only removed on explicit `favorites remove --cleanup`.

        if result.unmounted {
            let entry = state_entry_mut(state, &share.name);
            entry.active_backend = None;
            entry.last_error = None;
        }

        results.push(result);
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
    let alias = config.aliases.remove(idx);

    // Clean up the alias symlink on disk (spec 01: all filesystem ops through engine)
    let alias_path = config::expand_path(&alias.path);
    if alias_path.is_symlink()
        && let Err(e) = std::fs::remove_file(&alias_path)
    {
        log::warn!(
            "Failed to remove alias symlink {}: {}",
            alias_path.display(),
            e
        );
    }

    Ok(alias)
}

/// Add a new share to the config. Returns an error if a share with the same name
/// already exists (case-insensitive). Per spec 06, duplicate share names must be
/// rejected on add — users should edit config.toml directly to modify existing shares.
pub fn add_share(config: &mut Config, share: ShareConfig) -> Result<()> {
    if config::find_share(config, &share.name).is_some() {
        return Err(anyhow!(
            "favorite '{}' already exists. edit ~/.mountaineer/config.toml to modify it",
            share.name
        ));
    }
    config.shares.push(share);
    Ok(())
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
    let active_backend = config::find_share(config, share_name)
        .and_then(|share| current_active_backend(config, state, share));

    // Find the share_name on the remote to determine the volume mount path.
    // Use the share's `share_name` field if available, otherwise fall back to `share_name` param.
    let remote_name = config::find_share(config, share_name)
        .map(|s| s.share_name.as_str())
        .unwrap_or(share_name);
    let mount_point = config::volume_mount_path(remote_name);
    let mounted = mount::smb::is_mounted(&mount_point);
    let mut result = UnmountResult {
        share: share_name.to_string(),
        backend: active_backend.unwrap_or(Backend::Tb),
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
        } else {
            match mount::smb::unmount_graceful(&mount_point) {
                Ok(()) => {
                    result.unmounted = true;
                    result.message = Some("unmounted gracefully".to_string());
                }
                Err(err) => {
                    result.message = Some(format!("unmount failed: {}", err));
                }
            }
        }
    }

    state_entry_mut(state, share_name).active_backend = None;
    vec![result]
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

    let stable_path = config::share_stable_path(config, &share.name);
    let detected_active = detect_active_backend(state, &share.name);
    let remembered_active = state
        .shares
        .get(&share.name.to_ascii_lowercase())
        .and_then(|entry| entry.active_backend);
    let active_hint = detected_active.or(remembered_active);

    // Probe both backends (always check reachability for status display)
    // Only the active backend will attempt to mount
    let tb = probe_backend(share, Backend::Tb, timeout, attempt_mount, active_hint);
    let fb = probe_backend(
        share,
        Backend::Fallback,
        timeout,
        attempt_mount,
        active_hint,
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

    let desired_backend = choose_desired_backend(
        active_backend,
        tb.status.reachable,
        fb.status.reachable,
        config.global.auto_failback,
        tb_stability_since,
        config.global.auto_failback_stable_secs,
        now,
    );

    let mut last_error = None;

    if auto_switch {
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
                            // When lsof_recheck is disabled, skip open-file checks
                            // during auto-failback per spec 04
                            let skip_lsof = !config.global.lsof_recheck;
                            match switch_backend_single_mount(
                                config,
                                state,
                                share,
                                Backend::Fallback,
                                Backend::Tb,
                                skip_lsof,
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
            // No active backend - do initial mount at /Volumes/<SHARE>
            let host = backend_host(share, desired);
            let mount_path = config::volume_mount_path(&share.share_name);
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
        tb_recovery_pending: entry.tb_recovery_pending,
        tb: tb.status,
        fallback: fb.status,
        last_switch_at: entry.last_switch_at,
        last_error: last_error.or_else(|| entry.last_error.clone()),
    }
}

/// Choose desired backend based on reachability (since only the active backend is mounted).
fn choose_desired_backend(
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
            if tb_reachable
                && auto_failback
                && let Some(since) = tb_stability_since
            {
                let stable_for = (now - since).num_seconds().max(0) as u64;
                if stable_for >= failback_stable_secs {
                    return Some(Backend::Tb);
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
    share: &ShareConfig,
    backend: Backend,
    timeout: Duration,
    attempt_mount: bool,
    active_backend: Option<Backend>,
) -> BackendProbe {
    let host = backend_host(share, backend).to_string();
    let mount_path = config::volume_mount_path(&share.share_name);

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

    // Only mount if this is the active backend (or no backend is active yet)
    let should_mount =
        attempt_mount && (active_backend.is_none() || active_backend == Some(backend));

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
    _config: &Config,
    state: &RuntimeState,
    share: &ShareConfig,
) -> Option<Backend> {
    // Under single-mount architecture, both backends mount at /Volumes/<SHARE>.
    // The symlink target is always the same path, so we rely on RuntimeState.
    state
        .shares
        .get(&share.name.to_ascii_lowercase())
        .and_then(|entry| entry.active_backend)
}

/// Detect active backend from persisted state.
/// Under single-mount architecture, both backends mount at the same /Volumes/<SHARE> path,
/// so symlink inspection cannot distinguish them. We rely on RuntimeState exclusively.
fn detect_active_backend(state: &RuntimeState, share_name: &str) -> Option<Backend> {
    state
        .shares
        .get(&share_name.to_ascii_lowercase())
        .and_then(|entry| entry.active_backend)
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

pub(crate) fn set_symlink_atomically(target: &Path, link_path: &Path) -> Result<()> {
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
    fn desired_backend_prefers_fallback_when_tb_drops() {
        let now = Utc::now();
        let desired = choose_desired_backend(Some(Backend::Tb), false, false, true, None, 20, now);
        assert_eq!(desired, Some(Backend::Fallback));
    }

    #[test]
    fn desired_backend_stays_fallback_when_both_down() {
        let now = Utc::now();
        let desired =
            choose_desired_backend(Some(Backend::Fallback), false, false, true, None, 20, now);
        assert_eq!(desired, Some(Backend::Fallback));
    }

    #[test]
    fn desired_backend_failbacks_after_stability_window() {
        let now = Utc::now();
        let reachable_since = now - ChronoDuration::seconds(31);
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
    fn desired_backend_prefers_tb_when_none_active() {
        let now = Utc::now();
        let desired = choose_desired_backend(None, true, true, false, None, 30, now);
        assert_eq!(desired, Some(Backend::Tb));
    }

    #[test]
    fn desired_backend_falls_back_to_fb_when_tb_unreachable() {
        let now = Utc::now();
        let desired = choose_desired_backend(None, false, true, false, None, 30, now);
        assert_eq!(desired, Some(Backend::Fallback));
    }

    #[test]
    fn desired_backend_none_when_both_unreachable() {
        let now = Utc::now();
        let desired = choose_desired_backend(None, false, false, false, None, 30, now);
        assert_eq!(desired, None);
    }

    #[test]
    fn benign_mount_collision_detection_matches_expected_patterns() {
        let benign = "mount_smbfs failed (exit 64): //u@macmini.local/CORE: File exists; osascript fallback mounted no detectable share path";
        assert!(is_benign_mount_collision(benign));

        let fatal = "mount_smbfs failed (exit 64): permission denied";
        assert!(!is_benign_mount_collision(fatal));
    }
}
