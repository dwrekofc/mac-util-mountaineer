# Mountaineer V2 Spec

## Summary
Mountaineer V2 keeps SMB shares usable across interface changes by maintaining two backend mounts per share and exposing one stable app-facing path per share.

Primary behavior:
- Prefer Thunderbolt SMB path (`10.10.10.1`) when healthy.
- Fail over to Bonjour/hostname SMB path when Thunderbolt is unavailable.
- Keep a stable user path (`~/Shares/<SHARE>`) so apps use one location.
- Support stable failover-safe aliases for subfolders inside each share (for example `~/Shares/Links/projects`).
- Support multiple shares from config (`CORE`, `VAULT-R1`, etc.).
- Support explicit load/unload controls to mount or unmount all managed drives on demand.
- Auto-failback to Thunderbolt after a configurable healthy period.

Delivery is split into two phases:
- Phase 1: Full feature-complete CLI utility (no interactive/REPL modes, single-shot commands only).
- Phase 2: Menu bar UI update that integrates all Phase 1 features.

## Problem
macOS SMB sessions can remain bound to an interface/IP path. Reconnecting via another address can produce a different mounted volume identity (`/Volumes/CORE` vs `/Volumes/CORE-1`), which can break app file paths and workflows.

## Goals
- Provide near-seamless transition between Thunderbolt and fallback network paths.
- Minimize remount churn and volume-identity surprises for user workflows.
- Keep implementation transparent, observable, and recoverable.
- Ship all core behavior first in an AI-friendly CLI surface before UI integration.
- Ensure users can work from stable symlink paths for both share roots and selected subfolders, avoiding direct `/Volumes/...` usage.
- Allow users to add/remove favorites over time as network drives change, with simple load/unload of all managed volumes.

## Non-Goals
- True migration of already-open file descriptors between backends.
- Protocol-level SMB multichannel forcing on Apple SMB server.
- Kernel filesystem extension work in V2.
- Interactive shell or REPL command mode in Phase 1.

## Core Design

### 1) Stable User Path
For each share, Mountaineer manages one stable symlink:
- `~/Shares/CORE`
- `~/Shares/VAULT-R1`

Each symlink points to exactly one active backend mount target at a time:
- Thunderbolt backend mountpoint (example: `~/.mountaineer/mnts/core_tb`)
- Fallback backend mountpoint (example: `~/.mountaineer/mnts/core_fb`)

### 1b) Managed Subfolder Aliases
Mountaineer can also manage user-defined symlink aliases for subfolders inside each share.

Examples:
- `~/Shares/Links/projects` -> `~/Shares/CORE/dev/projects`
- `~/Shares/Links/assets` -> `~/Shares/VAULT-R1/media/assets`

Rules:
- Aliases are always resolved through `~/Shares/<SHARE>/...` stable roots, never directly to `/Volumes/...`.
- Alias targets remain valid across backend switches because share root symlinks are switched atomically.
- Alias definitions are persisted in config and validated on reconcile.

### 1c) Favorites as Managed Drive Source
Favorites are the canonical list of managed drives.

When a drive is added to favorites:
- A managed share entry is created (or updated) with TB and fallback addressing.
- Stable share symlink is created under `~/Shares/<SHARE>`.
- Drive is included in `mount all` / `unmount all` operations and reconcile logic.

When a drive is removed from favorites:
- Managed monitoring/reconcile for that drive stops.
- Optional cleanup can unmount backend mountpoints and remove its stable symlink.
- Existing user aliases that depend on that share are reported for cleanup/repair.

### 2) Keep Both Backends Mounted
Mountaineer attempts to keep both backend mounts alive (when reachable and credentials valid), so switching the stable path is fast and does not require a full cold mount on every transition.

### 3) Connectivity and Preference
Per share, backend preference order:
1. Thunderbolt host: `10.10.10.1`
2. Fallback host: Bonjour name (example: `macmini.local`)

Health checks:
- TCP connect test to SMB port `445`.
- Mountpoint state check.
- Optional debounce/stability window before failback.

