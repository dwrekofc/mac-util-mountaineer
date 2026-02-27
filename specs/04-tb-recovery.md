# TB Recovery

## Purpose
Manages the transition back from Fallback to Thunderbolt when TB becomes available again. This is the most critical user-facing workflow because files are typically open during recovery, requiring careful coordination between automatic detection, open-file safety, and user control.

## Requirements
- Detect TB availability via TCP 445 probe during each reconcile cycle
- Track `tb_reachable` and `tb_reachable_since` timestamp in runtime state
- Track `tb_healthy_since` separately — when TB is both reachable AND successfully mounted `[observed from code]`
- Apply a stability window (`auto_failback_stable_secs`, default 30s) before considering TB as stably available — prevents flapping
- When TB is stably available and share is on Fallback, check for open files via `lsof +D <mountpoint>`
- **No open files + auto_failback enabled**: auto-switch (unmount Fallback, remount via TB) silently
- **Open files detected**: set `tb_recovery_pending` = true, show "TB Ready" status — wait for user action or file closure
- Support periodic `lsof` re-check (every reconcile cycle) when `lsof_recheck` is enabled:
  - If files have closed since last check, auto-switch to TB
  - User can toggle `lsof_recheck` on/off via CLI and menu bar UI
- Support manual switch trigger via CLI (`switch --share <name> --to tb`) or menu bar button
- Support `--force` flag on manual switch to bypass open-file check
- On mount failure after unmounting Fallback: attempt rollback (remount via Fallback)
- After successful recovery, update state: `active_interface` = tb, `tb_recovery_pending` = false, `last_switch_at` = now
- Clear `tb_recovery_pending` when recovery completes or when TB becomes unreachable again

## Constraints
- Auto-switch only occurs when `auto_failback` is enabled in config AND stability window has passed
- `lsof_recheck` is a separate toggle from `auto_failback` — user may want TB Ready notification without auto-switch
- The stability window resets if TB becomes unreachable and then reachable again
- Force-switch is only available via explicit user action (CLI `--force` or UI force button)

## Acceptance Criteria
1. TB availability is detected within one reconcile cycle of TB becoming reachable
2. No auto-switch occurs before the stability window elapses
3. With no open files and auto_failback on, share auto-switches to TB silently
4. With open files, `tb_recovery_pending` is true and "TB Ready" appears in status output
5. Periodic lsof re-check auto-switches when files close (when `lsof_recheck` is enabled)
6. `lsof_recheck` can be toggled on/off via CLI and UI without restart
7. `switch --to tb --force` succeeds even with open files
8. Failed mount after unmount triggers rollback to Fallback
9. `tb_recovery_pending` clears when recovery completes or TB drops again

## References
- `.planning/reqs-001.md` — JTBD 2 (THE Critical JTBD), Core Design §6 (Recovery Policy), §7 (Open-File Safety)
- `.planning/decisions-001.md` — User-Controlled Recovery decision

## Notes
- **`--force` flag not implemented on CLI switch** `[observed from code]`: The `Switch` CLI command struct accepts `share` and `to` but does not include a `--force` flag. The engine's `switch_backend_single_mount` does accept a `force` parameter and skips open-file checks when true, but the CLI has no way to pass `true` currently.
- **`lsof_recheck` toggle not implemented** `[observed from code]`: The engine always re-checks lsof each reconcile cycle when `tb_recovery_pending` is true (via `has_open_handles()` in `switch_backend_single_mount`). There is no config toggle to disable this behavior. See also spec 02 Notes.
- **Recovery only triggers with `auto_failback` enabled** `[observed from code]`: When `auto_failback=false` (the default), the reconcile loop sets `tb_recovery_pending=true` but does NOT attempt auto-switch even if files close. The user must manually trigger via `switch --to tb`. When `auto_failback=true`, the engine checks the stability window and then calls `switch_backend_single_mount` which checks lsof internally. The `lsof_recheck` spec requirement (auto-switch when files close regardless of `auto_failback`) is NOT yet implemented — it requires adding `lsof_recheck` as a separate config toggle that triggers auto-switch independently of `auto_failback`.
