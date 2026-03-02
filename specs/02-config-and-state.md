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
- Runtime state persists per-share: `active_backend` (tb|fallback|none), `tb_reachable_since` (timestamp), `tb_healthy_since` (timestamp), `tb_recovery_pending` (bool), `last_switch_at` (timestamp), `last_error` (optional string)
- `tb_healthy_since` tracks when TB was first confirmed both reachable AND successfully mounted `[observed from code]`
- `tb_reachable`, `fb_reachable`, and `mount_alive` are computed live each reconcile cycle via TCP probes and `fs::metadata` — they are NOT persisted in state.json `[observed from code]`
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
- **Config path** `[RESOLVED P0]`: Was: code used `~/.config/mountaineer/`. Now correctly uses `~/.mountaineer/` for both config.toml and state.json.
- **`mount_root` field removed** `[RESOLVED P0]`: Was: `GlobalConfig` had a `mount_root` field for dual-mount. Field and all dual-mount code removed.
- **`single_mount_mode` toggle removed** `[RESOLVED P0]`: Was: `GlobalConfig` had `single_mount_mode: bool`. Toggle removed — single-mount is the only architecture.
- **`auto_failback` default** `[observed from code]`: Code defaults `auto_failback` to `false`. This is correct per spec.
- **`lsof_recheck` implemented** `[RESOLVED P1]`: Was: `GlobalConfig` did not include `lsof_recheck`. Now present with default `true`.
- **Config validation implemented** `[RESOLVED P1]`: Was: no validation for duplicate share names. Now validates on load — rejects missing required fields, duplicate share names.
