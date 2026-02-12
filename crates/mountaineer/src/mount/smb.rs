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
/// Uses `mount_smbfs //server/share /mount/point` without `-N` flag,
/// which tells macOS to look up credentials from the Keychain automatically.
pub fn mount_favorite(fav: &crate::config::Favorite) -> Result<(), MountError> {
    let mount_point = Path::new(&fav.mount_point);

    // Ensure mount point directory exists
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
