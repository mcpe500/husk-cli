use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[serde(rename = "deepseek")]
    Deepseek,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProvidersConfig {
    pub custom_providers: Vec<CustomProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderPreset,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: usize,
    pub telemetry_enabled: bool,
    /// Local embedded MiniCPM worker. Off by default: cloud-only startup,
    /// zero model RAM unless explicitly enabled.
    #[serde(default)]
    pub local: crate::local::LocalConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // DeepSeek via Netra Runtime is the harness's primary supervisor.
            provider: ProviderPreset::Deepseek,
            api_key: String::new(),
            base_url: "https://api.netraruntime.com/v1".to_string(),
            model: "deepseek/deepseek-v4-flash-0731".to_string(),
            max_tokens: 4096,
            telemetry_enabled: true,
            local: crate::local::LocalConfig::default(),
        }
    }
}

impl Config {
    pub fn husk_dir() -> PathBuf {
        let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        path.push(".husk");
        path
    }

    pub fn config_path() -> PathBuf {
        let mut path = Self::husk_dir();
        path.push("config.toml");
        path
    }

    /// Session history database — the husk.db analog of opencode.db.
    pub fn db_path() -> PathBuf {
        let mut path = Self::husk_dir();
        path.push("husk.db");
        path
    }

    pub fn providers_json_path() -> PathBuf {
        let mut path = Self::husk_dir();
        path.push("providers.json");
        path
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            let default_cfg = Config::default();
            default_cfg.save()?;
            return Ok(default_cfg);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config TOML at {:?}", path))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let husk_dir = Self::husk_dir();
        if !husk_dir.exists() {
            fs::create_dir_all(&husk_dir)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(Self::config_path(), content)?;
        Ok(())
    }

    pub fn apply_preset(&mut self, preset: ProviderPreset) {
        self.provider = preset.clone();
        match preset {
            ProviderPreset::ZaiAnthropic => {
                self.base_url = "https://api.z.ai/api/anthropic".to_string();
                // glm-4 has been retired by Z.AI; glm-5.2 is a verified current model.
                self.model = "glm-5.2".to_string();
            }
            ProviderPreset::ZaiOpenAi => {
                self.base_url = "https://api.z.ai/api/coding/paas/v4".to_string();
                self.model = "glm-5.2".to_string();
            }
            ProviderPreset::MiniMaxOpenAi => {
                self.base_url = "https://api.minimax.io/v1".to_string();
                self.model = "MiniMax-M3".to_string();
            }
            ProviderPreset::MiniMaxAnthropic => {
                self.base_url = "https://api.minimax.io/anthropic".to_string();
                self.model = "MiniMax-M3".to_string();
            }
            ProviderPreset::OpenAi => {
                self.base_url = "https://api.openai.com/v1".to_string();
                self.model = "gpt-4o".to_string();
            }
            ProviderPreset::Anthropic => {
                self.base_url = "https://api.anthropic.com/v1".to_string();
                self.model = "claude-3-5-sonnet".to_string();
            }
            ProviderPreset::Deepseek => {
                // Netra Runtime (user's provider): OpenAI-compatible,
                // reasoning via {"enabled","effort","exclude"} object.
                self.base_url = "https://api.netraruntime.com/v1".to_string();
                self.model = "deepseek/deepseek-v4-flash-0731".to_string();
            }
            ProviderPreset::Custom => {}
        }
    }

    pub fn load_custom_providers() -> Result<CustomProvidersConfig> {
        let path = Self::providers_json_path();
        if !path.exists() {
            return Ok(CustomProvidersConfig { custom_providers: Vec::new() });
        }
        let content = fs::read_to_string(&path)?;
        let parsed: CustomProvidersConfig = serde_json::from_str(&content)?;
        Ok(parsed)
    }

    pub fn save_custom_provider(provider: CustomProvider) -> Result<()> {
        let mut existing = Self::load_custom_providers().unwrap_or(CustomProvidersConfig { custom_providers: Vec::new() });
        existing.custom_providers.push(provider);

        let husk_dir = Self::husk_dir();
        if !husk_dir.exists() {
            fs::create_dir_all(&husk_dir)?;
        }
        let json_str = serde_json::to_string_pretty(&existing)?;
        fs::write(Self::providers_json_path(), json_str)?;
        Ok(())
    }
}
