# Mountaineer V2 Requirements

## Summary
Mountaineer V2 keeps SMB shares usable across network interface changes by mounting each share via only ONE interface at a time and switching interfaces through an unmount-remount sequence. Because only one mount exists at any time, macOS always assigns the same volume path (`/Volumes/CORE`), eliminating the `CORE` vs `CORE-1` identity collision entirely.

Primary behavior:
- Prefer Thunderbolt SMB path (`10.10.10.1`) when healthy.
- Fail over to Bonjour/hostname SMB path when Thunderbolt is unavailable.
- Only one interface mounts a share at a time — this is the crux of the design.
- Expose a stable user path (`~/Shares/<SHARE> → /Volumes/<SHARE>`) that never changes.
- Support aliases for subfolders inside each share.
- Support multiple shares from config (`CORE`, `VAULT-R1`, etc.).
- Support explicit load/unload controls.
- User-controlled recovery to Thunderbolt when files are open; auto-recovery when no files are open.

Delivery is split into two phases:
- Phase 1: Full feature-complete CLI utility (single-shot commands only).
- Phase 2: Menu bar UI update that integrates all Phase 1 features.

## Problem
macOS SMB sessions remain bound to the interface/IP they were established on. Having the same volume mounted via two interfaces simultaneously causes macOS to label them `CORE` and `CORE-1`. This path identity collision breaks application file paths and workflows. Reconnecting via another address after a disconnect can also produce different volume identities.

## Goals
- Eliminate volume identity collisions by ensuring only one mount per share exists at a time.
- Provide seamless failover from Thunderbolt to fallback when TB drops.
- Provide user-controlled (or auto when safe) recovery back to Thunderbolt.
- Keep implementation transparent, observable, and recoverable.
- Ship all core behavior in an AI-friendly CLI surface before UI integration.
- Expose stable symlink paths for shares and subfolders.
- Allow users to manage favorites and perform bulk mount/unmount operations.

## Non-Goals
- True migration of already-open file descriptors between interfaces.
- Protocol-level SMB multichannel forcing.
- Kernel filesystem extension work.
- Interactive shell or REPL in Phase 1.
- Dual-mount mode (both interfaces mounted simultaneously) — explicitly rejected.

---

## Jobs To Be Done

### JTBD 1: Uninterrupted Work When Thunderbolt Drops (Failover)
**When** my Thunderbolt connection drops (undocking, cable failure), **I want** the app to unmount the TB-connected share and remount it via Bonjour/fallback on the same `/Volumes/CORE` path, **so that** my applications continue working at the same file paths with zero intervention.

#### User Stories
- As a user, I want the app to detect TB unavailability within seconds via TCP 445 probes, so failover is fast.
- As a user, I want the unmount-then-remount sequence to result in the same `/Volumes/CORE` path (no `CORE-1`), so apps see no path change.
- As a user, I want `~/Shares/CORE` to always point to `/Volumes/CORE`, which stays valid because the volume identity doesn't change.

### JTBD 2: User-Controlled Recovery to Thunderbolt (THE Critical JTBD)
**When** I reconnect to Thunderbolt while working on the fallback connection with files open, **I want** the app to notify me that TB is available and let me trigger the switch when I'm ready (after closing files), **so that** I get back on the fast connection without losing work or breaking open file handles.

#### User Stories
- As a user, I want the app to detect TB availability and check for open files on the current mount.
- As a user, I want a clear notification and "TB Ready" status in the menu bar when TB returns but files are open.
- As a user, I want to press a button in the tray menu (or run a CLI command) to trigger the switch when I've closed my files.
- As a user, I want the switch to unmount FB and remount via TB, landing at the same `/Volumes/CORE` path.
- As a user, I want auto-switch to happen silently when no files are open (configurable behavior with stability window).

#### Open Questions
- Should there be a macOS notification (banner/alert) in addition to the menu bar status?
- Should the app periodically re-check `lsof` and auto-switch once files close, or only on explicit user action?

