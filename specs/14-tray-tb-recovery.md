# Tray: TB Recovery

## Purpose
Provides one-click Thunderbolt recovery from the menu bar when TB becomes available while on Fallback. This is the Phase 2 UI surface for the most critical user workflow (JTBD 12).

## Requirements
- Display a prominent, visually distinct "Switch to TB" action in the share's submenu when `tb_recovery_pending` is true
- Show a warning in the menu if files are currently open on the mount, including a count or summary
- Provide a "Force Switch" option to proceed despite open files
- After triggering the switch, show immediate visual feedback:
  - Switching in progress indicator
  - Success: menu updates to show TB as active interface
  - Failure: menu shows error message and reverts to showing Fallback as active
- The "Switch to TB" action calls the same engine `switch` function as the CLI `switch --to tb` command
- Hide the "Switch to TB" action when TB is not available or when already on TB

## Constraints
- UI calls engine functions — no separate switch logic in the UI layer
- macOS notification banners are NOT used (deferred to future roadmap)
- Menu bar indicator is the sole notification mechanism

## Acceptance Criteria
1. "Switch to TB" button appears in the share submenu when TB recovery is pending
2. Open-file warning is shown before switching
3. "Force Switch" option bypasses open-file check
4. Menu updates immediately after a successful switch (shows TB as active)
5. Menu shows an error if the switch fails
6. Button is hidden when TB is unavailable or already active
