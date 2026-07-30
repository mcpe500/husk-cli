use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderPreset {
    #[serde(rename = "zai-anthropic")]
    ZaiAnthropic,
    #[serde(rename = "zai-openai")]
    ZaiOpenAi,
    #[serde(rename = "minimax-openai")]
    MiniMaxOpenAi,
    #[serde(rename = "minimax-anthropic")]
    MiniMaxAnthropic,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "custom")]
    Custom,
}

impl Default for ProviderPreset {
    fn default() -> Self {
        ProviderPreset::ZaiAnthropic
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderPreset,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: usize,
    pub telemetry_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderPreset::ZaiAnthropic,
            api_key: std::env::var("HUSK_API_KEY")
                .or_else(|_| std::env::var("ZAI_API_KEY"))
                .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
                .unwrap_or_default(),
            base_url: "https://api.z.ai/api/anthropic".to_string(),
            model: "glm-4".to_string(),
            max_tokens: 4096,
            telemetry_enabled: true,
        }
    }
}

impl Config {
    pub fn apply_preset(&mut self, preset: ProviderPreset) {
        self.provider = preset.clone();
        match preset {
            ProviderPreset::ZaiAnthropic => {
                self.base_url = "https://api.z.ai/api/anthropic".to_string();
                self.model = "glm-4".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("ZAI_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::ZaiOpenAi => {
                self.base_url = "https://api.z.ai/api/coding/paas/v4".to_string();
                self.model = "glm-4".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("ZAI_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::MiniMaxOpenAi => {
                self.base_url = "https://api.minimax.io/v1".to_string();
                self.model = "MiniMax-M3".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("MINIMAX_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::MiniMaxAnthropic => {
                self.base_url = "https://api.minimax.io/anthropic".to_string();
                self.model = "MiniMax-M3".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("MINIMAX_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::OpenAi => {
                self.base_url = "https://api.openai.com/v1".to_string();
                self.model = "gpt-4o".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::Anthropic => {
                self.base_url = "https://api.anthropic.com/v1".to_string();
                self.model = "claude-3-5-sonnet-20241022".to_string();
                if self.api_key.is_empty() {
                    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                        self.api_key = key;
                    }
                }
            }
            ProviderPreset::Custom => {}
        }
    }

    pub fn husk_dir() -> PathBuf {
        PathBuf::from(".husk")
    }

    pub fn config_path() -> PathBuf {
        Self::husk_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config at {:?}", path))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML config at {:?}", path))?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::husk_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
