use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub fn is_smb_reachable_with_timeout(server: &str, timeout: Duration) -> bool {
    let addr = format!("{}:445", server);
    let addrs: Vec<_> = match addr.to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return false,
    };
    for sock_addr in addrs {
        if TcpStream::connect_timeout(&sock_addr, timeout).is_ok() {
            return true;
        }
    }
    false
}

// check_share_available and its supporting types are candidates for future
// probe enhancement (smbutil view preflight). Gated until wired into engine.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareCheckResult {
    Available,
    NotFound,
    Unknown { reason: String },
}

/// Check whether a specific share is available on a server by enumerating shares.
///
/// Uses `smbutil view //server` which lists shares without mounting.
/// Returns:
/// - [`ShareCheckResult::Available`] when share was listed
/// - [`ShareCheckResult::NotFound`] when enumeration succeeded but share is absent
/// - [`ShareCheckResult::Unknown`] for timeout/spawn/command failures
#[allow(dead_code)]
pub fn check_share_available(server: &str, share: &str, timeout: Duration) -> ShareCheckResult {
    let output = match run_smbutil_view(server, timeout) {
        Ok(o) => o,
        Err(reason) => {
            log::debug!(
                "smbutil view preflight unavailable for {}: {}",
                server,
                reason
            );
            return ShareCheckResult::Unknown { reason };
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let reason = format!(
            "smbutil view exited with {:?}: {}",
            output.status.code(),
            stderr
        );
        log::debug!(
            "smbutil view preflight unavailable for {}: {}",
            server,
            reason
        );
        return ShareCheckResult::Unknown { reason };
    }

    if parse_smbutil_view_contains_share(&output.stdout, share) {
        ShareCheckResult::Available
    } else {
        ShareCheckResult::NotFound
    }
}

#[allow(dead_code)]
struct CommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[allow(dead_code)]
fn run_smbutil_view(server: &str, timeout: Duration) -> Result<CommandOutput, String> {
    let mut child = Command::new("smbutil")
        .args(["view", &format!("//{}", server)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn smbutil view: {}", e))?;

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "smbutil view timed out after {}ms",
                        timeout.as_millis()
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed while waiting for smbutil view: {}", e));
            }
        }
    };

    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout);
    }
    let mut stderr = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut stderr);
    }

    Ok(CommandOutput {
        status,
        stdout,
        stderr,
    })
}

#[allow(dead_code)]
fn parse_smbutil_view_contains_share(stdout: &[u8], share: &str) -> bool {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("Share") || trimmed.starts_with("-----") {
                return None;
            }
            trimmed.split_whitespace().next()
        })
        .any(|name| name.eq_ignore_ascii_case(share))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_smbutil_view_contains_expected_share() {
        let sample = br#"
Share                         Type    Comments
-----                         ----    --------
CORE-01                       Disk
VAULT-R1                      Disk
"#;
        assert!(parse_smbutil_view_contains_share(sample, "CORE-01"));
        assert!(parse_smbutil_view_contains_share(sample, "vault-r1"));
    }

    #[test]
    fn parse_smbutil_view_reports_missing_share() {
        let sample = br#"
Share                         Type    Comments
-----                         ----    --------
CORE-01                       Disk
"#;
        assert!(!parse_smbutil_view_contains_share(sample, "VAULT-R1"));
    }
}
