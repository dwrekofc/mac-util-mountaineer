## Build & Run

- Language: Rust (edition 2024)
- UI Framework: GPUI (from Zed)
- Build: `cargo build`
- Run: `cargo run`

## Validation

- Tests: `cargo nextest run` (fallback: `cargo test`)
- Clippy: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

## Git

- Remote: always use SSH (`git@github.com:dwrekofc/...`), never HTTPS.

## Operational Notes

### Codebase Patterns