### JTBD 3: Stable File Paths That Never Break
**When** I configure applications, scripts, IDE projects, or Finder bookmarks to access network share content, **I want** to use `~/Shares/CORE` as a permanent stable path that always resolves to the mounted share, **so that** nothing I configure ever needs updating.

#### User Stories
- As a user, I want a stable symlink `~/Shares/CORE → /Volumes/CORE` created when a share is first managed.
- As a user, I want this symlink to never need updating because the volume is always at `/Volumes/CORE` under single-mount mode.
- As a user, I want `~/Shares` to be openable in Finder as my central hub for all managed shares.

### JTBD 4: Simple Drive Lifecycle (Favorites)
**When** I want to add or remove a network share from management, **I want** a simple favorites system that handles all setup or teardown automatically, **so that** managing drives takes a single command.

#### User Stories
- As a user, I want `favorites add` to register a share with TB and fallback hosts, create `~/Shares/<NAME>`, and mount immediately.
- As a user, I want `favorites remove --cleanup` to unmount, remove the symlink, and stop monitoring.
- As a user, I want `favorites list` to show all managed drives with their connection details.

### JTBD 5: Quick Access to Subfolders (Aliases)
**When** I frequently access specific subfolders deep inside shares, **I want** named alias shortcuts that resolve through the stable share root, **so that** I have short, memorable paths to my most-used directories.

#### User Stories
- As a user, I want aliases like `~/Shares/Links/projects → ~/Shares/CORE/dev/projects`.
- As a user, I want aliases to survive interface switches because they resolve through `~/Shares/<SHARE>` which always points to `/Volumes/<SHARE>`.
- As a user, I want alias CRUD via CLI and eventually the tray UI.

### JTBD 6: Bulk Mount/Unmount
**When** I arrive at or leave my desk, **I want** to mount or unmount all managed drives in a single command, **so that** I can quickly set up or tear down my network environment.

#### User Stories
- As a user, I want `mount --all` to mount every favorited share via the best available interface.
- As a user, I want `unmount --all` to safely unmount all shares with open-file checks, deferring busy ones.

### JTBD 7: At-a-Glance Menu Bar Status
**When** I look at my menu bar, **I want** to see per-share health, active interface, and especially "TB Ready" status when Thunderbolt has returned, **so that** I always know my connection state and can act on it.

#### User Stories
- As a user, I want each share shown with a status indicator and active interface label.
- As a user, I want a prominent "TB Ready" / "TB Available" indicator when TB returns while on fallback.
- As a user, I want a "Switch to TB" button that triggers the unmount-remount sequence.
- As a user, I want an "Open Shares Folder" action.

### JTBD 8: Scriptable CLI
**When** I want to automate or script drive management, **I want** single-shot CLI commands with `--json` output, **so that** I can integrate Mountaineer into scripts and AI testing.

#### User Stories
- As a user, I want all operations available as CLI commands (reconcile, status, switch, mount, unmount, folders, alias, favorites).
- As a user, I want `--json` on status/verify/folders/alias/favorites for machine parsing.
- As a user, I want a `monitor` command for continuous terminal operation.

### JTBD 9: Continuous Background Monitoring
**When** my Mac is running with varying network conditions, **I want** Mountaineer to continuously monitor share health and drive failover/recovery decisions in the background, **so that** my shares are always in the best available state.

#### User Stories
- As a user, I want a reconciliation loop running every N seconds probing interface availability and mount health.
- As a user, I want runtime state persisted to disk so state survives restarts.
- As a user, I want config hot-reloaded so I can edit settings without restarting.

### JTBD 10: Launch at Login
**When** I start my Mac, **I want** Mountaineer to automatically start managing my shares, **so that** I never have to launch it manually.

#### User Stories
- As a user, I want `mountaineer install` to register a LaunchAgent.
- As a user, I want the app to run as a menu bar accessory (no dock icon).

### JTBD 11: Diagnosable Operations
**When** something goes wrong, **I want** detailed logs of all state transitions, mount attempts, and errors, **so that** I can diagnose issues.

#### User Stories
- As a user, I want logs at `~/Library/Logs/mountaineer.log`.
- As a user, I want `last_error` per-share in status output.
- As a user, I want CLI mode to also show logs on stderr.

