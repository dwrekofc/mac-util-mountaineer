use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum MountError {
    CreateMountPoint {
        path: PathBuf,
        source: std::io::Error,
    },
    MountFailed {
        stderr: String,
        exit_code: Option<i32>,
    },
    UnmountFailed {
        stderr: String,
    },
    CommandSpawn {
        command: String,
        source: std::io::Error,
    },
}

impl fmt::Display for MountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MountError::CreateMountPoint { path, source } => {
                write!(
                    f,
                    "failed to create mount point {}: {}",
                    path.display(),
                    source
                )
            }
            MountError::MountFailed { stderr, exit_code } => {
                let code = exit_code.map_or_else(|| "?".to_string(), |code| code.to_string());
                write!(f, "mount failed (exit {}): {}", code, stderr)
            }
            MountError::UnmountFailed { stderr } => write!(f, "unmount failed: {}", stderr),
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

pub fn mount_share(
    host: &str,
    share: &str,
    username: &str,
    mount_point: &Path,
) -> Result<(), MountError> {
    if let Some(existing_mount) = find_existing_mount_for_share(host, share) {
        adopt_existing_mount(mount_point, &existing_mount)?;
        return Ok(());
    }

    // Prefer Finder-backed AppleScript mount for less disruptive UX.
    // If it fails or doesn't yield a detectable mount entry, fall back to mount_smbfs.
    let osascript_error = match try_osascript_mount(host, share, username) {
        Ok(()) => {
            if let Some(existing_mount) =
                wait_for_existing_mount_for_share(host, share, Duration::from_secs(2))
            {
                adopt_existing_mount(mount_point, &existing_mount)?;
                return Ok(());
            }
            Some("osascript mount returned success but no detectable share path".to_string())
        }
        Err(err) => Some(format!("osascript mount failed: {}", err)),
    };

    ensure_mount_point_dir(mount_point)?;

    let url = build_smb_url(host, share, username);
    let output = Command::new("mount_smbfs")
        .arg(&url)
        .arg(mount_point)
        .output()
        .map_err(|source| MountError::CommandSpawn {
            command: "mount_smbfs".to_string(),
            source,
        })?;

    if output.status.success() {
        return Ok(());
    }

    let original_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let original_exit = output.status.code();

    // If Finder mounted the share elsewhere, adopt that mount path.
    if let Some(existing_mount) = find_existing_mount_for_share(host, share) {
        adopt_existing_mount(mount_point, &existing_mount)?;
        return Ok(());
    }

    let mut combined = osascript_error.unwrap_or_else(|| "osascript mount failed".to_string());
    combined.push_str("; mount_smbfs fallback failed: ");
    combined.push_str(&original_stderr);
    Err(MountError::MountFailed {
        stderr: combined,
        exit_code: original_exit,
    })
}

fn build_smb_url(host: &str, share: &str, username: &str) -> String {
    if username.trim().is_empty() {
        format!("//{}/{}", host, share)
    } else {
        format!("//{}@{}/{}", username, host, share)
    }
}

pub fn is_mount_alive(mount_point: &Path) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let path = mount_point.to_path_buf();

    std::thread::spawn(move || {
        let _ = tx.send(std::fs::metadata(&path).is_ok());
    });

    rx.recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

