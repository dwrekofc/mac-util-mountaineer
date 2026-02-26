# Aliases

## Purpose
Provides named shortcut symlinks to frequently-accessed subfolders inside managed shares. Aliases resolve through the stable `~/Shares/<SHARE>` root, so they survive interface switches without any updates.

## Requirements
- `alias add` creates a symlink: `~/Shares/Links/<ALIAS_NAME> → ~/Shares/<SHARE>/<TARGET_SUBPATH>`
- Support custom alias path via `--alias-path` flag (overrides default `~/Shares/Links/<ALIAS_NAME>` location)
- `alias list` shows all aliases with their target paths and health status (valid/broken)
- Support `--json` output on `alias list`
- `alias remove` deletes the symlink and removes the alias from config
- `alias reconcile` validates all alias symlinks and repairs any that are missing or broken
- Alias targets always resolve through `~/Shares/<SHARE>/...`, never directly to `/Volumes/...`
- Aliases survive interface switches because the underlying `~/Shares/<SHARE>` symlink and `/Volumes/<SHARE>` path are stable
- Alias definitions are persisted in `~/.mountaineer/config.toml` `[[aliases]]` section
- Create the `~/Shares/Links/` directory if it does not exist (when using default alias paths)
- Validate that the referenced share exists in favorites on alias add — reject if share not found

## Constraints
- Aliases are symlinks, not copies or bind mounts
- Alias names must be unique across all shares
- Alias target subpath is relative to the share root — not an absolute path
- Alias validation happens during `alias reconcile` and during the regular reconcile cycle

## Acceptance Criteria
1. `alias add --name projects --share CORE --target-subpath dev/projects` creates `~/Shares/Links/projects → ~/Shares/CORE/dev/projects`
2. `alias list` shows alias name, target, and whether the symlink is valid
3. `alias list --json` outputs valid JSON
4. `alias remove --name projects` deletes the symlink and config entry
5. `alias reconcile` recreates missing or broken alias symlinks
6. Aliases continue to resolve after an interface switch (TB → FB or FB → TB)
7. Adding an alias for a non-existent share produces a clear error

## References
- `.planning/reqs-001.md` — JTBD 5, Core Design §3 (Managed Subfolder Aliases)

## Notes
- **Atomic symlink creation** `[observed from code]`: `reconcile_alias()` in `engine.rs` uses an atomic write-to-temp-then-rename pattern, consistent with stable path symlink management.
