use std::fs::{self, OpenOptions};
use std::io::{self, LineWriter};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum LoggingMode {
    Gui,
    Cli,
}

/// Initialize application logging.
///
/// - GUI mode writes directly to ~/Library/Logs/mountaineer.log so logs work
///   even when the app is launched manually outside launchd.  The file handle
///   is wrapped in `LineWriter` so every log line is flushed immediately.
/// - CLI mode logs to stderr.
pub fn init(mode: LoggingMode) -> anyhow::Result<()> {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));

    match mode {
        LoggingMode::Gui => {
            let file = open_log_file()?;
            let writer = LineWriter::new(file);
            builder.target(env_logger::Target::Pipe(Box::new(writer)));
        }
        LoggingMode::Cli => {
            builder.target(env_logger::Target::Stderr);
        }
    }

    builder
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to initialize logger: {}", e))
}

fn open_log_file() -> anyhow::Result<std::fs::File> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| anyhow::anyhow!("failed to open log file {}: {}", path.display(), e))
}

fn log_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine home directory",
        )
    })?;
    Ok(home.join("Library/Logs/mountaineer.log"))
}
