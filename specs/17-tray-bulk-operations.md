# Tray: Bulk Operations

## Purpose
Provides single-click mount-all and unmount-all actions in the menu bar for quick desk arrival/departure workflows (JTBD 15).

## Requirements
- Provide a "Mount All" action in the tray menu
- "Mount All" mounts every favorited share via the best available interface
- Provide an "Unmount All" action in the tray menu
- "Unmount All" respects open-file safety: defer busy shares, unmount clear ones
- After "Unmount All", show which shares were unmounted and which couldn't be (busy)
- Provide visual feedback during the operation (in-progress state)
- Actions call the same engine bulk mount/unmount functions as the CLI

## Constraints
- Busy shares are deferred, not force-unmounted — user must close files or use CLI `--force`
- The UI does not offer a "Force Unmount All" to prevent accidental data loss from the tray

## Acceptance Criteria
1. "Mount All" button mounts all unmounted favorites
2. "Unmount All" button unmounts shares without open files
3. Busy shares are reported to the user (not silently skipped)
4. Menu updates to reflect new mount state after operation completes
5. Actions use the same engine functions as CLI `mount --all` and `unmount --all`

## References
- `.planning/reqs-001.md` — JTBD 15

## Notes
- **Not yet implemented** `[observed from code]`: The current tray menu does not include "Mount All" or "Unmount All" actions. The engine functions `reconcile_all` and `unmount_all` exist and are used by the CLI. This is a Phase 2 build task.
