# CLI Interface

## Purpose
Provides a complete, scriptable command-line surface for all Mountaineer operations. Every feature is accessible via single-shot CLI commands with machine-parseable output, enabling both human use and automation/scripting.

## Requirements
- Implement the following commands:
  - `mountaineer reconcile --all` — single reconciliation pass for all shares
  - `mountaineer monitor --interval <secs>` — continuous reconcile loop (the only non-single-shot command)
  - `mountaineer status --all [--json]` — health and state for all shares
  - `mountaineer switch --share <name> --to tb|fallback [--force]` — manual interface switch
  - `mountaineer verify --share <name>|--all [--json]` — health checks without changes
  - `mountaineer mount --all` — mount all favorited shares
  - `mountaineer unmount --all [--force]` — unmount all managed shares
  - `mountaineer folders --share <name> [--subpath <dir>] [--json]` — list folders in a share
  - `mountaineer alias add --name <alias> --share <name> --target-subpath <path> [--alias-path <path>]`
  - `mountaineer alias list [--json]`
  - `mountaineer alias remove --name <alias>`
  - `mountaineer alias reconcile [--all]`
  - `mountaineer favorites add --share <name> --tb-host <ip> --fallback-host <host> --username <user> [--remote-share <name>]`
  - `mountaineer favorites remove --share <name> [--cleanup]`
  - `mountaineer favorites list [--json]`
  - `mountaineer install` — install LaunchAgent
  - `mountaineer uninstall` — remove LaunchAgent
  - `mountaineer config set lsof-recheck on|off` — toggle lsof re-check setting
- All commands except `monitor` are single-shot and exit after completion
- Commands with `--json` flag output valid JSON to stdout
- Human-readable output goes to stdout; logs and errors go to stderr
- Exit codes: 0 for success, non-zero for errors
- Commands are deterministic and automation-friendly for AI testing

## Constraints
- No interactive shell or REPL
- CLI is built with `clap` for argument parsing
- CLI calls the same engine functions that the menu bar UI will call
- `monitor` is the only long-running command

## Acceptance Criteria
1. Every command listed above is implemented and callable
2. `--json` output is valid JSON parseable by `jq`
3. Exit code is 0 on success, non-zero on failure
4. `monitor` runs continuously until interrupted (Ctrl-C)
5. `config set lsof-recheck on|off` updates config and takes effect on next reconcile
6. All commands work without the menu bar UI running

## References
- `.planning/reqs-001.md` — JTBD 8, Phase 1 CLI Commands

## Notes
- **`--force` flags implemented** `[RESOLVED P1]`: Was: `Switch` and `Unmount` had no `--force` fields. Now both wired in `cli.rs`.
- **`config set` command implemented** `[RESOLVED P1]`: Was: no `Config` variant in `Command` enum. Now `config set lsof-recheck on|off` and `config show` are implemented.
- **`mount --all` == `reconcile --all`** `[observed from code]`: `cmd_mount` delegates to `engine::reconcile_all`, making it functionally identical to reconcile. This is by design — mount also performs failover/recovery logic.
- **`status` requires `--all`** `[observed from code]`: The CLI rejects `status` without `--all`. Per-share `status --share <name>` is not supported — only `--all` mode. This differs from `verify` which supports both `--share` and `--all`.
- **`mount-backends` command removed** `[RESOLVED P0]`: Was: dual-mount artifact `MountBackends` CLI command. Removed along with all dual-mount code.
- **`switch` uses single-mount path** `[RESOLVED P0]`: Was: `cmd_switch` used dual-mount `backend_mount_path`. Now calls `switch_backend_single_mount` for proper unmount-then-remount at `/Volumes/<SHARE>`.
- **`monitor` consumes network events** `[RESOLVED P2]`: Was: `cmd_monitor` used fixed `thread::sleep` loop without network events. Now wired to SCDynamicStore network change events with 500ms debounce.
