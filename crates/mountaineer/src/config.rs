use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Tb,
    Fallback,
}

impl Backend {
    pub fn mount_suffix(self) -> &'static str {
        match self {
            Backend::Tb => "tb",
            Backend::Fallback => "fb",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Backend::Tb => "tb",
            Backend::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_shares_root")]
    pub shares_root: String,
    #[serde(default = "default_mount_root")]
    pub mount_root: String,
    #[serde(default = "default_check_interval_secs")]
    pub check_interval_secs: u64,
    #[serde(default = "default_auto_failback")]
    pub auto_failback: bool,
    #[serde(default = "default_auto_failback_stable_secs")]
    pub auto_failback_stable_secs: u64,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    /// Single-mount mode: only one backend mounted at a time (default: true).
    /// When enabled, switching backends unmounts the old one before mounting the new one.
    /// This prevents macOS from creating deduplicated volume names like /Volumes/CORE-1.
    #[serde(default = "default_single_mount_mode")]
    pub single_mount_mode: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            shares_root: default_shares_root(),
            mount_root: default_mount_root(),
            check_interval_secs: default_check_interval_secs(),
            auto_failback: default_auto_failback(),
            auto_failback_stable_secs: default_auto_failback_stable_secs(),
            connect_timeout_ms: default_connect_timeout_ms(),
            single_mount_mode: default_single_mount_mode(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareConfig {
    pub name: String,
    pub username: String,
    pub thunderbolt_host: String,
    pub fallback_host: String,
    pub share_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasConfig {
    pub name: String,
    pub path: String,
    pub share: String,
    #[serde(default)]
    pub target_subpath: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub shares: Vec<ShareConfig>,
    #[serde(default)]
    pub aliases: Vec<AliasConfig>,
}

fn default_shares_root() -> String {
    "~/Shares".to_string()
}

fn default_mount_root() -> String {
    "~/.mountaineer/mnts".to_string()
}

fn default_check_interval_secs() -> u64 {
    2
}

fn default_auto_failback() -> bool {
    false
}

fn default_auto_failback_stable_secs() -> u64 {
    30
}

fn default_connect_timeout_ms() -> u64 {
    800
}

fn default_single_mount_mode() -> bool {
    true
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".mountaineer")
        .join("config.toml")
}

pub fn state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".mountaineer")
        .join("state.json")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed reading config {}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("failed parsing TOML {}", path.display()))?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let toml = toml::to_string_pretty(config)?;
    fs::write(&path, toml).with_context(|| format!("failed writing {}", path.display()))?;
    Ok(())
}

pub fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }

    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }

    PathBuf::from(path)
}

pub fn shares_root_path(config: &Config) -> PathBuf {
    expand_path(&config.global.shares_root)
}

pub fn mount_root_path(config: &Config) -> PathBuf {
    expand_path(&config.global.mount_root)
}

pub fn share_stable_path(config: &Config, share_name: &str) -> PathBuf {
    shares_root_path(config).join(share_name)
}

pub fn backend_mount_path(config: &Config, share_name: &str, backend: Backend) -> PathBuf {
    mount_root_path(config).join(format!(
        "{}_{}",
        sanitize_share_name(share_name),
        backend.mount_suffix()
    ))
}

pub fn default_alias_path(config: &Config, alias_name: &str) -> PathBuf {
    shares_root_path(config).join("Links").join(alias_name)
}

pub fn alias_target_path(config: &Config, alias: &AliasConfig) -> PathBuf {
    let mut target = share_stable_path(config, &alias.share);
    let subpath = alias.target_subpath.trim_matches('/');
    if !subpath.is_empty() {
        target = target.join(subpath);
    }
    target
}

pub fn find_share<'a>(config: &'a Config, name: &str) -> Option<&'a ShareConfig> {
    config
        .shares
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
}

pub fn find_share_mut<'a>(config: &'a mut Config, name: &str) -> Option<&'a mut ShareConfig> {
    config
        .shares
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case(name))
}

pub fn normalize_alias_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn sanitize_share_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_paths_use_suffixes() {
        let cfg = Config::default();
        let tb = backend_mount_path(&cfg, "CORE", Backend::Tb);
        let fb = backend_mount_path(&cfg, "CORE", Backend::Fallback);
        assert!(tb.to_string_lossy().contains("core_tb"));
        assert!(fb.to_string_lossy().contains("core_fb"));
    }

    #[test]
    fn alias_target_joins_subpath() {
        let cfg = Config::default();
        let alias = AliasConfig {
            name: "projects".to_string(),
            path: "~/Shares/Links/projects".to_string(),
            share: "CORE".to_string(),
            target_subpath: "dev/projects".to_string(),
        };
        let target = alias_target_path(&cfg, &alias);
        assert!(
            target
                .to_string_lossy()
                .ends_with("/Shares/CORE/dev/projects")
        );
    }
}
