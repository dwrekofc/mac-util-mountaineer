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
    #[serde(default = "default_check_interval_secs")]
    pub check_interval_secs: u64,
    #[serde(default = "default_auto_failback")]
    pub auto_failback: bool,
    #[serde(default = "default_auto_failback_stable_secs")]
    pub auto_failback_stable_secs: u64,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_lsof_recheck")]
    pub lsof_recheck: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            shares_root: default_shares_root(),
            check_interval_secs: default_check_interval_secs(),
            auto_failback: default_auto_failback(),
            auto_failback_stable_secs: default_auto_failback_stable_secs(),
            connect_timeout_ms: default_connect_timeout_ms(),
            lsof_recheck: default_lsof_recheck(),
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

fn default_lsof_recheck() -> bool {
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
    validate(&config)?;
    Ok(config)
}

/// Validate config on load per spec 02: reject duplicate share names,
/// empty required fields, and duplicate alias names.
fn validate(config: &Config) -> Result<()> {
    let mut seen_shares = std::collections::HashSet::new();
    for share in &config.shares {
        if share.name.trim().is_empty() {
            anyhow::bail!("config error: share has empty name");
        }
        if share.thunderbolt_host.trim().is_empty() {
            anyhow::bail!(
                "config error: share '{}' has empty thunderbolt_host",
                share.name
            );
        }
        if share.fallback_host.trim().is_empty() {
            anyhow::bail!(
                "config error: share '{}' has empty fallback_host",
                share.name
            );
        }
        let key = share.name.to_ascii_lowercase();
        if !seen_shares.insert(key) {
            anyhow::bail!("config error: duplicate share name '{}'", share.name);
        }
    }

    let mut seen_aliases = std::collections::HashSet::new();
    for alias in &config.aliases {
        if alias.name.trim().is_empty() {
            anyhow::bail!("config error: alias has empty name");
        }
        let key = alias.name.to_ascii_lowercase();
        if !seen_aliases.insert(key) {
            anyhow::bail!("config error: duplicate alias name '{}'", alias.name);
        }
    }

    Ok(())
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let toml = toml::to_string_pretty(config)?;

    // Atomic write: write to .tmp then rename, so a crash mid-write won't corrupt config.toml
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &toml)
        .with_context(|| format!("failed writing temp config {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed renaming temp config to {}", path.display()))?;
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

pub fn share_stable_path(config: &Config, share_name: &str) -> PathBuf {
    shares_root_path(config).join(share_name)
}

