use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
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
    fs::write(&plist, &content)
        .with_context(|| format!("Failed to write plist to {:?}", plist))?;

    // Load the agent
    let status = Command::new("launchctl")
        .args(["load", plist.to_str().unwrap()])
        .status()
        .context("Failed to run launchctl load")?;

    if !status.success() {
        anyhow::bail!("launchctl load exited with status {}", status);
    }

    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist = plist_path()?;

    if !plist.exists() {
        anyhow::bail!("LaunchAgent is not installed (no plist found)");
    }

    // Unload the agent (ignore errors — it may not be loaded)
    let _ = Command::new("launchctl")
        .args(["unload", plist.to_str().unwrap()])
        .status();

    // Remove the plist file
    fs::remove_file(&plist).with_context(|| format!("Failed to remove {:?}", plist))?;

    Ok(())
}

pub fn is_installed() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}
