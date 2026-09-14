use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;

use super::{ChatMessage, ChatRequest, ChatResponse, ChatRole, LlmProvider};
use crate::config::Config;
use crate::tokenutil::TokenUsage;

pub struct AnthropicProvider {
    config: Config,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(config: Config) -> Self {
        Self { config, client: reqwest::Client::new() }
    }

    fn url(&self) -> String {
        format!("{}/messages", self.config.base_url.trim_end_matches('/'))
    }

    fn post(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        let mut req = self.client.post(self.url()).json(body);
        if !self.config.api_key.is_empty() {
            req = req.header("x-api-key", &self.config.api_key);
            req = req.header("anthropic-version", "2023-06-01");
        }
        req
    }

    fn request_body(req: &ChatRequest, stream: bool) -> serde_json::Value {
        let system: Vec<&str> = req
            .messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .collect();
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .filter(|m| m.role != ChatRole::System)
            .map(|m| serde_json::json!({"role": m.role.as_str(), "content": m.content}))
            .collect();
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": stream,
        });
        if !system.is_empty() {
            body["system"] = serde_json::json!(system.join("\n\n"));
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        body
    }

    fn parse_response(value: &serde_json::Value) -> ChatResponse {
        let content = value["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        let usage = value.get("usage").map(|u| {
            TokenUsage::new(
                u["input_tokens"].as_u64().unwrap_or(0),
                u["cache_read_input_tokens"].as_u64().unwrap_or(0),
                u["output_tokens"].as_u64().unwrap_or(0),
            )
        });
        ChatResponse { content, usage }
    }

    pub async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let resp = self.post(&Self::request_body(req, false)).send().await?;
        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(anyhow!("Anthropic API error: {err_text}"));
        }
        let value: serde_json::Value = resp.json().await?;
        Ok(Self::parse_response(&value))
    }

    pub async fn chat_stream(
        &self,
        req: &ChatRequest,
        token_tx: UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        let resp = self.post(&Self::request_body(req, true)).send().await?;
        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(anyhow!("Anthropic stream error: {err_text}"));
        }

        let mut stream = resp.bytes_stream();
        let mut content = String::new();
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&chunk?));
            while let Some(pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=pos).collect();
                let (delta, usage) = parse_anthropic_sse_line(line.trim_end());
                if let Some(text) = delta {
                    content.push_str(&text);
                    let _ = token_tx.send(text);
                }
                if let Some((inp, out)) = usage {
                    input_tokens = input_tokens.max(inp);
                    output_tokens = output_tokens.max(out);
                }
            }
        }
        Ok(ChatResponse { content, usage: Some(TokenUsage::new(input_tokens, 0, output_tokens)) })
    }
}

/// Parse one Anthropic SSE line. Returns (text delta, usage snapshot).
/// Pure: unit tested with canned events.
pub fn parse_anthropic_sse_line(line: &str) -> (Option<String>, Option<(u64, u64)>) {
    let payload = match line.strip_prefix("data:") {
        Some(p) => p.trim(),
        None => return (None, None),
    };
    if payload.is_empty() || payload == "[DONE]" {
        return (None, None);
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return (None, None);
    };
    let event_type = value["type"].as_str().unwrap_or("");
    let mut delta = None;
    let mut usage = None;
    match event_type {
        "content_block_delta" => {
            delta = value["delta"]["text"].as_str().map(str::to_string).filter(|s| !s.is_empty());
        }
        "message_start" => {
            let inp = value["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
            let out = value["message"]["usage"]["output_tokens"].as_u64().unwrap_or(0);
            usage = Some((inp, out));
        }
        "message_delta" => {
            let out = value["usage"]["output_tokens"].as_u64().unwrap_or(0);
            usage = Some((0, out));
        }
        _ => {}
    }
    (delta, usage)
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        self.chat(req).await
    }

    async fn chat_stream(
        &self,
        req: &ChatRequest,
        token_tx: UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        self.chat_stream(req, token_tx).await
    }

    async fn chat_simple(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: Option<UnboundedSender<String>>,
    ) -> Result<ChatResponse> {
        let req = ChatRequest::new(
            self.config.model.clone(),
            vec![
                ChatMessage::new(ChatRole::System, system_prompt),
                ChatMessage::new(ChatRole::User, user_prompt),
            ],
        )
        .with_max_tokens(self.config.max_tokens as u32);
        match token_tx {
            Some(tx) => self.chat_stream(&req, tx).await,
            None => self.chat(&req).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_sse_parses_deltas_and_usage() {
        let (delta, _) = parse_anthropic_sse_line(
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hi"}}"#,
        );
        assert_eq!(delta.as_deref(), Some("Hi"));

        let (_, usage) = parse_anthropic_sse_line(
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":25,"output_tokens":1}}}"#,
        );
        assert_eq!(usage, Some((25, 1)));

        let (_, usage) = parse_anthropic_sse_line(
            r#"data: {"type":"message_delta","usage":{"output_tokens":42}}"#,
        );
        assert_eq!(usage, Some((0, 42)));

        let (delta, _) = parse_anthropic_sse_line("event: ping");
        assert!(delta.is_none());
    }

    #[test]
    fn request_body_splits_system_and_chat_messages() {
        let req = ChatRequest::new(
            "claude-3-5-sonnet",
            vec![
                ChatMessage::new(ChatRole::System, "be terse"),
                ChatMessage::new(ChatRole::User, "hello"),
                ChatMessage::new(ChatRole::Assistant, "hi"),
                ChatMessage::new(ChatRole::User, "bye"),
            ],
        )
        .with_max_tokens(777);
        let body = AnthropicProvider::request_body(&req, false);
        assert_eq!(body["system"], "be terse");
        assert_eq!(body["messages"].as_array().unwrap().len(), 3, "system must not repeat in messages");
        assert_eq!(body["max_tokens"], 777);
        assert_eq!(body["stream"], false);
    }
}
