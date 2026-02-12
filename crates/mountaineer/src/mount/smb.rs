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
// SMB URL helpers
// ---------------------------------------------------------------------------

/// Percent-encode special characters in an SMB URL component (username or password).
///
/// Only unreserved characters (RFC 3986 §2.3) pass through unencoded.
/// Everything else — including `@`, `:`, `/`, `%` — is percent-encoded.
fn encode_url_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Build the SMB URL for `mount_smbfs`.
///
/// Format: `//user:pass@server/share`
/// Username and password are percent-encoded so special characters don't break parsing.
pub(crate) fn build_smb_url(username: &str, password: &str, server: &str, share: &str) -> String {
    format!(
        "//{}:{}@{}/{}",
        encode_url_component(username),
        encode_url_component(password),
        server,
        share,
    )
}

// ---------------------------------------------------------------------------
// MountParams
// ---------------------------------------------------------------------------

/// Parameters for mounting an SMB share.
pub struct MountParams<'a> {
    pub server: &'a str,
    pub share: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub mount_point: &'a Path,
}

// ---------------------------------------------------------------------------
// mount
// ---------------------------------------------------------------------------

/// Mount an SMB share using `mount_smbfs`.
///
/// Creates the mount point directory if it doesn't exist.
/// Credentials are embedded in the SMB URL and percent-encoded.
pub fn mount(params: &MountParams) -> Result<(), MountError> {
    // Ensure mount point directory exists.
    if !params.mount_point.exists() {
        std::fs::create_dir_all(params.mount_point).map_err(|e| MountError::CreateMountPoint {
            path: params.mount_point.to_path_buf(),
            source: e,
        })?;
    }

    let smb_url = build_smb_url(params.username, params.password, params.server, params.share);

    let output = Command::new("mount_smbfs")
        .arg("-N") // don't prompt for password — credentials are in URL
        .arg(&smb_url)
        .arg(params.mount_point)
        .output()
        .map_err(|e| MountError::CommandSpawn {
            command: "mount_smbfs".into(),
            source: e,
        })?;

    if output.status.success() {
        log::info!(
            "Mounted //{}@{}/{} at {}",
            params.username,
            params.server,
            params.share,
            params.mount_point.display(),
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
    fn encode_plain_ascii() {
        assert_eq!(encode_url_component("alice"), "alice");
        assert_eq!(encode_url_component("MyP4ss"), "MyP4ss");
    }

    #[test]
    fn encode_special_characters() {
        assert_eq!(encode_url_component("p@ss:word"), "p%40ss%3Aword");
        assert_eq!(encode_url_component("a/b"), "a%2Fb");
        assert_eq!(encode_url_component("100%"), "100%25");
        assert_eq!(encode_url_component("hello world"), "hello%20world");
    }

    #[test]
    fn build_url_simple() {
        let url = build_smb_url("alice", "secret", "nas.local", "shared");
        assert_eq!(url, "//alice:secret@nas.local/shared");
    }

    #[test]
    fn build_url_encodes_password() {
        let url = build_smb_url("alice", "p@ss:w0rd!", "10.0.0.5", "data");
        assert_eq!(url, "//alice:p%40ss%3Aw0rd%21@10.0.0.5/data");
    }

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
