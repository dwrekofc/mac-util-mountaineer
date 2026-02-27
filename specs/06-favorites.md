# Favorites

## Purpose
Manages the lifecycle of managed drives. Favorites are the canonical list of shares that Mountaineer monitors, mounts, and maintains. Adding or removing a favorite handles all associated setup and teardown automatically.

## Requirements
- `favorites add` registers a new share with: name, thunderbolt_host, fallback_host, username, and optional remote share_name (defaults to name)
- On add: create `~/Shares/<SHARE>` symlink, write share to config, attempt immediate mount via best available interface
- `favorites remove` stops monitoring and optionally cleans up
- On remove with `--cleanup`: unmount the share, remove `~/Shares/<SHARE>` symlink, remove share from config
- On remove without `--cleanup`: remove share from config only, leave mount and symlink intact
- Report dependent aliases when removing a favorite so the user knows what else will break
- `favorites list` shows all managed shares with their TB host, fallback host, and current status
- Support `--json` output on `favorites list`
- Favorite names must be unique — reject duplicates on add
- Persist favorites to `~/.mountaineer/config.toml` `[[shares]]` section

## Constraints
- Favorites are the sole mechanism for adding shares to management — there is no auto-discovery
- A favorite's share name determines the volume path (`/Volumes/<SHARE>`) and symlink path (`~/Shares/<SHARE>`)
- Removing a favorite does NOT auto-remove dependent aliases — they are reported but left for the user

## Acceptance Criteria
1. `favorites add` creates a config entry, a `~/Shares/<SHARE>` symlink, and mounts the share
2. `favorites remove --cleanup` unmounts, removes symlink, and removes config entry
3. `favorites remove` (no cleanup) removes config entry only
4. `favorites list` shows all managed shares with connection details
5. `favorites list --json` outputs valid JSON
6. Duplicate share names are rejected with a clear error
7. Dependent aliases are listed when removing a favorite

## References
- `.planning/reqs-001.md` — JTBD 4, Core Design §4 (Favorites as Managed Drive Source)

## Notes
- **Upsert behavior must change to reject** `[observed from code]`: `add_or_update_share()` in `engine.rs` performs an upsert — if a share with the same name already exists, it updates the entry. Code must be changed to reject duplicates on `add` per spec. Users who need to change connection details should edit `config.toml` directly.
- **`--cleanup` flag not in CLI struct** `[observed from code]`: The `FavoritesCommand::Remove` variant in `cli.rs` needs verification that it includes a `--cleanup` flag. The engine's `cleanup_removed_share()` exists to perform the cleanup, but the CLI wiring must be confirmed and fixed if missing.
