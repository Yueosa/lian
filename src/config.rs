use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "https://api.deepseek.com/chat/completions";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub temperature: f32,
    pub report_dir: PathBuf,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
}

impl Config {
    /// 获取 API URL，优先配置文件，否则使用默认值
    pub fn get_api_url(&self) -> &str {
        self.api_url.as_deref().unwrap_or(DEFAULT_API_URL)
    }
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self {
            model: "deepseek-reasoner".to_string(),
            temperature: 0.8,
            report_dir: PathBuf::from(home).join(".lian/pacman"),
            api_url: None,
            api_key: None,
            proxy: None,
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
}
