# Tray: Quick Actions

## Purpose
Provides one-click access to common tasks and settings toggles from the menu bar, eliminating the need to open a terminal or edit config files for everyday operations (JTBD 17).

## Requirements
- "Open Shares Folder" action: opens `~/Shares` in Finder
- "Open Logs" action: opens `~/Library/Logs/mountaineer.log` in the default text editor or Console.app
- Toggle for `auto_failback` (on/off): changes the config setting without requiring a restart
- Toggle for `lsof_recheck` (on/off): changes the config setting without requiring a restart
- Toggles update `~/.mountaineer/config.toml` and take effect on the next reconcile cycle
- Visual indication of current toggle state (checkmark, on/off label, or similar)
- "Quit" action to stop Mountaineer

## Constraints
- Toggle changes persist to config.toml — they are not ephemeral
- Config writes must not corrupt the file (atomic write or equivalent)
- Quick actions section should be visually separated from per-share sections in the menu

## Acceptance Criteria
1. "Open Shares Folder" opens `~/Shares` in Finder
2. "Open Logs" opens the log file
3. Auto-failback toggle changes the config value and is reflected in the menu immediately
4. Lsof-recheck toggle changes the config value and is reflected in the menu immediately
5. Neither toggle requires an app restart to take effect
6. "Quit" cleanly stops the Mountaineer process
