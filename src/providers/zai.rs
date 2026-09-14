use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::{ChatRequest, ChatResponse, LlmProvider};
use crate::config::Config;

/// Z.AI GLM provider. Routes by base_url protocol: `.../anthropic` speaks
/// Anthropic Messages, everything else speaks OpenAI-compatible chat.
/// Model IDs verified live: `glm-4` is retired; use `glm-5.2`/`glm-5.3`.
pub struct ZaiProvider {
    inner: Box<dyn LlmProvider>,
}

impl ZaiProvider {
    pub fn new(config: Config) -> Self {
        let inner: Box<dyn LlmProvider> = if config.base_url.contains("anthropic") {
            Box::new(AnthropicProvider::new(config))
        } else {
            Box::new(OpenAiProvider::new(config))
        };
        Self { inner }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        self.inner.chat(req).await
    }

    async fn chat_stream(
        &self,
        req: &ChatRequest,
        token_tx: UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        self.inner.chat_stream(req, token_tx).await
    }

    async fn chat_simple(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: Option<UnboundedSender<String>>,
    ) -> Result<ChatResponse> {
        self.inner.chat_simple(system_prompt, user_prompt, token_tx).await
    }
}
