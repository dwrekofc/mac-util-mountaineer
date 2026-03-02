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
- **Config path** `[RESOLVED P0]`: Was: code used `~/.config/mountaineer/`. Now uses `~/.mountaineer/` as specified.
- **UI framework** `[RESOLVED P6]`: Was: code used GPUI. Now uses native macOS NSApplication via `objc` crate — lightweight, direct AppKit bindings.
- **Dual-mount code removed** `[RESOLVED P0]`: Was: `engine.rs` retained dual-mount mode with `single_mount_mode` toggle. All dual-mount code and the toggle removed. Single-mount is the only architecture.
- **V1 modules cleaned** `[RESOLVED P0/P3]`: `watcher.rs` removed (P3). `discovery.rs` pruned to retain only `is_smb_reachable`/`is_smb_reachable_with_timeout` and gated `check_share_available` (P3). `wol.rs` remains on disk but excluded from compilation (no `mod wol;`) — retained for future WoL spec.
- **`network/interface.rs` intentionally retained** `[observed from code]`: Module `network::interface` (interface enumeration via SCNetworkConfiguration) is marked `#[allow(dead_code)]`. Retained for future NIC auto-detection feature. 11 tests (4 ignored system-dependent, 7 pure unit).