---

### Phase 2: Menu Bar UI JTBDs

### JTBD 12: One-Click TB Recovery from the Menu Bar
**When** I see "TB Ready" in my menu bar after reconnecting to Thunderbolt, **I want** to click a single button to trigger the switch back to TB, **so that** I don't have to open a terminal and type a CLI command.

#### User Stories
- As a user, I want a prominent, visually distinct "Switch to TB" action in the share's submenu when TB is available.
- As a user, I want the menu to show a warning if files are open, with an option to force-switch anyway.
- As a user, I want immediate visual feedback (menu update) after the switch completes or fails.

### JTBD 13: Manage Favorites Without the Terminal
**When** I want to add a new network share or remove one I no longer use, **I want** to do it from the menu bar app, **so that** I never have to remember CLI syntax or open a terminal for drive management.

#### User Stories
- As a user, I want an "Add Favorite" flow that lets me enter share name, TB host, fallback host, and username.
- As a user, I want a "Remove Favorite" action per share with an option to clean up (unmount + remove symlink).
- As a user, I want the new favorite to start mounting immediately after I add it.

### JTBD 14: Manage Aliases from the Menu Bar
**When** I want to create a shortcut to a subfolder inside a share, **I want** to browse the share's folder tree and create an alias from the UI, **so that** I can set up shortcuts visually without constructing paths by hand.

#### User Stories
- As a user, I want to browse folders inside a share from the menu or a companion window.
- As a user, I want to select a folder and create a named alias with one action.
- As a user, I want to see existing aliases and remove them from the UI.

### JTBD 15: Bulk Mount/Unmount from the Menu Bar
**When** I arrive at or leave my desk, **I want** a single menu action to mount all or unmount all my managed shares, **so that** I can set up or tear down my entire network environment in one click.

