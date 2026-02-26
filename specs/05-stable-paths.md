# Stable Paths

## Purpose
Provides permanent, predictable file paths that applications, scripts, and Finder bookmarks can rely on. Because only one mount exists at a time, the volume path never changes and the symlink never needs updating.

## Requirements
- Maintain a stable symlink `~/Shares/<SHARE> → /Volumes/<SHARE>` for each managed share
- Create the `~/Shares/` root directory if it does not exist
- Create the share symlink when a share is first added to favorites
- The symlink target is always `/Volumes/<SHARE>` — it never points elsewhere
- The symlink never needs to be updated or re-pointed because the volume identity is stable under single-mount mode
- `~/Shares/` is openable in Finder as the central hub for all managed shares
- Validate symlink health during reconciliation — recreate if missing or broken
- Remove symlink only on explicit favorites removal with cleanup flag
- Create and update symlinks atomically using a write-to-temp-then-rename pattern to avoid broken intermediate states `[observed from code]`

## Constraints
- Symlinks point to `/Volumes/<SHARE>`, not directly to mount backend paths
- Symlink creation/removal is tied to the favorites lifecycle (spec 06)
- Symlinks are simple single-level links — no intermediate directories or indirection layers

## Acceptance Criteria
1. `~/Shares/<SHARE>` exists as a symlink pointing to `/Volumes/<SHARE>` for every managed share
2. `~/Shares/` directory exists and is browsable in Finder
3. The symlink resolves correctly regardless of which interface (TB or Fallback) is active
4. A missing or broken symlink is recreated during reconciliation
5. Symlink is only removed when the share is removed from favorites with `--cleanup`

## References
- `.planning/reqs-001.md` — JTBD 3, Core Design §2 (Stable User Path)
- `.planning/decisions-001.md` — Simplified Symlink decision
