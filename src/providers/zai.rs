use anyhow::Result;
use async_trait::async_trait;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::LlmProvider;

pub enum ZaiProtocol {
    Anthropic(AnthropicProvider),
    OpenAi(OpenAiProvider),
}

pub struct ZaiProvider {
    inner: ZaiProtocol,
}

impl ZaiProvider {
    pub fn new_anthropic(api_key: String, base_url: String, model: String, max_tokens: usize) -> Self {
        let url = if base_url.is_empty() {
            "https://api.z.ai/api/anthropic".to_string()
        } else {
            base_url
        };
        Self {
            inner: ZaiProtocol::Anthropic(AnthropicProvider::new(api_key, url, model, max_tokens)),
        }
    }

    pub fn new_openai(api_key: String, base_url: String, model: String, max_tokens: usize) -> Self {
        let url = if base_url.is_empty() {
            "https://api.z.ai/api/coding/paas/v4".to_string()
        } else {
            base_url
        };
        Self {
            inner: ZaiProtocol::OpenAi(OpenAiProvider::new(api_key, url, model, max_tokens)),
        }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        match &self.inner {
            ZaiProtocol::Anthropic(p) => p.completion(system_prompt, user_prompt).await,
            ZaiProtocol::OpenAi(p) => p.completion(system_prompt, user_prompt).await,
        }
    }
}
