use std::fs::{self, OpenOptions};
use std::io::{self, LineWriter, Write};
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
/// - CLI mode writes to both stderr and the same log file.
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
            let file = open_log_file()?;
            let writer = MultiWriter {
                stderr: io::stderr(),
                file: LineWriter::new(file),
            };
            // Keep CLI operational logs visible even if shell-level RUST_LOG is
            // restrictive (for example, RUST_LOG=warn).
            builder.filter_module("mountaineer", log::LevelFilter::Info);
            builder.target(env_logger::Target::Pipe(Box::new(writer)));
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

struct MultiWriter {
    stderr: io::Stderr,
    file: LineWriter<std::fs::File>,
}

impl Write for MultiWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write_all(buf)?;
        self.file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()?;
        self.file.flush()
    }
}
