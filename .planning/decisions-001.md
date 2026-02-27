---
session: "001"
summary: "Mountaineer V2 — single-mount SMB failover with user-controlled recovery"
reqs_file: ".planning/reqs-001.md"
created: "2026-02-24"
last_updated: "2026-02-27"
status: "decided"
---

# Decisions Log — Session 001

This log captures decisions, explorations, and preferences as they emerge during the Mountaineer V2 requirements session.

---

## CRITICAL: Single-Mount Architecture — Only One Interface at a Time

**Status:** decided
**Strength:** authoritative

**Options Considered:**
- **Dual-mount with symlink switching** — Keep both TB and FB mounts alive simultaneously, swap a symlink between them. This is what the reqs-001.md spec currently describes.
- **Single-mount with unmount-remount** — Only one interface mounts the share at a time. Unmount the old interface, then mount via the new one. macOS always assigns the same `/Volumes/CORE` path because there's no collision.

**Decision:** Single-mount. Only ONE interface mounts the share at any given time. This is the crux of the entire design.

**Rationale:** Having the same volume mounted via two interfaces simultaneously is what causes macOS to label them `CORE` and `CORE-1`. By ensuring only one mount exists at a time, the path is always `/Volumes/CORE` regardless of which interface was used. This eliminates the entire class of path-identity problems.

**Impact on existing reqs-001.md:** The spec's "Keep Both Backends Mounted" section (Section 2) is WRONG per this decision. The dual-backend mount strategy is explicitly rejected.

**Date:** 2026-02-24

---

## CRITICAL: Simplified Symlink — Stable Path Always Points to /Volumes/SHARE

**Status:** decided
**Strength:** authoritative

**Options Considered:**
- **Two-level symlink chain** — `~/Shares/CORE → ~/.mountaineer/mnts/core_tb → /Volumes/CORE` with middle-layer switching. This is what the current code implements.
- **Simple single symlink** — `~/Shares/CORE → /Volumes/CORE`, never changes because the volume path never changes under single-mount mode.

**Decision:** Simple single symlink. Since only one mount exists at a time and macOS always uses `/Volumes/CORE`, the stable symlink `~/Shares/CORE → /Volumes/CORE` never needs to change. The middle layer (`core_tb`/`core_fb`) is unnecessary overhead.

**Rationale:** The `core_tb`/`core_fb` indirection existed for dual-mount mode to distinguish which backend was active. In single-mount mode, which-interface-is-active is tracked in `state.json` (`active_backend` field), making the filesystem-level distinction redundant.

**Date:** 2026-02-24

---

## CRITICAL: User-Controlled Recovery — The Core Workflow

**Status:** decided
**Strength:** authoritative

**Decision:** When TB returns while on fallback:
1. App detects TB is reachable again.
2. App checks for open files on the current mount (`lsof`).
3. **If no open files** → app can auto-switch (unmount FB, remount via TB). User sees nothing.
4. **If open files exist (the common case)** → app shows notification + menu bar status "TB Ready". User closes their files, then presses a button or runs a CLI command to trigger the switch.
5. This user-driven recovery flow is the MOST CRITICAL requirement and JTBD.

**Rationale:** In practice, files will almost always be open when TB returns (the user was working the whole time on fallback). Forcing an auto-switch would be destructive. The user needs clear visibility ("TB is ready") and a deliberate action to trigger the transition.

**Date:** 2026-02-24

---

## Previous JTBD Draft — Superseded

**Status:** exploring → superseded
**Strength:** tentative → superseded

**Decision:** Initial 12 JTBDs drafted from codebase analysis need revision based on the three authoritative decisions above. Key changes needed:
- Former JTBD 1-4 (failover, stable paths, recovery, safe transitions) need to be rewritten around the single-mount + user-controlled recovery model
- The symlink management story is much simpler now
- "Safe transitions" and "automatic recovery" merge into one JTBD centered on user-controlled recovery with open-file awareness

**Date:** 2026-02-24

---

## Phase 2 UI: Full Management Surface, Not Just Status Display

**Status:** decided
**Strength:** authoritative

**Options Considered:**
- **Status-only menu bar** — Tray menu shows read-only status; all management done via CLI
- **Full management UI** — Tray menu is a complete management surface: switch interfaces, add/remove favorites, create/manage aliases, bulk mount/unmount, toggle settings

**Decision:** Full management UI. The menu bar app should let the user do everything without opening a terminal. CLI exists for automation and scripting; UI exists for daily human use.

**JTBDs added (12–17):**
- JTBD 12: One-click TB recovery from the menu bar
- JTBD 13: Manage favorites (add/remove) from the UI
- JTBD 14: Manage aliases (browse folders, create/remove) from the UI
- JTBD 15: Bulk mount/unmount from the menu bar
- JTBD 16: Live per-share status at a glance
- JTBD 17: Quick-access actions (Open Shares, Open Logs, toggle auto-failback)

**Rationale:** User explicitly stated: "I also want a working menu bar UI that I can manage these features through so I don't have to do it in the CLI."

**Date:** 2026-02-24

---

## Final JTBD Set — Written to reqs-001.md

**Status:** decided
**Strength:** authoritative

**Decision:** 17 JTBDs committed to reqs-001.md:

