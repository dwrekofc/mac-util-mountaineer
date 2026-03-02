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
- **`ShareStatus` struct** `[observed from code]`: The `ShareStatus` struct in `engine.rs` includes per-backend `BackendStatus` (host, mount_point, reachable, mounted, alive, ready, last_error), `desired_backend`, `stable_path`, `tb_reachable_since`, and `tb_healthy_since` — richer than the minimum spec requirements. All fields listed in the Requirements section are present.
- **`lsof_recheck` in status** `[resolved]`: `lsof_recheck` is in `GlobalConfig` and exposed via `StatusOutput` wrapper in both `status --json` and `verify --json` output.
- **`tb_recovery_pending` in `ShareStatus`** `[resolved]`: `tb_recovery_pending` is included in `ShareStatus` and appears in JSON output.
- **`tb_reachable_since` and `tb_healthy_since`** `[resolved]`: Both timestamps are now surfaced in `ShareStatus` from `ShareRuntimeState`, appearing in JSON output.
- **`verify` vs `status` difference** `[observed from code]`: Both `verify_all` and `verify_selected` call `reconcile_share` with `attempt_mount=false, auto_switch=false`. They are functionally identical to `status`. The distinction exists only at the CLI level (different command names).
