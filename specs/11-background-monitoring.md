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
- **SCDynamicStore wired to reconcile** `[RESOLVED P2]`: Was: tray reconcile loop did not consume network events. Now both tray and CLI monitor consume SCDynamicStore events via a dedicated network bridge thread.
- **500ms debounce implemented** `[RESOLVED P2]`: Was: debounce only in V1 dead code. Now implemented in V2 — network bridge thread debounces SCDynamicStore events at 500ms.
- **Config hot-reload via re-read** `[observed from code]`: Both tray and CLI monitor reconcile loops reload config from disk every cycle (`config::Config::load()`). This polling approach is acceptable — achieves the functional goal without adding a file watcher dependency.
- **State persistence atomic** `[RESOLVED P1]`: Was: `save_runtime_state` used non-atomic `fs::write`. Now uses temp-then-rename for crash safety.
- **V1 `watcher.rs` removed** `[RESOLVED P3]`: Was: dead code V1 watch loop. File removed. V1 functions in `discovery.rs` pruned.
- **`monitor` CLI config hot-reload** `[RESOLVED P1]`: Was: `cmd_monitor` loaded config once at startup. Now re-reads config each cycle, matching tray behavior.