**Phase 1 (CLI + Engine):**
1. Uninterrupted Work When TB Drops (Failover)
2. User-Controlled Recovery to TB (THE Critical JTBD)
3. Stable File Paths That Never Break
4. Simple Drive Lifecycle (Favorites)
5. Quick Access to Subfolders (Aliases)
6. Bulk Mount/Unmount
7. At-a-Glance Menu Bar Status
8. Scriptable CLI
9. Continuous Background Monitoring
10. Launch at Login
11. Diagnosable Operations

**Phase 2 (Menu Bar UI Management):**
12. One-Click TB Recovery from Menu Bar
13. Manage Favorites Without Terminal
14. Manage Aliases from Menu Bar
15. Bulk Mount/Unmount from Menu Bar
16. Live Per-Share Status at a Glance
17. Quick Access Actions (Open Shares, Open Logs, Settings)

**Date:** 2026-02-24

---

## Config and State Path — `~/.mountaineer/`

**Status:** decided
**Strength:** authoritative

**Decision:** All Mountaineer config and state files live under `~/.mountaineer/`:
- `~/.mountaineer/config.toml` — user-edited configuration
- `~/.mountaineer/state.json` — machine-managed runtime state

**Rationale:** Code currently uses `~/.config/mountaineer/` but this decision standardizes on `~/.mountaineer/` for simplicity. Single directory, easy to find.

**Date:** 2026-02-27

---

## UI Framework — NOT GPUI

**Status:** decided
**Strength:** authoritative

**Decision:** The menu bar UI must NOT use GPUI. Use native Swift or a lightweight macOS-native framework instead.

**Rationale:** GPUI (from Zed) is too large a dependency for a menu-bar-only app. The current codebase uses GPUI (`gui.rs`, `tray.rs`) but this will be replaced.

**Date:** 2026-02-27

---

## `auto_failback` Default — `false`

**Status:** decided
**Strength:** authoritative

**Decision:** `auto_failback` defaults to `false`. User must explicitly enable auto-recovery.

**Rationale:** Safer default — user should opt in to automatic interface switching. The reqs config example previously showed `true` but has been updated to match this decision.

**Date:** 2026-02-27

---

## `lsof_recheck` — Toggleable Config Parameter

**Status:** decided
**Strength:** authoritative

**Decision:** Add `lsof_recheck` (default `true`) to `[global]` config. When enabled, the reconcile loop re-checks `lsof` each cycle for shares with `tb_recovery_pending` and auto-switches when files close. User can toggle on/off via CLI (`config set lsof-recheck on|off`) and menu bar UI.

**Rationale:** User explicitly requested periodic re-check with the ability to toggle it on/off from both CLI and menu bar.

**Date:** 2026-02-27

---

## macOS Notifications — Deferred

**Status:** decided
**Strength:** authoritative

**Decision:** macOS notification center integration (banners/alerts for TB recovery) is deferred to future roadmap. Menu bar indicator is sufficient for V2.

**Date:** 2026-02-27

---

## Failover Retry Policy

**Status:** decided
**Strength:** authoritative

**Decision:** If Fallback mount fails after TB has been unmounted during failover: retry the Fallback mount once. If the retry also fails, leave the share unmounted with `last_error` set. Do NOT attempt to remount TB (it was unreachable, which triggered failover). Next reconcile cycle will re-evaluate.

**Date:** 2026-02-27

---

## `mount-backends` Command — Remove

**Status:** decided
**Strength:** authoritative

**Decision:** The `mount-backends` CLI command must be removed. It is a dual-mount artifact that calls `engine::mount_backends_for_shares` and has no place in the single-mount architecture.

**Date:** 2026-02-27

---

## LaunchAgent `KeepAlive` — Restart on Crash Only

**Status:** decided
**Strength:** authoritative

**Decision:** Set `KeepAlive = { SuccessfulExit = false }` in the LaunchAgent plist. macOS will auto-restart Mountaineer on crash but NOT on clean quit.

**Date:** 2026-02-27

---

## LaunchAgent Binary Path — Hardcoded

**Status:** decided
**Strength:** authoritative

**Decision:** The plist binary path is hardcoded to `~/Applications/Mountaineer.app/Contents/MacOS/Mountaineer`. This is the standardized install location.

**Date:** 2026-02-27

---

## Favorites `add` — Reject Duplicates

**Status:** decided
**Strength:** authoritative

**Decision:** `favorites add` rejects duplicate share names with a clear error. If the share name already exists in favorites, the command fails. Users who need to change connection details (TB host, fallback host) should edit `config.toml` directly.

**Rationale:** A favorite registers a share for management. The same share mounted via TB or FB is the same favorite — the interface is selected automatically. Once registered, connection details rarely change.

**Date:** 2026-02-27

---

## Symlink Persistence on Unmount

**Status:** decided
**Strength:** authoritative

**Decision:** `unmount --all` must NOT remove `~/Shares/<SHARE>` symlinks. Symlinks represent the favorites list and persist through unmount/remount cycles. Symlinks are only removed via `favorites remove --cleanup`.

**Rationale:** Unmount is temporary (desk departure). Removing symlinks would break applications configured to use `~/Shares/<SHARE>` paths.

**Date:** 2026-02-27

---
