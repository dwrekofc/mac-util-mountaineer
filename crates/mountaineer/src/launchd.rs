use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const LABEL: &str = "com.mountaineer.agent";

fn plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", LABEL)))
}

fn generate_plist(home: &str) -> String {
    let executable = format!(
        "{}/Applications/Mountaineer.app/Contents/MacOS/Mountaineer",
        home
    );
    let log_path = format!("{}/Library/Logs/mountaineer.log", home);

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        label = LABEL,
        executable = executable,
        log = log_path,
    )
}

pub fn install() -> Result<()> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let home_str = home.to_str().context("Home directory is not valid UTF-8")?;

    let plist = plist_path()?;

    // Ensure ~/Library/LaunchAgents/ exists
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    // Write plist
    let content = generate_plist(home_str);
    fs::write(&plist, &content).with_context(|| format!("Failed to write plist to {:?}", plist))?;

    let plist_str = plist
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("plist path is not valid UTF-8"))?;
    let domain = launch_domain();

    // Best effort cleanup of previously loaded job.
    let _ = run_launchctl(["bootout", domain.as_str(), plist_str]);

    // Use bootstrap for explicit error codes on modern macOS.
    let output = run_launchctl(["bootstrap", domain.as_str(), plist_str])
        .context("Failed to run launchctl bootstrap")?;
    if !output.status.success() {
        anyhow::bail!(
            "launchctl bootstrap failed (status {:?}): {}",
            output.status.code(),
            format_launchctl_output(&output)
        );
    }

    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist = plist_path()?;

    if !plist.exists() {
        log::info!("LaunchAgent plist not found — already uninstalled");
        return Ok(());
    }

    let plist_str = plist
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("plist path is not valid UTF-8"))?;
    let domain = launch_domain();
    let output = run_launchctl(["bootout", domain.as_str(), plist_str])
        .context("Failed to run launchctl bootout")?;

    if !output.status.success() {
        let msg = format_launchctl_output(&output);
        if !is_not_loaded_error(&msg) {
            anyhow::bail!(
                "launchctl bootout failed (status {:?}): {}",
                output.status.code(),
                msg
            );
        }
    }

    // Remove the plist file
    fs::remove_file(&plist).with_context(|| format!("Failed to remove {:?}", plist))?;

    Ok(())
}

pub fn is_installed() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

fn launch_domain() -> String {
    let uid = current_uid().unwrap_or(0);
    format!("gui/{}", uid)
}

fn run_launchctl<const N: usize>(args: [&str; N]) -> Result<Output> {
    let mut cmd = Command::new("launchctl");
    cmd.args(args);
    cmd.output()
        .map_err(|e| anyhow::anyhow!("failed spawning launchctl: {}", e))
}

fn format_launchctl_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() && stderr.is_empty() {
        "no output".to_string()
    } else if stdout.is_empty() {
        stderr
    } else if stderr.is_empty() {
        stdout
    } else {
        format!("stdout: {}; stderr: {}", stdout, stderr)
    }
}

fn is_not_loaded_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("could not find service")
        || lower.contains("service cannot load in requested session")
        || lower.contains("no such process")
}

fn current_uid() -> Option<u32> {
    if let Ok(uid) = std::env::var("UID")
        && let Ok(uid) = uid.parse::<u32>()
    {
        return Some(uid);
    }

    let output = Command::new("id").arg("-u").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}
