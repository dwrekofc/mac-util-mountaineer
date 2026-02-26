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
