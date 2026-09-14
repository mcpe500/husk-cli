use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::{ChatRequest, ChatResponse, LlmProvider};
use crate::config::Config;

pub struct MiniMaxProvider {
    inner: Box<dyn LlmProvider>,
}

impl MiniMaxProvider {
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
impl LlmProvider for MiniMaxProvider {
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