pub fn is_mounted(mount_point: &Path) -> bool {
    let output = match Command::new("mount").args(["-t", "smbfs"]).output() {
        Ok(output) => output,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let adopted_target = resolve_symlink_target(mount_point);

    stdout.lines().any(|line| {
        let Some((_host, _share, current_mount)) = parse_mount_smb_line(line) else {
            return false;
        };

        paths_match(&current_mount, mount_point)
            || adopted_target
                .as_ref()
                .is_some_and(|target| paths_match(&current_mount, target))
    })
}

pub fn unmount(mount_point: &Path) -> Result<(), MountError> {
    unmount_impl(mount_point, true)
}

pub fn unmount_graceful(mount_point: &Path) -> Result<(), MountError> {
    unmount_impl(mount_point, false)
}

fn unmount_impl(mount_point: &Path, force: bool) -> Result<(), MountError> {
    let unmount_target = resolve_symlink_target(mount_point).unwrap_or_else(|| mount_point.into());

    let diskutil = if force {
        Command::new("diskutil")
            .args(["unmount", "force"])
            .arg(&unmount_target)
            .output()
    } else {
        Command::new("diskutil")
            .arg("unmount")
            .arg(&unmount_target)
            .output()
    }
    .map_err(|source| MountError::CommandSpawn {
        command: "diskutil".to_string(),
        source,
    })?;

    if diskutil.status.success() {
        return Ok(());
    }

    let diskutil_err = String::from_utf8_lossy(&diskutil.stderr).trim().to_string();
    let umount = if force {
        Command::new("umount")
            .arg("-f")
            .arg(&unmount_target)
            .output()
    } else {
        Command::new("umount").arg(&unmount_target).output()
    }
    .map_err(|source| MountError::CommandSpawn {
        command: "umount".to_string(),
        source,
    })?;

    if umount.status.success() {
        Ok(())
    } else {
        let umount_err = String::from_utf8_lossy(&umount.stderr).trim().to_string();
        let mode = if force { "force" } else { "graceful" };
        Err(MountError::UnmountFailed {
            stderr: format!(
                "{} unmount failed; diskutil: {}; umount: {}",
                mode, diskutil_err, umount_err
            ),
        })
    }
}

fn ensure_mount_point_dir(mount_point: &Path) -> Result<(), MountError> {
    if fs::symlink_metadata(mount_point).is_ok() {
        return Ok(());
    }

    fs::create_dir_all(mount_point).map_err(|source| MountError::CreateMountPoint {
        path: mount_point.to_path_buf(),
        source,
    })
}

fn try_osascript_mount(host: &str, share: &str, username: &str) -> Result<(), String> {
    let smb_url = if username.trim().is_empty() {
        format!("smb://{}/{}", host, share)
    } else {
        format!("smb://{}@{}/{}", username, host, share)
    };
    let script = format!(
        r#"tell application "Finder"
mount volume "{}"
end tell"#,
        smb_url
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|err| format!("failed to run osascript: {}", err))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn wait_for_existing_mount_for_share(
    host: &str,
    share: &str,
    timeout: Duration,
) -> Option<PathBuf> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(path) = find_existing_mount_for_share(host, share) {
            return Some(path);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn adopt_existing_mount(mount_point: &Path, existing_mount: &Path) -> Result<(), MountError> {
    if paths_match(mount_point, existing_mount) {
        return Ok(());
    }

    if let Some(parent) = mount_point.parent() {
        fs::create_dir_all(parent).map_err(|source| MountError::CreateMountPoint {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    if let Ok(meta) = fs::symlink_metadata(mount_point) {
        if meta.file_type().is_symlink() {
            if let Some(current_target) = resolve_symlink_target(mount_point)
                && paths_match(&current_target, existing_mount)
            {
                return Ok(());
            }
            fs::remove_file(mount_point).map_err(|err| MountError::MountFailed {
                stderr: format!(
                    "failed clearing stale mountpoint symlink {}: {}",
                    mount_point.display(),
                    err
                ),
                exit_code: None,
            })?;
        } else if meta.file_type().is_dir() {
            if paths_match(mount_point, existing_mount) {
                return Ok(());
            }
            fs::remove_dir(mount_point).map_err(|err| MountError::MountFailed {
                stderr: format!(
                    "failed clearing mountpoint directory {} before adopt: {}",
                    mount_point.display(),
                    err
                ),
                exit_code: None,
            })?;
        } else {
            fs::remove_file(mount_point).map_err(|err| MountError::MountFailed {
                stderr: format!(
                    "failed clearing mountpoint file {} before adopt: {}",
                    mount_point.display(),
                    err
                ),
                exit_code: None,
            })?;
        }
    }

    std::os::unix::fs::symlink(existing_mount, mount_point).map_err(|err| {
        MountError::MountFailed {
            stderr: format!(
                "failed adopting existing mount {} -> {}: {}",
                mount_point.display(),
                existing_mount.display(),
                err
            ),
            exit_code: None,
        }
    })?;

    Ok(())
}

fn find_existing_mount_for_share(host: &str, share: &str) -> Option<PathBuf> {
    let output = Command::new("mount").args(["-t", "smbfs"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_mount_smb_line)
        .find(|(current_host, current_share, _)| {
            current_host.eq_ignore_ascii_case(host) && current_share.eq_ignore_ascii_case(share)
        })
        .map(|(_, _, mount_path)| mount_path)
}

fn parse_mount_smb_line(line: &str) -> Option<(String, String, PathBuf)> {
    let (left, right) = line.split_once(" on ")?;
    let (mount_path, _flags) = right.split_once(" (")?;

    let smb = left.strip_prefix("//")?;
    let smb = smb.split_once('@').map_or(smb, |(_, rest)| rest);
    let (host, share) = smb.split_once('/')?;

    Some((
        host.to_string(),
        share.to_string(),
        PathBuf::from(mount_path),
    ))
}

fn resolve_symlink_target(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_link(path).ok()?;
    if raw.is_absolute() {
        return Some(raw);
    }
    path.parent().map(|parent| parent.join(raw))
}

fn paths_match(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_with_user() {
        assert_eq!(
            build_smb_url("server.local", "CORE", "user"),
            "//user@server.local/CORE"
        );
    }

    #[test]
    fn build_url_without_user() {
        assert_eq!(build_smb_url("10.10.10.1", "CORE", ""), "//10.10.10.1/CORE");
    }

    #[test]
    fn parse_mount_smb_line_with_user_prefix() {
        let line = "//user@10.10.10.1/CORE on /Volumes/CORE (smbfs, nodev)";
        let (host, share, mount_path) = parse_mount_smb_line(line).unwrap();
        assert_eq!(host, "10.10.10.1");
        assert_eq!(share, "CORE");
        assert_eq!(mount_path, PathBuf::from("/Volumes/CORE"));
    }

    #[test]
    fn parse_mount_smb_line_without_user_prefix() {
        let line = "//macmini.local/VAULT-R1 on /tmp/vault (smbfs, nodev)";
        let (host, share, mount_path) = parse_mount_smb_line(line).unwrap();
        assert_eq!(host, "macmini.local");
        assert_eq!(share, "VAULT-R1");
        assert_eq!(mount_path, PathBuf::from("/tmp/vault"));
    }
}
