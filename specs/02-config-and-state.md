# Config and State

## Purpose
Defines the configuration model (TOML) and runtime state persistence (JSON) that all other components read and write. Config describes what the user wants; state describes what's currently happening.

## Requirements
- Load configuration from `~/.mountaineer/config.toml`
- Create default config with sensible defaults if file does not exist
- Support `[global]` section with: `shares_root` (default `~/Shares`), `check_interval_secs` (default 2), `auto_failback` (default `false`), `auto_failback_stable_secs` (default 30), `connect_timeout_ms` (default 800), `lsof_recheck` (default `true`)
- Support `[[shares]]` array with per-share: `name`, `username`, `thunderbolt_host`, `fallback_host`, `share_name`
- Support `[[aliases]]` array with per-alias: `name`, `path`, `share`, `target_subpath`
- Expand `~/` to the user's home directory in all path fields
- Persist runtime state to `~/.mountaineer/state.json`
- Runtime state tracks per-share: `active_interface` (tb|fallback), `tb_reachable` (bool), `tb_reachable_since` (timestamp), `fb_reachable` (bool), `mount_alive` (bool), `tb_recovery_pending` (bool), `last_switch_at` (timestamp), `last_error` (optional string)
- Runtime state also tracks `tb_healthy_since` (timestamp) per share — when TB was first confirmed both reachable AND mounted `[observed from code]`
- Support config hot-reload: detect changes to `config.toml` and apply without restart
- Save runtime state after every state-changing operation
- Validate config on load: reject missing required fields, duplicate share names, invalid hosts
- Use `shares_root` config field as the root for all stable symlink paths and alias paths — do not hardcode `~/Shares/`

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

## References
- `.planning/reqs-001.md` — Config Model (TOML), State Model

## Notes
- **Config path mismatch** `[observed from code]`: Code loads config from `~/.config/mountaineer/config.toml` and state from `~/.config/mountaineer/state.json`. Code must be updated to use `~/.mountaineer/`.
- **`mount_root` field must be removed** `[observed from code]`: `GlobalConfig` in `config.rs` still has a `mount_root` field (default `~/.mountaineer/mnts`) used by `backend_mount_path()` for dual-mount mode. This field must be removed along with all dual-mount code.
- **`single_mount_mode` toggle must be removed** `[observed from code]`: `GlobalConfig` has `single_mount_mode: bool` (default true). Single-mount is the only architecture, not a toggle. This field must be removed.
- **`auto_failback` default** `[observed from code]`: Code defaults `auto_failback` to `false`. This is correct per spec. The reqs config example shows `true` but the reqs example will be updated to match.
- **`lsof_recheck` not in code** `[observed from code]`: The `GlobalConfig` struct does not currently include a `lsof_recheck` field. Must be added with default `true`.
- **Config validation** `[observed from code]`: Code does not currently validate for duplicate share names or invalid hosts on load. Validation must be implemented.
