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
- If Fallback is also unreachable, log the error and set `last_error` on the share — do not leave the share in a half-unmounted state if possible
- Failover is automatic and fast — no user interaction required
- After successful failover, update runtime state: `active_interface` = fallback, `last_switch_at` = now
- The resulting volume path must be identical (`/Volumes/<SHARE>`) — no `-1` suffix
- `~/Shares/<SHARE>` symlink continues to resolve correctly because the volume path is unchanged
- Each share fails over independently — one share's TB failure does not affect others

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
6. Multiple shares fail over independently
