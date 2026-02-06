use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub temperature: f32,
    pub report_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self {
            model: "deepseek-reasoner".to_string(),
            temperature: 0.8,
            report_dir: PathBuf::from(home).join(".lian/pacman"),
        }
    }
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_path = PathBuf::from(home).join(".config/lian-pacman/config.toml");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    #[allow(dead_code)]
    pub fn ensure_report_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.report_dir)?;
        Ok(())
    }
}
