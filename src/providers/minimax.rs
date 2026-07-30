use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::LlmProvider;
use crate::config::Config;

pub struct MiniMaxProvider {
    inner: Box<dyn LlmProvider>,
}

impl MiniMaxProvider {
    pub fn new(config: Config) -> Self {
        let is_anthropic = config.base_url.contains("anthropic");
        let inner: Box<dyn LlmProvider> = if is_anthropic {
            Box::new(AnthropicProvider::new(config.clone()))
        } else {
            Box::new(OpenAiProvider::new(config.clone()))
        };
        Self { inner }
    }
}

#[async_trait]
impl LlmProvider for MiniMaxProvider {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        self.inner.completion(system_prompt, user_prompt).await
    }

    async fn stream_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: UnboundedSender<String>,
    ) -> Result<String> {
        self.inner.stream_completion(system_prompt, user_prompt, token_tx).await
    }
}