### 4) Switch Policy
- On TB unhealthy: switch stable symlink to fallback backend quickly.
- On TB healthy again:
  - If `auto_failback=true`, wait `auto_failback_stable_secs` (default 30s), then switch back.
  - If `auto_failback=false`, remain on fallback until manual switch.

### 5) Open-File Safety
Before unmounting an inactive backend mountpoint:
- Check for open handles (`lsof +D <mountpoint>`).
- If busy, defer unmount.
- Never force unmount the currently active backend target for a share.

## Config Model (TOML)

```toml
[global]
shares_root = "~/Shares"
mount_root = "~/.mountaineer/mnts"
check_interval_secs = 2
auto_failback = true
auto_failback_stable_secs = 30
connect_timeout_ms = 800

[[shares]]
name = "CORE"
username = "I852000"
thunderbolt_host = "10.10.10.1"
fallback_host = "macmini.local"
share_name = "CORE"

[[shares]]
name = "VAULT-R1"
username = "I852000"
thunderbolt_host = "10.10.10.1"
fallback_host = "macmini.local"
share_name = "VAULT-R1"

[[aliases]]
name = "projects"
path = "~/Shares/Links/projects"
share = "CORE"
target_subpath = "dev/projects"

[[aliases]]
name = "assets"
path = "~/Shares/Links/assets"
share = "VAULT-R1"
target_subpath = "media/assets"
```

Notes:
- Credentials should come from Keychain or existing SMB auth context.
- User-facing path is always `~/Shares/<name>`.
- Alias `target_subpath` is relative to the share root.
- Favorites persist as managed drive records and are the default input set for bulk operations.

## State Model
Per-share state:
- `tb_health`: up/down + last healthy timestamp
- `fb_health`: up/down
- `active_backend`: `tb` or `fallback`
- `last_switch_at`
- `last_error` (optional)

## Phase 1: CLI Utility (Feature Complete)

### Requirements
- Entire V2 backend behavior is delivered in CLI first.
- No interactive shell, no REPL, no prompt-driven command loop.
- Commands must be deterministic and automation-friendly for AI testing.
- Output supports machine parsing (`--json` where appropriate).

### CLI / Runtime Behavior
- `mountaineer reconcile --all`: single reconciliation pass for all shares.
- `mountaineer monitor --interval <secs>`: continuous monitor loop with auto-switch.
- `mountaineer status --all [--json]`: health, active backend, mount targets, stable paths.
- `mountaineer switch --share <name> --to tb|fallback`: manual backend switch.
- `mountaineer mount-backends --share <name>|--all`: ensure backend mounts exist.
- `mountaineer verify --share <name>|--all [--json]`: run health and mountpoint checks only.
- `mountaineer mount --all`: mount/load all favorite-managed drives.
- `mountaineer unmount --all`: unmount/unload all favorite-managed drives (safe mode with open-file checks).
- `mountaineer folders --share <name> [--subpath <dir>] [--json]`: list folders in a share via stable path.
- `mountaineer alias add --name <alias> --share <name> --target-subpath <path> [--alias-path <path>]`: create managed alias.
- `mountaineer alias list [--json]`: list configured aliases and health/target status.
- `mountaineer alias remove --name <alias>`: remove managed alias.
- `mountaineer alias reconcile [--all]`: validate and repair alias symlinks.
- `mountaineer favorites add --share <name> --tb-host <ip> --fallback-host <host> --username <user> [--remote-share <name>]`: add managed drive favorite.
- `mountaineer favorites remove --share <name> [--cleanup]`: remove managed drive favorite.
- `mountaineer favorites list [--json]`: list favorite-managed drives and their state.

