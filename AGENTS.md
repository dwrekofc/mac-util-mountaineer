The role of this file is to describe common mistakes and confusion points that agents might encounter as they work in this project. 

If you ever encounter something in the project that surprises you, please alert the developer working with you and indicate that this is the case to help prevent future agents from having the same issue.

This project is super green field and no one is using it yet. we are focused on getting it in the right shape.

## Build & Run

- Language: Rust (edition 2024)
- UI: Native macOS (NSApplication via objc crate) + tray-icon crate
- Build: `cargo build`
- Run: `cargo run`

### Environment Requirements

When the Xcode license has not been accepted (common after Xcode updates), use:

```sh
DEVELOPER_DIR=/Library/Developer/CommandLineTools \
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk \
BINDGEN_EXTRA_CLANG_ARGS="-isysroot /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk" \
cargo build
```


## Validation

- Tests: `cargo nextest run` (fallback: `cargo test`)
- Clippy: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

## Operational Notes

### Codebase Patterns

- Root Cargo.toml is a workspace manifest (not a package). The actual binary crate is in `crates/mountaineer/`.
- Application lifecycle uses NSApplication directly via `objc` crate. Main thread runs a custom event pump; background reconcile thread signals it via AtomicBool.
