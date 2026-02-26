# LaunchAgent Integration

## Purpose
Enables Mountaineer to start automatically at login and run as a menu bar accessory, so users never have to manually launch the app.

## Requirements
- `mountaineer install` creates a LaunchAgent plist at `~/Library/LaunchAgents/com.mountaineer.agent.plist`
- The plist configures the app to run at login (`RunAtLoad = true`)
- The launched process runs in menu bar accessory mode (no Dock icon)
- Set `RUST_LOG=info` in the plist environment
- Configure stdout/stderr to log to `~/Library/Logs/mountaineer.log`
- `mountaineer uninstall` removes the plist and unloads the agent
- Install command reports success/failure and the plist path
- Uninstall command is idempotent — no error if plist does not exist

## Constraints
- Plist lives at `~/Library/LaunchAgents/` (user-level, not system-level)
- The installed binary path must be resolved at install time (not hardcoded)
- Install/uninstall use `launchctl load`/`launchctl unload` (or `bootstrap`/`bootout` on newer macOS)

## Acceptance Criteria
1. `mountaineer install` creates a valid plist at `~/Library/LaunchAgents/com.mountaineer.agent.plist`
2. After install + login (or `launchctl load`), Mountaineer starts automatically
3. The running app has no Dock icon (menu bar accessory only)
4. `mountaineer uninstall` removes the plist and stops the agent
5. Running `uninstall` when no plist exists does not error
