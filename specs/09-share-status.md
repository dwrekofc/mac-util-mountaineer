# Share Status

## Purpose
Defines the per-share health and status data model that powers both CLI output and the menu bar UI. This is the common status representation consumed by all presentation layers.

## Requirements
- Provide per-share status containing: share name, active interface (tb|fallback|none), mount state (mounted|unmounted|error), mount path, TB reachable (bool), FB reachable (bool), tb_recovery_pending (bool), last_error (optional string), last_switch_at (timestamp), tb_reachable_since (timestamp), tb_healthy_since (timestamp) `[observed from code]`
- `status --all` displays health for every managed share in human-readable format
- `status --all --json` outputs the same data as valid JSON for machine parsing
- Include the "TB Ready" indicator when `tb_recovery_pending` is true — this must be prominent in both human and JSON output
- Include the active interface label (Thunderbolt or Fallback) per share
- Include `lsof_recheck` current setting (on/off) in global status
- `verify --share <name>` or `verify --all` runs health and mount checks without making changes, reports results
- Support `--json` output on `verify`
- Status data model is the single source consumed by both CLI formatting and tray menu rendering

## Constraints
- Status is read from runtime state + live probes (not cached from last reconcile)
- Status commands are read-only — they never change mount state
- The status data structure must be serializable to JSON

## Acceptance Criteria
1. `status --all` shows each share's name, active interface, mount state, and TB Ready indicator
2. `status --all --json` outputs valid JSON matching the same data
3. "TB Ready" is clearly visible when TB is available but files are open
4. `verify --all --json` runs health checks and outputs results as JSON
5. Status output includes `lsof_recheck` setting
6. The same status data structure is usable by the tray menu without transformation

## References
- `.planning/reqs-001.md` — JTBD 7, JTBD 8 (CLI status), State Model

## Notes
- **`ShareStatus` struct** `[observed from code]`: The `ShareStatus` struct in `engine.rs` includes per-backend `BackendStatus` (host, mount_point, reachable, mounted, alive, ready, last_error), `desired_backend`, and `stable_path` — richer than what the spec enumerates. The spec requirements should be considered a minimum.
- **`lsof_recheck` in status** `[observed from code]`: Since `lsof_recheck` is not yet in the config struct, it cannot appear in status output. This is a build task, not a spec gap.
- **`tb_recovery_pending` not in `ShareStatus`** `[observed from code]`: The `ShareStatus` struct does not include `tb_recovery_pending`. The tray menu reads this from `RuntimeState` directly. For CLI `status --json` output, `tb_recovery_pending` should be included in the serialized status to surface "TB Ready" in JSON output. Currently it is absent from the JSON.
- **`verify` vs `status` difference** `[observed from code]`: `verify_all` and `share_statuses` (used by `status`) both call the same `reconcile_share` with `attempt_mount=false, auto_switch=false`. They are functionally identical. The distinction exists only at the CLI level (different command names).
