use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// MountError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum MountError {
    /// The mount point directory could not be created.
    CreateMountPoint { path: PathBuf, source: std::io::Error },
    /// The mount_smbfs command failed.
    MountFailed { stderr: String, exit_code: Option<i32> },
    /// Both diskutil and umount failed.
    UnmountFailed { stderr: String },
    /// The command binary could not be spawned.
    CommandSpawn { command: String, source: std::io::Error },
}

impl fmt::Display for MountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MountError::CreateMountPoint { path, source } => {
                write!(f, "failed to create mount point {}: {}", path.display(), source)
            }
            MountError::MountFailed { stderr, exit_code } => {
                let code = exit_code.map_or("?".to_string(), |c| c.to_string());
                write!(f, "mount_smbfs failed (exit {}): {}", code, stderr)
            }
            MountError::UnmountFailed { stderr } => {
                write!(f, "unmount failed: {}", stderr)
            }
            MountError::CommandSpawn { command, source } => {
                write!(f, "failed to spawn {}: {}", command, source)
            }
        }
    }
}

impl std::error::Error for MountError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MountError::CreateMountPoint { source, .. }
            | MountError::CommandSpawn { source, .. } => Some(source),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// mount_favorite — Keychain-based mount (no explicit credentials)
// ---------------------------------------------------------------------------

/// Mount a favorite share using macOS Keychain for authentication.
///
/// For /Volumes/ mount points, uses `osascript -e 'mount volume "smb://..."'`
/// which mounts silently via Finder (no window, no progress bar, no error dialog).
/// For other mount points, uses `mount_smbfs //server/share /mount/point`.
pub fn mount_favorite(fav: &crate::config::Favorite) -> Result<(), MountError> {
    let mount_point = Path::new(&fav.mount_point);

    // /Volumes/ is SIP-protected — use AppleScript `mount volume` for silent mount
    if fav.mount_point.starts_with("/Volumes/") {
        let script = format!("mount volume \"smb://{}/{}\"", fav.server, fav.share);
        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| MountError::CommandSpawn {
                command: "osascript".into(),
                source: e,
            })?;

        if output.status.success() {
            log::info!(
                "Mounted //{}/{} via osascript mount volume (silent)",
                fav.server,
                fav.share,
            );
            // osascript mount volume is synchronous — no sleep needed
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            log::error!("osascript mount volume failed: {}", stderr);
            Err(MountError::MountFailed {
                stderr,
                exit_code: output.status.code(),
            })
        }
    } else {
        // Non-/Volumes/ mount point — use mount_smbfs directly
        if !mount_point.exists() {
            std::fs::create_dir_all(mount_point).map_err(|e| MountError::CreateMountPoint {
                path: mount_point.to_path_buf(),
                source: e,
            })?;
        }

        let smb_url = format!("//{}/{}", fav.server, fav.share);
        let output = Command::new("mount_smbfs")
            .arg(&smb_url)
            .arg(mount_point)
            .output()
            .map_err(|e| MountError::CommandSpawn {
                command: "mount_smbfs".into(),
                source: e,
            })?;

        if output.status.success() {
            log::info!(
                "Mounted //{}/{} at {}",
                fav.server,
                fav.share,
                mount_point.display(),
            );
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            log::error!("mount_smbfs failed: {}", stderr);
            Err(MountError::MountFailed {
                stderr,
                exit_code: output.status.code(),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// is_mount_alive — stale mount detection
// ---------------------------------------------------------------------------

/// Check if a mount point is actually alive (not stale).
/// Spawns a thread to call `std::fs::metadata` with a 2-second timeout.
/// Returns `false` if the metadata call hangs (stale mount) or errors.
pub fn is_mount_alive(mount_point: &std::path::Path) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let path = mount_point.to_path_buf();

    std::thread::spawn(move || {
        let _ = tx.send(std::fs::metadata(&path).is_ok());
    });

    rx.recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// is_mounted
// ---------------------------------------------------------------------------

/// Check if a path is currently an active SMB mount point.
#[allow(dead_code)]
pub fn is_mounted(mount_point: &Path) -> bool {
    let output = match Command::new("mount").arg("-t").arg("smbfs").output() {
        Ok(out) => out,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let target = mount_point.to_string_lossy();
    stdout.lines().any(|line| line.contains(&*target))
}

// ---------------------------------------------------------------------------
// unmount
// ---------------------------------------------------------------------------

/// Unmount a mounted share.
///
/// Tries `diskutil unmount force` first. If that fails, falls back to `umount -f`.
pub fn unmount(mount_point: &Path) -> Result<(), MountError> {
    // Primary: diskutil unmount force
    let output = Command::new("diskutil")
        .args(["unmount", "force"])
        .arg(mount_point)
        .output()
        .map_err(|e| MountError::CommandSpawn {
            command: "diskutil".into(),
            source: e,
        })?;

    if output.status.success() {
        log::info!("Unmounted {} via diskutil", mount_point.display());
        return Ok(());
    }

    let diskutil_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    log::warn!(
        "diskutil unmount failed for {}: {} — trying umount -f",
        mount_point.display(),
        diskutil_err,
    );

    // Fallback: umount -f
    let output = Command::new("umount")
        .arg("-f")
        .arg(mount_point)
        .output()
        .map_err(|e| MountError::CommandSpawn {
            command: "umount".into(),
            source: e,
        })?;

    if output.status.success() {
        log::info!("Unmounted {} via umount -f", mount_point.display());
        Ok(())
    } else {
        let umount_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        log::error!("Both unmount methods failed for {}", mount_point.display());
        Err(MountError::UnmountFailed {
            stderr: format!("diskutil: {}; umount: {}", diskutil_err, umount_err),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_error_display() {
        let err = MountError::MountFailed {
            stderr: "permission denied".into(),
            exit_code: Some(1),
        };
        assert_eq!(err.to_string(), "mount_smbfs failed (exit 1): permission denied");

        let err = MountError::UnmountFailed {
            stderr: "not mounted".into(),
        };
        assert_eq!(err.to_string(), "unmount failed: not mounted");
    }

    #[test]
    fn mount_error_display_unknown_exit() {
        let err = MountError::MountFailed {
            stderr: "signal".into(),
            exit_code: None,
        };
        assert_eq!(err.to_string(), "mount_smbfs failed (exit ?): signal");
    }
}
