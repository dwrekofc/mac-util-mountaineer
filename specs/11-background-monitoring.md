# Background Monitoring

## Purpose
Runs a continuous reconciliation loop that probes interface availability, checks mount health, and drives failover/recovery decisions in the background so shares are always in the best available state.

## Requirements
- Run a reconciliation loop at a configurable interval (`check_interval_secs`, default 2s)
- Each reconcile cycle: probe TB and FB reachability, check mount liveness, run failover/recovery logic
- Listen for macOS network change events via SCDynamicStore to trigger immediate reconciliation on interface changes
- Debounce rapid network events (e.g., 500ms window) to avoid thrashing
- Persist runtime state to `~/.mountaineer/state.json` after every state-changing operation
- Support config hot-reload: watch `~/.mountaineer/config.toml` for changes and apply on next cycle
- When `lsof_recheck` is enabled, include open-file checks in each reconcile cycle for shares with `tb_recovery_pending`
- The reconcile loop powers both `monitor` CLI command and the background engine for the menu bar app
- Log every state transition: interface up/down, mount attempt, failover, recovery, deferred recovery

## Constraints
- The reconcile loop must not block the UI thread when running in menu bar mode
- Network event listener runs on a dedicated background thread with CFRunLoop
- State is written atomically to prevent corruption on crash
- Config reload does not reset runtime state

## Acceptance Criteria
1. Reconciliation runs every `check_interval_secs` seconds
2. Network interface changes trigger an immediate reconcile cycle
3. Rapid network events are debounced (no multiple reconciles within debounce window)
4. State file is updated after every mount/unmount/failover/recovery
5. Config changes (e.g., adding a share, changing interval) take effect without restart
6. `lsof` re-check runs each cycle for recovery-pending shares when enabled
7. All state transitions are logged

## References
- `.planning/reqs-001.md` — JTBD 9

## Notes
- **SCDynamicStore not connected to reconcile** `[observed from code]`: `network/monitor.rs` fully implements an SCDynamicStore watcher that watches IPv4/IPv6/Link changes and sends events to an mpsc channel. However, the tray reconciliation loop in `tray.rs` uses a fixed-interval timer and does not consume network events. The watcher was used in V1 `watcher.rs` (now dead code). Must be wired into the V2 reconcile loop.
- **500ms debounce in dead code only** `[observed from code]`: The debounce logic exists in `watcher.rs` (V1 dead code). V2 reconcile loop must implement debouncing.
- **Config hot-reload via re-read** `[observed from code]`: The tray reconcile loop reloads config from disk every cycle (`config::Config::load()`). This polling approach is acceptable — it achieves the functional goal without adding a file watcher dependency.
- **State persistence not atomic** `[observed from code]`: `engine::save_runtime_state` writes state.json using `serde_json::to_string_pretty` + `fs::write`. Must be updated to atomic temp-then-rename to prevent corruption on crash.
- **V1 `watcher.rs` is dead code** `[observed from code]`: `watcher.rs` implements a V1 watch loop using `network::monitor` and `discovery::discover_mounted_shares()`. It is not called by any V2 code path (V2 uses `tray.rs` reconcile loop or `cmd_monitor`). Should be removed along with `discovery.rs` V1 functions (`discover_mounted_shares`, `discover_mac_address`, etc.) that are only used by the watcher.
- **`monitor` CLI re-reads config only once** `[observed from code]`: `cmd_monitor` loads config once at startup and reuses it for all cycles. It does not re-read config.toml each iteration, unlike the tray reconcile loop which re-reads every cycle. The monitor command needs config hot-reload to match the spec.
