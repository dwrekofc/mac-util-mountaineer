# Logging and Diagnostics

## Purpose
Provides detailed, structured logging of all state transitions, mount operations, and errors so that issues can be diagnosed after the fact. Supports dual-mode output for both background daemon and interactive CLI use.

## Requirements
- Log to `~/Library/Logs/mountaineer.log` in all modes
- In CLI mode: additionally write logs to stderr for immediate visibility
- In GUI/daemon mode: write only to log file (no terminal output)
- Use immediate-flush writes (LineWriter) so logs are visible in real-time
- Log level defaults to `info`, overridable via `RUST_LOG` environment variable
- Log all state transitions:
  - Interface availability changes (TB up/down, FB up/down)
  - Mount attempts with success/failure and target path
  - Open-file check results (file count, defer/proceed decision)
  - Interface switch events (failover, recovery, manual) with from/to
  - Deferred recovery due to open files
  - "TB Ready" state transitions (entered/cleared)
  - Config reload events
- Track `last_error` per share in runtime state — most recent error message for each share
- Include `last_error` in status output (CLI and UI)
- Errors include enough context to diagnose: share name, interface, host, error message

## Constraints
- Log file path is fixed at `~/Library/Logs/mountaineer.log`
- Log rotation is left to macOS (newsyslog or ASL) — Mountaineer does not rotate its own logs
- `last_error` is stored in state.json, not only in the log file

## Acceptance Criteria
1. All state transitions listed above produce log entries
2. CLI commands show logs on stderr while also writing to the log file
3. GUI mode writes only to the log file
4. `last_error` per share is visible in `status` output and updated on each error
5. `RUST_LOG=debug` increases log verbosity
6. Log file is written with immediate flush (no buffering delays)
