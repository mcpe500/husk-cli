use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::LlmProvider;

pub struct AnthropicProvider {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: usize,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: String, model: String, max_tokens: usize) -> Self {
        Self {
            api_key,
            base_url,
            model,
            max_tokens,
            client: Client::new(),
        }
    }
}

#[derive(Serialize)]
struct MessageContent {
    r#type: String,
    text: String,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: Vec<MessageContent>,
}

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    system: String,
    messages: Vec<Message>,
    max_tokens: usize,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        if self.api_key.is_empty() {
            return Ok(format!(
                "[Offline Demo Mode] Generated response via Anthropic Protocol:\n{}",
                user_prompt
            ));
        }

        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));
        let body = MessagesRequest {
            model: self.model.clone(),
            system: system_prompt.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![MessageContent {
                    r#type: "text".to_string(),
                    text: user_prompt.to_string(),
                }],
            }],
            max_tokens: self.max_tokens,
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(anyhow!("Anthropic API error: {}", err_text));
        }

        let res_json: MessagesResponse = response.json().await?;
        let text = res_json
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        Ok(text)
    }
}
