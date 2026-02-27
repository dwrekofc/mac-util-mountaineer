# Failover

## Purpose
Automatically switches a share from Thunderbolt to Fallback when the Thunderbolt connection drops, preserving the same `/Volumes/<SHARE>` path so applications see no change. This is the primary reliability mechanism.

## Requirements
- Detect Thunderbolt unavailability via TCP connect probe to SMB port 445 on the TB host
- Use configurable connect timeout (`connect_timeout_ms`, default 800ms)
- When TB is detected as unreachable and the share is currently mounted via TB:
  1. Unmount the TB mount at `/Volumes/<SHARE>`
  2. Remount via Fallback host at the same `/Volumes/<SHARE>` path
- Verify Fallback host is reachable before attempting remount
- If Fallback is also unreachable before unmounting TB, log the error and set `last_error` — do not unmount TB (nothing to fail over to)
- If Fallback mount fails after TB has already been unmounted, retry the Fallback mount once, then leave unmounted with `last_error` set if second attempt also fails
- Failover is automatic and fast — no user interaction required
- After successful failover, update runtime state: `active_interface` = fallback, `last_switch_at` = now
- The resulting volume path must be identical (`/Volumes/<SHARE>`) — no `-1` suffix
- `~/Shares/<SHARE>` symlink continues to resolve correctly because the volume path is unchanged
- Each share fails over independently — one share's TB failure does not affect others
- Detect and clean up stale mounts: if a mount point exists but `fs::metadata` times out (mount is hung), unmount it before attempting remount `[observed from code]`

## Constraints
- Failover only triggers when the currently active interface becomes unreachable
- Never mount both interfaces simultaneously — unmount first, then remount
- TCP 445 probe is the health check mechanism (not ping, not DNS)
- Failover runs as part of the reconciliation cycle

## Acceptance Criteria
1. When TB drops, the share is unmounted from TB and remounted via Fallback within one reconcile cycle
2. The remounted volume is at `/Volumes/<SHARE>` (no `CORE-1` or similar suffix)
3. `~/Shares/<SHARE>` remains valid and resolves to the mounted share
4. `active_interface` in state.json reads `fallback` after failover
5. `last_error` is set if both TB and Fallback are unreachable
6. If Fallback mount fails after TB unmount, Fallback mount is retried once before giving up
7. Multiple shares fail over independently

## Notes
- **Two-phase mount strategy** `[observed from code]`: `mount::smb::mount_share` first attempts to mount via `osascript` (AppleScript Finder `mount volume` command), then falls back to `mount_smbfs` if that fails. The osascript approach integrates with macOS Keychain for authentication. This strategy is not specified in any requirement but affects mount behavior and reliability.
- **Mount adoption** `[observed from code]`: If a share is already mounted at a different path (e.g., `/Volumes/CORE` exists from a previous session), the code adopts the existing mount rather than creating a duplicate. This prevents mount collisions but is not explicitly required by any spec.
- **Failover retry policy**: If Fallback mount fails after TB unmount, retry Fallback mount once. Do not attempt to remount TB (it was unreachable, which triggered failover). If retry fails, share remains unmounted with `last_error` until next reconcile cycle.

## References
- `.planning/reqs-001.md` — JTBD 1, Core Design §6 (Recovery Policy)
- `.planning/decisions-001.md` — Single-Mount Architecture decision