### Phase 1 Acceptance Criteria
- All V2 core logic works end-to-end from CLI only.
- Commands are single-shot and scriptable (except explicit `monitor` long-running mode).
- AI/test automation can execute setup, failover, failback, and verification via commands only.
- No dependency on tray/menu-bar UI to validate functionality.
- CLI can enumerate share folders and create/remove managed subfolder aliases.
- Alias symlinks remain valid when active backend changes.
- CLI can mount/unmount all managed drives in a single command.
- Favorites add/remove fully controls which drives are managed and symlinked under `~/Shares`.

## Phase 2: Menu Bar App Integration

### Requirements
- Existing menu bar app consumes Phase 1 engine/services.
- UI exposes status, manual switch, and policy controls (auto-failback vs manual failback).
- UI remains optional; CLI remains fully functional and supported.
- UI includes alias management to browse share folders, create/edit/remove alias symlinks, and open alias targets in Finder.
- UI includes bulk controls to mount all and unmount all managed drives.
- UI includes favorites management to add/remove drives and immediately include/exclude them from managed failover behavior.

### Phase 2 Acceptance Criteria
- Menu bar app reflects the same per-share state as CLI.
- UI actions map directly to Phase 1 command/service behaviors.
- No regressions in CLI functionality.
- Aliases created in UI match CLI alias behavior and survive failover/failback transitions.
- Mount all / unmount all from UI behaves equivalently to CLI bulk commands.
- Adding/removing favorites in UI creates/removes managed share symlinks and updates reconcile scope.

## Logging and Observability
Log all state transitions and reconcile actions:
- health changes
- mount attempts/success/fail
- symlink switch events
- deferred unmount due to open handles

## Acceptance Criteria
- User opens files via `~/Shares/<SHARE>/...` only.
- Pulling Thunderbolt does not change app path prefix.
- On TB return, auto-failback occurs only after configured stability period.
- No duplicate user-facing mount paths (`CORE`, `CORE-1`) are required in normal operation.
- Multiple shares in config are reconciled independently.
- Managed alias paths (for example `~/Shares/Links/...`) continue to resolve after backend switches.
- User can intentionally load/unload all managed volumes at will.
- New drives can be added as favorites and immediately become managed with stable symlink paths and dual-path failover settings.

## Test Plan

### Automated Quality Gates
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build
```

### Phase 1 Manual Functional Tests (CLI)
1. Start with TB + fallback both available.
2. Run `mountaineer status --all --json` and confirm active backend = TB and path uses `~/Shares/<SHARE>`.
3. Disconnect TB and run `mountaineer reconcile --all`; verify active backend switches to fallback without path change.
4. Reconnect TB and verify:
- no immediate flap
- switch back occurs after stable threshold (default 30s) when enabled.
5. Verify both configured shares (`CORE`, `VAULT-R1`) behave independently via CLI status.
6. Run `mountaineer folders --share CORE --subpath dev --json` and verify folder listing works.
7. Run `mountaineer alias add --name projects --share CORE --target-subpath dev/projects` and verify alias path opens expected content.
8. Repeat failover/failback and verify alias path remains usable without changes.
9. Run `mountaineer mount --all` and verify all favorites are mounted.
10. Run `mountaineer unmount --all` and verify managed volumes unload cleanly.
11. Run `mountaineer favorites add ...` for a new drive and verify `~/Shares/<SHARE>` is created and managed.
12. Run `mountaineer favorites remove --share <name> --cleanup` and verify it is removed from management.

### Phase 2 Manual Functional Tests (UI)
1. Menu bar state matches `mountaineer status --all`.
2. Trigger manual switch from UI and confirm backend/path transition.
3. Toggle auto-failback policy from UI and validate behavior matches CLI policy state.
4. Create alias from UI folder browser and verify path exists and resolves correctly.
5. Fail over and fail back; verify UI-created alias still resolves correctly.
6. Use UI "Mount All" and "Unmount All" and verify behavior matches CLI.
7. Add a new favorite in UI and verify managed symlink + failover behavior are active.
8. Remove a favorite in UI and verify it is excluded from bulk ops and reconcile.

## Future Roadmap
- Dynamic discovery of all SMB shares (auto-enumerate available shares and offer one-click add to managed config).
