The role of this file is to describe common mistakes and confusion points that agents might encounter as they work in this project. 

If you ever encounter something in the project that surprises you, please alert the developer working with you and indicate that this is the case to help prevent future agents from having the same issue.

This project is super green field and no one is using it yet. we are focused on getting it in the right shape.

## Build & Run

- Language: Rust (edition 2024)
- UI Framework: GPUI (from Zed)
- Build: `cargo build`
- Run: `cargo run`

## Validation

- Tests: `cargo nextest run` (fallback: `cargo test`)
- Clippy: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

## Operational Notes

### Codebase Patterns
