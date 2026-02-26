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
- **SCDynamicStore implemented but not connected to reconcile** `[observed from code]`: `network/monitor.rs` fully implements an SCDynamicStore watcher that watches IPv4/IPv6/Link changes and sends events to an mpsc channel. However, the tray reconciliation loop in `tray.rs` uses a fixed-interval timer (`check_interval_secs`) and does not consume network events. The watcher was used in the V1 `watcher.rs` (now dead code) but is not integrated into the V2 reconcile path. Build task: wire network events into the tray reconcile loop.
- **500ms debounce exists in dead code only** `[observed from code]`: The debounce logic (drain events within 500ms) exists in `watcher.rs` but that file is V1 dead code. The V2 tray reconcile loop does not implement debouncing.
- **Config hot-reload via re-read, not file watcher** `[observed from code]`: The tray reconcile loop reloads config from disk on every cycle (`config::Config::load()`). There is no file watcher (fsnotify/kqueue). This achieves the functional goal (changes picked up on next cycle) but via polling rather than event-driven reload.
- **State persistence** `[observed from code]`: `engine::save_runtime_state` writes state.json using `serde_json::to_string_pretty` + `fs::write`. This is not atomic (no temp-then-rename). A crash during write could corrupt the file. The spec's constraint says "State is written atomically to prevent corruption on crash."
