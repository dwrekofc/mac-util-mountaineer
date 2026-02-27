# Design Principles

## Purpose
Cross-cutting architecture invariants, conventions, and constraints that govern every component of Mountaineer V2. This spec is the source of truth for decisions that apply system-wide.

## Requirements
- Enforce single-mount architecture: only ONE interface (Thunderbolt or Fallback) mounts a given share at any time
- Mount paths are always `/Volumes/<SHARE>`, managed by macOS — Mountaineer never creates its own mount point directories
- Stable user paths follow the pattern `~/Shares/<SHARE> → /Volumes/<SHARE>`
- All Mountaineer-managed files live under `~/.mountaineer/` (config, state, any runtime data)
- Log to `~/Library/Logs/mountaineer.log` following macOS conventions
- Engine and CLI are implemented in Rust (edition 2024)
- Menu bar UI uses native Swift or a lightweight macOS-native framework — NOT GPUI (GPUI is too large a dependency for a menu-bar-only app)
- UI is optional; CLI remains fully functional and independently supported
- All UI actions call the same engine functions as CLI — no separate code paths
- Support multiple shares from config (e.g., `CORE`, `VAULT-R1`)
- Per-share interface preference order: Thunderbolt first, Fallback second
- Never force-mount both interfaces simultaneously for the same share (dual-mount explicitly rejected)

## Constraints
- No true migration of open file descriptors between interfaces
- No protocol-level SMB multichannel forcing
- No kernel filesystem extension work
- No interactive shell or REPL
- macOS notification center integration deferred to future roadmap
- Credentials come from Keychain or existing SMB auth context — Mountaineer does not store passwords

## Acceptance Criteria
1. No code path exists that mounts the same share via two interfaces simultaneously
2. All mount operations target `/Volumes/<SHARE>` (no `~/.mountaineer/mnts/` directories)
3. `~/.mountaineer/` directory contains `config.toml` and `state.json`
4. CLI works end-to-end without the menu bar UI running
5. Every UI action in Phase 2 calls the same Rust engine function as the corresponding CLI command

## References
- `.planning/reqs-001.md` — Core Design §1 (Single-Mount Architecture), Non-Goals
- `.planning/decisions-001.md` — Single-Mount Architecture decision, Phase 2 UI decision

## Notes
- **Config path mismatch** `[observed from code]`: Code uses `~/.config/mountaineer/` for config.toml and state.json. Code must be updated to use `~/.mountaineer/`.
- **UI framework mismatch** `[observed from code]`: Code uses GPUI (from Zed) for the menu bar UI (`gui.rs`, `tray.rs`). GPUI will be removed and replaced with native Swift or lightweight macOS-native framework.
- **Dual-mount code must be removed** `[observed from code]`: `engine.rs` retains `choose_desired_backend()` for dual-mount mode, controlled by a `single_mount_mode` config toggle (default true). This code must be removed — single-mount is the only architecture, not a toggle.
