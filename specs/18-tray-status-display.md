# Tray: Status Display

## Purpose
Shows real-time per-share health, active interface, and recovery state in the menu bar so the user always knows their connection state at a glance (JTBD 16).

## Requirements
- Display each managed share in the tray menu with:
  - Share name
  - Status indicator (connected/disconnected/error)
  - Active interface label (Thunderbolt or Fallback)
  - "TB Ready" badge when `tb_recovery_pending` is true — must be visually prominent
  - Last error message if the share has a problem
- Update the menu in real-time as the background reconciliation loop runs
- The tray icon itself should reflect overall health (e.g., different icon states for all-healthy, some-degraded, all-disconnected)
- Menu state matches what `mountaineer status --all` would show — same data source
- Include `lsof_recheck` status (on/off) visible somewhere in the menu

## Constraints
- Status data comes from the shared engine status model (spec 09) — no separate data path
- Menu updates must not block the reconcile loop
- Menu rendering must handle 1-N shares dynamically (not hardcoded)

## Acceptance Criteria
1. Each share shows name, connection status, and active interface in the menu
2. "TB Ready" is prominently visible when TB recovery is pending
3. Last error is shown for shares with problems
4. Menu updates automatically as share state changes (no manual refresh needed)
5. Tray icon reflects overall system health
6. Menu content matches `mountaineer status --all` output
