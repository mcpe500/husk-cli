use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::openai::OpenAiCompatible;
use super::{ChatRequest, ChatResponse, LlmProvider};
use crate::config::Config;

/// DeepSeek V4 Flash 0731 via Netra Runtime (OpenAI-compatible endpoint,
/// `reasoning: {enabled, effort, exclude}` wire object). This is the ONLY
/// model allowed to do hard reasoning in the harness; effort is set per
/// request by the deterministic TaskRouter.
pub struct DeepseekProvider {
    core: OpenAiCompatible,
    config: Config,
}

impl DeepseekProvider {
    pub fn new(config: Config) -> Self {
        Self {
            core: OpenAiCompatible::new(config.clone())
                .with_reasoning_style(super::ReasoningStyle::Netra),
            config,
        }
    }

    /// Default model ID and base URL (user-provided netraruntime curl sample).
    pub const DEFAULT_MODEL: &'static str = "deepseek/deepseek-v4-flash-0731";
    pub const DEFAULT_BASE_URL: &'static str = "https://api.netraruntime.com/v1";

    pub fn model(&self) -> &str {
        &self.config.model
    }
}

#[async_trait]
impl LlmProvider for DeepseekProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        self.core.chat(req).await
    }

    async fn chat_stream(
        &self,
        req: &ChatRequest,
        token_tx: UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        self.core.chat_stream(req, token_tx).await
    }

    async fn chat_simple(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: Option<UnboundedSender<String>>,
    ) -> Result<ChatResponse> {
        self.core.chat_simple(system_prompt, user_prompt, token_tx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_netra_runtime() {
        assert_eq!(DeepseekProvider::DEFAULT_MODEL, "deepseek/deepseek-v4-flash-0731");
        assert_eq!(DeepseekProvider::DEFAULT_BASE_URL, "https://api.netraruntime.com/v1");
        let mut config = Config::default();
        config.apply_preset(crate::config::ProviderPreset::Deepseek);
        let provider = DeepseekProvider::new(config.clone());
        assert_eq!(provider.model(), "deepseek/deepseek-v4-flash-0731");
        assert_eq!(config.base_url, DeepseekProvider::DEFAULT_BASE_URL);
    }
}
