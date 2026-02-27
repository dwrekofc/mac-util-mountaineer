# Bulk Operations

## Purpose
Provides single-command mount and unmount of all managed shares for quick desk arrival/departure workflows.

## Requirements
- `mount --all` mounts every favorited share via the best available interface (TB preferred, Fallback if TB unreachable)
- Skip shares that are already mounted — do not unmount and remount
- `unmount --all` safely unmounts all managed shares with open-file checks
- Shares with open files are deferred (not unmounted) — reported to the user as "busy"
- Report per-share results: which shares were mounted/unmounted, which were skipped, which failed, which were busy
- Support `--force` on unmount to bypass open-file checks
- Each share is processed independently — one failure does not abort the entire operation
- Update runtime state for each share after its mount/unmount completes
- `unmount --all` must NOT remove `~/Shares/<SHARE>` symlinks — symlinks persist through unmount/remount cycles and are only removed via `favorites remove --cleanup` (see spec 05)

## Constraints
- Bulk operations iterate over the favorites list — only managed shares are affected
- Mount order is not guaranteed; shares are mounted independently
- Bulk unmount respects the same open-file safety as individual unmount

## Acceptance Criteria
1. `mount --all` mounts all unmounted favorited shares
2. Already-mounted shares are skipped without error
3. `unmount --all` unmounts shares with no open files and reports busy shares
4. `unmount --all --force` unmounts all shares regardless of open files
5. Per-share success/failure/busy results are reported to the user
6. Runtime state is updated for each share after the operation

## References
- `.planning/reqs-001.md` — JTBD 6

## Notes
- **`mount --all` delegates to reconcile** `[observed from code]`: `cmd_mount` in `main.rs` calls `engine::reconcile_all`, making it functionally identical to `reconcile --all`. This matches the intent (mount via best interface) but means mount also triggers failover/recovery logic, not just mount.
- **`--force` flag on unmount** `[observed from code]`: The `Unmount` CLI command struct only has an `all: bool` field — no `--force` flag is currently wired. The engine supports force unmount but the CLI must be updated to wire `--force`.
- **Symlink removal bug** `[observed from code]`: `engine::unmount_all` (line ~406-409) removes stable symlinks (`~/Shares/<SHARE>`) after unmounting via `fs::remove_file`. This is incorrect — symlinks must persist through unmount/remount cycles. Code must be fixed to preserve symlinks on unmount.
- **Dual-backend iteration in unmount** `[observed from code]`: `engine::unmount_all` iterates both `Backend::Tb` and `Backend::Fallback` per share, checking mount status at the dual-mount backend paths (`~/.mountaineer/mnts/core_tb`, `core_fb`). In single-mount mode, only one mount exists at `/Volumes/<SHARE>`. This is a dual-mount artifact — unmount should target the active mount path only.
- **State cleared to None on unmount** `[observed from code]`: `unmount_all` sets `active_backend = None` after unmounting. This is correct behavior — after unmounting, no backend is active.
