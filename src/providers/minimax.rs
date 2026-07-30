use anyhow::Result;
use async_trait::async_trait;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::LlmProvider;

pub enum MiniMaxProtocol {
    OpenAi(OpenAiProvider),
    Anthropic(AnthropicProvider),
}

pub struct MiniMaxProvider {
    inner: MiniMaxProtocol,
}

impl MiniMaxProvider {
    pub fn new_openai(api_key: String, base_url: String, model: String, max_tokens: usize) -> Self {
        let url = if base_url.is_empty() {
            "https://api.minimax.io/v1".to_string()
        } else {
            base_url
        };
        let m = if model.is_empty() {
            "MiniMax-M3".to_string()
        } else {
            model
        };
        Self {
            inner: MiniMaxProtocol::OpenAi(OpenAiProvider::new(api_key, url, m, max_tokens)),
        }
    }

    pub fn new_anthropic(api_key: String, base_url: String, model: String, max_tokens: usize) -> Self {
        let url = if base_url.is_empty() {
            "https://api.minimax.io/anthropic".to_string()
        } else {
            base_url
        };
        let m = if model.is_empty() {
            "MiniMax-M3".to_string()
        } else {
            model
        };
        Self {
            inner: MiniMaxProtocol::Anthropic(AnthropicProvider::new(api_key, url, m, max_tokens)),
        }
    }
}

#[async_trait]
impl LlmProvider for MiniMaxProvider {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        match &self.inner {
            MiniMaxProtocol::OpenAi(p) => p.completion(system_prompt, user_prompt).await,
            MiniMaxProtocol::Anthropic(p) => p.completion(system_prompt, user_prompt).await,
        }
    }
}