/// Returns the macOS-managed volume mount point at `/Volumes/<share_name>`.
/// Under single-mount architecture, both TB and Fallback mount to the same path.
/// macOS manages the `/Volumes/` directory — Mountaineer must NOT create it.
pub fn volume_mount_path(share_name: &str) -> PathBuf {
    PathBuf::from("/Volumes").join(share_name)
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

pub fn normalize_alias_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_mount_path_uses_volumes_dir() {
        let path = volume_mount_path("CORE");
        assert_eq!(path, PathBuf::from("/Volumes/CORE"));
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

    fn make_share(name: &str) -> ShareConfig {
        ShareConfig {
            name: name.to_string(),
            username: "user".to_string(),
            thunderbolt_host: "10.0.0.1".to_string(),
            fallback_host: "192.168.1.1".to_string(),
            share_name: name.to_string(),
        }
    }

    #[test]
    fn validate_rejects_duplicate_share_names() {
        let cfg = Config {
            shares: vec![make_share("CORE"), make_share("core")],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("duplicate share name"));
    }

    #[test]
    fn validate_rejects_empty_share_name() {
        let mut share = make_share("");
        share.name = "  ".to_string();
        let cfg = Config {
            shares: vec![share],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("empty name"));
    }

    #[test]
    fn validate_rejects_empty_thunderbolt_host() {
        let mut share = make_share("CORE");
        share.thunderbolt_host = "".to_string();
        let cfg = Config {
            shares: vec![share],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("empty thunderbolt_host"));
    }

    #[test]
    fn validate_rejects_empty_fallback_host() {
        let mut share = make_share("CORE");
        share.fallback_host = " ".to_string();
        let cfg = Config {
            shares: vec![share],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("empty fallback_host"));
    }

    #[test]
    fn validate_rejects_duplicate_alias_names() {
        let cfg = Config {
            aliases: vec![
                AliasConfig {
                    name: "projects".to_string(),
                    path: "~/Shares/Links/projects".to_string(),
                    share: "CORE".to_string(),
                    target_subpath: "dev/projects".to_string(),
                },
                AliasConfig {
                    name: "PROJECTS".to_string(),
                    path: "~/Shares/Links/projects2".to_string(),
                    share: "CORE".to_string(),
                    target_subpath: "dev/projects2".to_string(),
                },
            ],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("duplicate alias name"));
    }

    #[test]
    fn validate_accepts_valid_config() {
        let cfg = Config {
            shares: vec![make_share("CORE"), make_share("DATA")],
            aliases: vec![AliasConfig {
                name: "projects".to_string(),
                path: "~/Shares/Links/projects".to_string(),
                share: "CORE".to_string(),
                target_subpath: "dev/projects".to_string(),
            }],
            ..Config::default()
        };
        validate(&cfg).expect("valid config should pass validation");
    }

    #[test]
    fn config_roundtrip_toml() {
        let cfg = Config {
            global: GlobalConfig {
                lsof_recheck: false,
                auto_failback: true,
                check_interval_secs: 5,
                connect_timeout_ms: 1500,
                ..GlobalConfig::default()
            },
            shares: vec![make_share("CORE")],
            ..Config::default()
        };
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert!(!parsed.global.lsof_recheck);
        assert!(parsed.global.auto_failback);
        assert_eq!(parsed.global.check_interval_secs, 5);
        assert_eq!(parsed.global.connect_timeout_ms, 1500);
        assert_eq!(parsed.shares.len(), 1);
        assert_eq!(parsed.shares[0].name, "CORE");
    }

    // --- P7.1: Config load/save round-trip with filesystem ---

    #[test]
    fn config_save_and_load_roundtrip_filesystem() {
        // Save a config to a temp directory, then load it back and verify fidelity.
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config.toml");

        let cfg = Config {
            global: GlobalConfig {
                lsof_recheck: false,
                auto_failback: true,
                check_interval_secs: 10,
                connect_timeout_ms: 2000,
                auto_failback_stable_secs: 60,
                shares_root: "~/MyShares".to_string(),
            },
            shares: vec![make_share("CORE"), make_share("DATA")],
            aliases: vec![AliasConfig {
                name: "projects".to_string(),
                path: "~/Shares/Links/projects".to_string(),
                share: "CORE".to_string(),
                target_subpath: "dev/projects".to_string(),
            }],
        };

        // Write config to the temp file
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        std::fs::write(&config_file, &toml_str).unwrap();

        // Read it back and parse
        let contents = std::fs::read_to_string(&config_file).unwrap();
        let parsed: Config = toml::from_str(&contents).unwrap();

        // Verify all fields survive the round-trip
        assert_eq!(parsed.global.shares_root, "~/MyShares");
        assert_eq!(parsed.global.check_interval_secs, 10);
        assert!(parsed.global.auto_failback);
        assert_eq!(parsed.global.auto_failback_stable_secs, 60);
        assert_eq!(parsed.global.connect_timeout_ms, 2000);
        assert!(!parsed.global.lsof_recheck);
        assert_eq!(parsed.shares.len(), 2);
        assert_eq!(parsed.shares[0].name, "CORE");
        assert_eq!(parsed.shares[1].name, "DATA");
        assert_eq!(parsed.shares[0].username, "user");
        assert_eq!(parsed.shares[0].thunderbolt_host, "10.0.0.1");
        assert_eq!(parsed.shares[0].fallback_host, "192.168.1.1");
        assert_eq!(parsed.aliases.len(), 1);
        assert_eq!(parsed.aliases[0].name, "projects");
        assert_eq!(parsed.aliases[0].share, "CORE");
        assert_eq!(parsed.aliases[0].target_subpath, "dev/projects");
    }

    #[test]
    fn config_atomic_save_creates_file() {
        // Verify save() creates the directory and file atomically.
        // We can't easily test the real config_path() without modifying HOME,
        // so we test the serialization + atomic write pattern directly.
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("subdir").join("config.toml");
        let tmp_file = config_file.with_extension("toml.tmp");

        let cfg = Config {
            shares: vec![make_share("NAS")],
            ..Config::default()
        };

        // Manually replicate save() logic against our temp path
        std::fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        std::fs::write(&tmp_file, &toml_str).unwrap();
        std::fs::rename(&tmp_file, &config_file).unwrap();

        // Verify the final file exists and the tmp file was consumed by rename
        assert!(config_file.exists());
        assert!(!tmp_file.exists());

        // Verify content
        let contents = std::fs::read_to_string(&config_file).unwrap();
        let parsed: Config = toml::from_str(&contents).unwrap();
        assert_eq!(parsed.shares.len(), 1);
        assert_eq!(parsed.shares[0].name, "NAS");
    }

    #[test]
    fn config_default_values_match_spec() {
        // Spec 02 mandates specific defaults. Verify they're correct.
        let cfg = Config::default();
        assert_eq!(cfg.global.shares_root, "~/Shares");
        assert_eq!(cfg.global.check_interval_secs, 2);
        assert!(!cfg.global.auto_failback); // spec 02: auto_failback defaults to false
        assert_eq!(cfg.global.auto_failback_stable_secs, 30);
        assert_eq!(cfg.global.connect_timeout_ms, 800);
        assert!(cfg.global.lsof_recheck); // spec 02: lsof_recheck defaults to true
        assert!(cfg.shares.is_empty());
        assert!(cfg.aliases.is_empty());
    }

    #[test]
    fn config_missing_fields_get_defaults() {
        // When TOML only has partial config, missing fields should get defaults.
        let toml_str = r#"
[global]
auto_failback = true
"#;
        let parsed: Config = toml::from_str(toml_str).unwrap();
        assert!(parsed.global.auto_failback);
        // All other fields should be defaults
        assert_eq!(parsed.global.shares_root, "~/Shares");
        assert_eq!(parsed.global.check_interval_secs, 2);
        assert_eq!(parsed.global.auto_failback_stable_secs, 30);
        assert_eq!(parsed.global.connect_timeout_ms, 800);
        assert!(parsed.global.lsof_recheck);
    }

    #[test]
    fn config_empty_toml_gives_defaults() {
        // An empty TOML string should produce Config::default().
        let parsed: Config = toml::from_str("").unwrap();
        assert_eq!(parsed.global.shares_root, "~/Shares");
        assert!(!parsed.global.auto_failback);
        assert!(parsed.shares.is_empty());
        assert!(parsed.aliases.is_empty());
    }

    #[test]
    fn config_alias_default_target_subpath_is_empty() {
        // Spec 07: target_subpath defaults to empty string via serde(default)
        let toml_str = r#"
[[aliases]]
name = "docs"
path = "~/Shares/Links/docs"
share = "CORE"
"#;
        let parsed: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.aliases[0].target_subpath, "");
    }

    #[test]
    fn expand_path_tilde_only() {
        let expanded = expand_path("~");
        assert!(
            expanded.is_absolute(),
            "~ should expand to an absolute path"
        );
    }

    #[test]
    fn expand_path_tilde_prefix() {
        let expanded = expand_path("~/Shares");
        assert!(expanded.to_string_lossy().ends_with("/Shares"));
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn expand_path_absolute_unchanged() {
        let expanded = expand_path("/usr/local/bin");
        assert_eq!(expanded, PathBuf::from("/usr/local/bin"));
    }

    #[test]
    fn expand_path_relative_unchanged() {
        let expanded = expand_path("relative/path");
        assert_eq!(expanded, PathBuf::from("relative/path"));
    }

    #[test]
    fn find_share_case_insensitive() {
        let cfg = Config {
            shares: vec![make_share("CORE")],
            ..Config::default()
        };
        assert!(find_share(&cfg, "core").is_some());
        assert!(find_share(&cfg, "CORE").is_some());
        assert!(find_share(&cfg, "Core").is_some());
        assert!(find_share(&cfg, "missing").is_none());
    }

    #[test]
    fn config_path_under_mountaineer_dir() {
        let path = config_path();
        assert!(path.to_string_lossy().contains(".mountaineer"));
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn state_path_under_mountaineer_dir() {
        let path = state_path();
        assert!(path.to_string_lossy().contains(".mountaineer"));
        assert!(path.to_string_lossy().ends_with("state.json"));
    }

    #[test]
    fn shares_root_path_expands_tilde() {
        let cfg = Config::default(); // shares_root = "~/Shares"
        let path = shares_root_path(&cfg);
        assert!(!path.to_string_lossy().starts_with("~"));
        assert!(path.to_string_lossy().ends_with("/Shares"));
    }

    #[test]
    fn share_stable_path_joins_share_name() {
        let cfg = Config::default();
        let path = share_stable_path(&cfg, "CORE");
        assert!(path.to_string_lossy().ends_with("/Shares/CORE"));
    }

    #[test]
    fn default_alias_path_under_links() {
        let cfg = Config::default();
        let path = default_alias_path(&cfg, "myalias");
        assert!(path.to_string_lossy().ends_with("/Shares/Links/myalias"));
    }

    #[test]
    fn alias_target_path_strips_leading_trailing_slashes() {
        let cfg = Config::default();
        let alias = AliasConfig {
            name: "test".to_string(),
            path: "~/Shares/Links/test".to_string(),
            share: "CORE".to_string(),
            target_subpath: "/dev/projects/".to_string(),
        };
        let target = alias_target_path(&cfg, &alias);
        // Should strip leading and trailing slashes from subpath
        assert!(
            target
                .to_string_lossy()
                .ends_with("/Shares/CORE/dev/projects")
        );
    }

    #[test]
    fn backend_short_labels() {
        assert_eq!(Backend::Tb.short_label(), "tb");
        assert_eq!(Backend::Fallback.short_label(), "fallback");
    }

    #[test]
    fn validate_rejects_empty_alias_name() {
        let cfg = Config {
            aliases: vec![AliasConfig {
                name: "  ".to_string(),
                path: "~/Shares/Links/test".to_string(),
                share: "CORE".to_string(),
                target_subpath: "".to_string(),
            }],
            ..Config::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("empty name"));
    }
}
