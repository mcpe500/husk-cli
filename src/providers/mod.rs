use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

pub mod anthropic;
pub mod minimax;
pub mod openai;
pub mod zai;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;
    
    async fn stream_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: UnboundedSender<String>,
    ) -> Result<String> {
        let full = self.completion(system_prompt, user_prompt).await?;
        let _ = token_tx.send(full.clone());
        Ok(full)
    }
}

pub fn create_provider(config: &crate::config::Config) -> Result<Box<dyn LlmProvider>> {
    use crate::config::ProviderPreset;
    match config.provider {
        ProviderPreset::ZaiAnthropic | ProviderPreset::ZaiOpenAi => {
            Ok(Box::new(zai::ZaiProvider::new(config.clone())))
        }
        ProviderPreset::MiniMaxOpenAi | ProviderPreset::MiniMaxAnthropic => {
            Ok(Box::new(minimax::MiniMaxProvider::new(config.clone())))
        }
        ProviderPreset::OpenAi => Ok(Box::new(openai::OpenAiProvider::new(config.clone()))),
        ProviderPreset::Anthropic => Ok(Box::new(anthropic::AnthropicProvider::new(config.clone()))),
        ProviderPreset::Custom => Ok(Box::new(openai::OpenAiProvider::new(config.clone()))),
    }
}
