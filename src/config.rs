use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "https://api.deepseek.com/chat/completions";

/// AI 分析开关配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// 系统更新后自动 AI 分析 (默认开启)
    #[serde(default = "default_true")]
    pub update: bool,
    /// 安装软件包后 AI 分析 (默认关闭)
    #[serde(default)]
    pub install: bool,
    /// 卸载软件包后 AI 分析 (默认关闭)
    #[serde(default)]
    pub remove: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            update: true,
            install: false,
            remove: false,
        }
    }
}

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
    #[serde(default)]
    pub ai: AiConfig,
}

impl Config {
    /// 获取 API URL，优先配置文件，否则使用默认值
    pub fn get_api_url(&self) -> &str {
        self.api_url.as_deref().unwrap_or(DEFAULT_API_URL)
    }

    /// 检查指定操作是否启用 AI 分析
    pub fn ai_enabled_for(&self, operation: &str) -> bool {
        match operation {
            "update" => self.ai.update,
            "install" => self.ai.install,
            "remove" => self.ai.remove,
            _ => false,
        }
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
            ai: AiConfig::default(),
        }
    }
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_path = PathBuf::from(home).join(".config/lian/config.toml");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// 保存配置到 ~/.config/lian/config.toml
    pub fn save(&self) -> Result<()> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_dir = PathBuf::from(&home).join(".config/lian");
        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }
}
