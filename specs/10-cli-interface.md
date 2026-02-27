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
- **Missing `--force` flags** `[observed from code]`: The `Switch` command struct has no `--force` field. The `Unmount` command struct has no `--force` field. Both are specified in the reqs and this spec but not yet wired in `cli.rs`.
- **Missing `config set` command** `[observed from code]`: No `Config` variant exists in the `Command` enum. The `config set lsof-recheck on|off` command is not implemented.
- **`mount --all` == `reconcile --all`** `[observed from code]`: `cmd_mount` delegates to `engine::reconcile_all`, making it functionally identical to reconcile. This is by design but worth noting — mount also performs failover/recovery logic.
- **`status` requires `--all`** `[observed from code]`: The CLI rejects `status` without `--all`. Per-share `status --share <name>` is not supported — only `--all` mode. This differs from `verify` which supports both `--share` and `--all`.
- **`mount-backends` command must be removed** `[observed from code]`: Code includes a `MountBackends` CLI command not in the spec. It calls `engine::mount_backends_for_shares` and is a dual-mount artifact. Must be removed along with all dual-mount code.
- **`switch` uses old dual-mount path** `[observed from code]`: `cmd_switch` in `main.rs` calls `engine::switch_share` which uses `backend_mount_path` (dual-mount style) and `set_symlink_atomically` to point the stable path at the backend mount directory. In single-mount mode, it should call `switch_backend_single_mount` instead, which does the proper unmount-then-remount sequence at `/Volumes/<SHARE>`.
- **`monitor` does not consume network events** `[observed from code]`: `cmd_monitor` uses a fixed `thread::sleep` loop and does not consume the SCDynamicStore network change events from `network::monitor`. The `watcher.rs` V1 code does consume them. This must be wired into the V2 monitor loop.
