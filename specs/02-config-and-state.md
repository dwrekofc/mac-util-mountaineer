# Config and State

## Purpose
Defines the configuration model (TOML) and runtime state persistence (JSON) that all other components read and write. Config describes what the user wants; state describes what's currently happening.

## Requirements
- Load configuration from `~/.mountaineer/config.toml`
- Create default config with sensible defaults if file does not exist
- Support `[global]` section with: `shares_root`, `check_interval_secs`, `auto_failback`, `auto_failback_stable_secs`, `connect_timeout_ms`, `lsof_recheck`
- Support `[[shares]]` array with per-share: `name`, `username`, `thunderbolt_host`, `fallback_host`, `share_name`
- Support `[[aliases]]` array with per-alias: `name`, `path`, `share`, `target_subpath`
- Expand `~/` to the user's home directory in all path fields
- Persist runtime state to `~/.mountaineer/state.json`
- Runtime state tracks per-share: `active_interface` (tb|fallback), `tb_reachable` (bool), `tb_reachable_since` (timestamp), `fb_reachable` (bool), `mount_alive` (bool), `tb_recovery_pending` (bool), `last_switch_at` (timestamp), `last_error` (optional string)
- Support config hot-reload: detect changes to `config.toml` and apply without restart
- Save runtime state after every state-changing operation
- Validate config on load: reject missing required fields, duplicate share names, invalid hosts

## Constraints
- Config file is TOML; state file is JSON
- Both files live under `~/.mountaineer/`
- Config is user-edited; state is machine-managed (users should not edit state.json)
- Config changes take effect on next reconcile cycle (hot-reload)
- State must survive process restarts

## Acceptance Criteria
1. `~/.mountaineer/config.toml` is loaded and parsed without error
2. Default config is created on first run if no config file exists
3. `~/.mountaineer/state.json` is written after every state change
4. Adding a new `[[shares]]` entry to config.toml is picked up on next reconcile without restart
5. `lsof_recheck` field in `[global]` defaults to `true` and is toggleable
6. Invalid config (missing share name, duplicate names) produces a clear error message
7. `~/` is expanded to the absolute home directory path in all config path fields
