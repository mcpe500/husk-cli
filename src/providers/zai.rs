use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::LlmProvider;
use crate::config::Config;

pub struct ZaiProvider {
    config: Config,
    client: reqwest::Client,
}

impl ZaiProvider {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let is_anthropic = self.config.base_url.contains("anthropic");

        if is_anthropic {
            let url = format!("{}/messages", self.config.base_url.trim_end_matches('/'));
            let body = json!({
                "model": self.config.model,
                "system": system_prompt,
                "messages": [
                    { "role": "user", "content": user_prompt }
                ],
                "max_tokens": self.config.max_tokens
            });

            let mut req = self.client.post(&url).json(&body);
            if !self.config.api_key.is_empty() {
                req = req.header("x-api-key", &self.config.api_key);
                req = req.header("anthropic-version", "2023-06-01");
            }

            let resp = req.send().await?;
            if !resp.status().is_success() {
                let err_text = resp.text().await?;
                return Err(anyhow!("GLM Z.AI Anthropic API error: {}", err_text));
            }

            let json_resp: serde_json::Value = resp.json().await?;
            let content = json_resp["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();

            Ok(content)
        } else {
            let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
            let body = json!({
                "model": self.config.model,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "max_tokens": self.config.max_tokens
            });

            let mut req = self.client.post(&url).json(&body);
            if !self.config.api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
            }

            let resp = req.send().await?;
            if !resp.status().is_success() {
                let err_text = resp.text().await?;
                return Err(anyhow!("GLM Z.AI OpenAI API error: {}", err_text));
            }

            let json_resp: serde_json::Value = resp.json().await?;
            let content = json_resp["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            Ok(content)
        }
    }

    async fn stream_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: UnboundedSender<String>,
    ) -> Result<String> {
        let full_text = self.completion(system_prompt, user_prompt).await.unwrap_or_default();
        for chunk in full_text.split_whitespace() {
            let _ = token_tx.send(format!("{} ", chunk));
            tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
        }
        Ok(full_text)
    }
}
