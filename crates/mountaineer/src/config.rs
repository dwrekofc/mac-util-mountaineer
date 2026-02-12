use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favorite {
    pub server: String,
    pub share: String,
    pub mount_point: String,
    pub mac_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub favorites: Vec<Favorite>,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("mountaineer")
        .join("config.json")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = std::fs::read_to_string(&path)?;
    let config: Config = serde_json::from_str(&contents)?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty() {
        let cfg = Config::default();
        assert!(cfg.favorites.is_empty());
    }

    #[test]
    fn favorite_roundtrip_json() {
        let fav = Favorite {
            server: "nas.local".to_string(),
            share: "DATA".to_string(),
            mount_point: "/Volumes/DATA".to_string(),
            mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
        };
        let json = serde_json::to_string(&fav).unwrap();
        let restored: Favorite = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.share, "DATA");
        assert_eq!(restored.mac_address, Some("aa:bb:cc:dd:ee:ff".to_string()));
    }
}