#### User Stories
- As a user, I want a "Mount All" action in the tray menu.
- As a user, I want an "Unmount All" action that respects open-file safety (defers busy shares, shows which couldn't unmount).

### JTBD 16: Live Per-Share Status at a Glance
**When** I click the Mountaineer menu bar icon, **I want** to see real-time health, active interface, and any issues for every managed share, **so that** I know my network state without running CLI commands.

#### User Stories
- As a user, I want each share displayed with a clear status indicator (connected/disconnected) and which interface is active (TB or Fallback).
- As a user, I want "TB Ready" shown prominently when Thunderbolt has returned but I haven't switched yet.
- As a user, I want to see the last error for any share having problems.
- As a user, I want the menu to update in real-time as the reconciliation loop runs.

### JTBD 17: Quick Access Actions
**When** I want to open my shares folder, view logs, or change settings, **I want** quick-access actions in the tray menu, **so that** common tasks are one click away.

#### User Stories
- As a user, I want an "Open Shares Folder" action that opens `~/Shares` in Finder.
- As a user, I want an "Open Logs" action that opens `~/Library/Logs/mountaineer.log`.
- As a user, I want a way to toggle auto-failback on/off from the UI without editing config files.

---

## Core Design

### 1) Single-Mount Architecture (CRITICAL)
Only ONE interface mounts a share at any given time. This is the foundational design decision.

- When TB is healthy: share is mounted via TB at `/Volumes/CORE`.
- When TB drops: unmount `/Volumes/CORE`, remount via fallback. macOS assigns `/Volumes/CORE` again (no collision).
- When TB returns: check for open files, then unmount fallback and remount via TB.

This eliminates the `CORE` vs `CORE-1` problem entirely. The volume path never changes.

### 2) Stable User Path
For each share, Mountaineer manages one stable symlink:
- `~/Shares/CORE → /Volumes/CORE`
- `~/Shares/VAULT-R1 → /Volumes/VAULT-R1`

This symlink never needs to change because the volume is always at the same path under single-mount mode. It exists as a convenience — a predictable location under `~/Shares/` rather than requiring users to reference `/Volumes/` directly.

### 3) Managed Subfolder Aliases
User-defined symlink aliases for subfolders inside each share.

Examples:
- `~/Shares/Links/projects → ~/Shares/CORE/dev/projects`
- `~/Shares/Links/assets → ~/Shares/VAULT-R1/media/assets`

Rules:
- Aliases resolve through `~/Shares/<SHARE>/...` stable roots, never directly to `/Volumes/...`.
- Aliases survive interface switches because the underlying volume path doesn't change.
- Alias definitions are persisted in config and validated on reconcile.

### 4) Favorites as Managed Drive Source
Favorites are the canonical list of managed drives.

When a drive is added to favorites:
- A managed share entry is created with TB and fallback addressing.
- Stable share symlink is created under `~/Shares/<SHARE>`.
- Drive is included in bulk operations and reconcile logic.

When a drive is removed from favorites:
- Monitoring stops.
- Optional cleanup unmounts and removes the stable symlink.
- Dependent aliases are reported for cleanup.

### 5) Connectivity and Preference
Per share, interface preference order:
1. Thunderbolt host (e.g., `10.10.10.1`)
2. Fallback host (e.g., `macmini.local`)

Health checks:
- TCP connect test to SMB port `445`.
- Mount liveness check (`fs::metadata` with timeout).
- Stability window before auto-recovery (default 30s).

### 6) Recovery Policy (The Core Workflow)
When TB drops (failover):
- Unmount the TB mount, remount via fallback. Fast, automatic.

When TB returns (recovery):
- App detects TB reachable, checks open files via `lsof`.
- **No open files**: auto-switch (unmount FB, remount via TB) after stability window.
- **Open files (common case)**: show "TB Ready" in menu bar + notification. Wait for user to close files and trigger switch manually.
- User can always force-switch via CLI regardless of open files.

### 7) Open-File Safety
Before unmounting for a switch:
- Check for open handles (`lsof +D <mountpoint>`).
- If busy and not force: defer unmount, notify user.
- If force (user-initiated CLI `--force`): proceed with unmount.
- On mount failure after unmount: attempt rollback (remount the old interface).

## Config Model (TOML)

```toml
[global]
shares_root = "~/Shares"
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
- `mount_root` is removed — no longer needed since we don't maintain backend-specific mount directories.
- Credentials come from Keychain or existing SMB auth context.
- User-facing path is always `~/Shares/<name>`.
- The actual mount path is always `/Volumes/<share_name>`, managed by macOS.

## State Model
Per-share state:
- `active_interface`: `tb` or `fallback`
- `tb_reachable`: bool + `tb_reachable_since` timestamp
- `fb_reachable`: bool
- `mount_alive`: bool
- `tb_recovery_pending`: bool (TB available but files open, awaiting user action)
- `last_switch_at`: timestamp
- `last_error`: optional string

## Phase 1: CLI Utility (Feature Complete)

### Requirements
- Entire V2 backend behavior is delivered in CLI first.
- No interactive shell or REPL.
- Commands must be deterministic and automation-friendly for AI testing.
- Output supports machine parsing (`--json` where appropriate).

### CLI Commands
- `mountaineer reconcile --all`: single reconciliation pass for all shares.
- `mountaineer monitor --interval <secs>`: continuous monitor loop.
- `mountaineer status --all [--json]`: health, active interface, mount state, stable paths.
- `mountaineer switch --share <name> --to tb|fallback [--force]`: manual interface switch (--force bypasses open-file check).
- `mountaineer verify --share <name>|--all [--json]`: run health and mount checks only.
- `mountaineer mount --all`: mount all favorite-managed drives.
- `mountaineer unmount --all`: unmount all managed drives (safe mode with open-file checks).
- `mountaineer folders --share <name> [--subpath <dir>] [--json]`: list folders in a share.
- `mountaineer alias add --name <alias> --share <name> --target-subpath <path> [--alias-path <path>]`: create alias.
- `mountaineer alias list [--json]`: list aliases and health.
- `mountaineer alias remove --name <alias>`: remove alias.
- `mountaineer alias reconcile [--all]`: validate and repair alias symlinks.
- `mountaineer favorites add --share <name> --tb-host <ip> --fallback-host <host> --username <user> [--remote-share <name>]`: add favorite.
- `mountaineer favorites remove --share <name> [--cleanup]`: remove favorite.
- `mountaineer favorites list [--json]`: list favorites.
- `mountaineer install`: install LaunchAgent.
- `mountaineer uninstall`: remove LaunchAgent.

### Phase 1 Acceptance Criteria
- All core logic works end-to-end from CLI only.
- Commands are single-shot and scriptable (except `monitor`).
- Only one mount per share exists at any time — no `CORE-1` paths.
- Failover (TB → FB) works: unmount TB mount, remount via FB, same `/Volumes/CORE` path.
- Recovery (FB → TB) works: detects TB available, checks open files, switches when safe.
- "TB Ready" state is visible in `status` output when TB is available but files are open.
- `--force` on `switch` bypasses open-file checks for manual override.
- Aliases remain valid across interface switches.
- Bulk mount/unmount works for all favorites.

## Phase 2: Menu Bar App Integration

Phase 2 delivers JTBDs 12–17: full management through the menu bar without needing a terminal.

### Requirements
- Menu bar app consumes Phase 1 engine/services — all UI actions call the same engine functions as CLI.
- UI exposes real-time per-share status with active interface label and health indicators (JTBD 16).
- "TB Ready" state is prominently visible with a one-click "Switch to TB" action (JTBD 12).
- Favorites management: add/remove shares from the UI (JTBD 13).
- Alias management: browse folders, create/remove aliases from the UI (JTBD 14).
- Bulk mount/unmount controls (JTBD 15).
- Quick-access actions: Open Shares Folder, Open Logs, toggle auto-failback (JTBD 17).
- UI remains optional; CLI remains fully functional and supported.

### Phase 2 Acceptance Criteria
- Menu bar state matches `mountaineer status --all` in real-time.
- "TB Ready" is prominently visible when TB returns while on fallback.
- "Switch to TB" action triggers the unmount-remount sequence and shows success/failure.
- Open-file warning shown before switch; option to force-switch from UI.
- Add/remove favorites from UI — behavior matches CLI `favorites add/remove`.
- Browse share folders and create/remove aliases from UI — behavior matches CLI `alias add/remove`.
- "Mount All" and "Unmount All" actions work with open-file safety.
- "Open Shares Folder" opens `~/Shares` in Finder.
- "Open Logs" opens the log file.
- Auto-failback toggle changes config without requiring a restart.
- UI actions map 1:1 to Phase 1 engine functions — no separate code paths.
- No regressions in CLI functionality.

## Logging and Observability
Log all state transitions and reconcile actions:
- Interface availability changes (TB up/down, FB up/down)
- Mount attempts/success/fail
- Open-file check results
- Interface switch events (failover, recovery, manual)
- Deferred recovery due to open files
- "TB Ready" state transitions

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
2. Run `mountaineer status --all --json` — confirm active interface = TB, mount at `/Volumes/CORE`.
3. Disconnect TB, run `mountaineer reconcile --all` — verify unmount + remount via FB, same `/Volumes/CORE` path, no `CORE-1`.
4. Reconnect TB, run `mountaineer status` — verify "TB Ready" / `tb_recovery_pending` state shown.
5. Run `mountaineer switch --share CORE --to tb` — if files open, verify it reports busy; with `--force`, verify it switches.
6. Verify both shares (`CORE`, `VAULT-R1`) behave independently.
7. Test alias add/list/remove and verify aliases survive failover/recovery.
8. Test `mount --all` and `unmount --all`.
9. Test `favorites add` for a new drive and verify `~/Shares/<SHARE>` is created.
10. Test `favorites remove --cleanup` and verify cleanup.

### Phase 2 Manual Functional Tests (UI)
1. Menu bar state matches CLI status.
2. "TB Ready" badge visible when TB returns while on fallback.
3. "Switch to TB" button triggers recovery sequence.
4. All UI actions match CLI behavior.

## Future Roadmap
- Dynamic discovery of all SMB shares (auto-enumerate and offer one-click add).
- macOS notification center integration for TB recovery alerts.
- Periodic `lsof` re-check with auto-switch once files close (optional behavior).
