pub mod anthropic;
pub mod minimax;
pub mod openai;
pub mod zai;

use anyhow::Result;
use async_trait::async_trait;
use crate::config::{Config, ProviderPreset};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;
}

pub fn create_provider(config: &Config) -> Result<Box<dyn LlmProvider>> {
    match config.provider {
        ProviderPreset::ZaiAnthropic => Ok(Box::new(zai::ZaiProvider::new_anthropic(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
        ProviderPreset::ZaiOpenAi => Ok(Box::new(zai::ZaiProvider::new_openai(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
        ProviderPreset::MiniMaxOpenAi => Ok(Box::new(minimax::MiniMaxProvider::new_openai(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
        ProviderPreset::MiniMaxAnthropic => Ok(Box::new(minimax::MiniMaxProvider::new_anthropic(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
        ProviderPreset::OpenAi | ProviderPreset::Custom => Ok(Box::new(openai::OpenAiProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
        ProviderPreset::Anthropic => Ok(Box::new(anthropic::AnthropicProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
            config.max_tokens,
        ))),
    }
}
