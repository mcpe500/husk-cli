use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

pub mod anthropic;
pub mod deepseek;
pub mod minimax;
pub mod openai;
pub mod zai;

use crate::router::ReasoningEffort;
use crate::tokenutil::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

/// A provider-agnostic chat request. `reasoning_effort` is only emitted by
/// providers that support it (DeepSeek V4 Flash 0731: low/high/max).
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self { model: model.into(), messages, max_tokens: None, temperature: None, reasoning_effort: None }
    }

    pub fn simple(model: impl Into<String>, system: &str, user: &str) -> Self {
        Self::new(
            model,
            vec![
                ChatMessage::new(ChatRole::System, system),
                ChatMessage::new(ChatRole::User, user),
            ],
        )
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Option<TokenUsage>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// One-shot chat completion.
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;

    /// Streaming completion; deltas are pushed to `token_tx`, and the
    /// assembled response (including usage when the provider reports it)
    /// is returned at the end.
    async fn chat_stream(
        &self,
        req: &ChatRequest,
        token_tx: UnboundedSender<String>,
    ) -> Result<ChatResponse>;

    /// Legacy convenience: simple system+user exchange driven by the
    /// provider's configured model/max_tokens, optionally streamed.
    async fn chat_simple(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: Option<UnboundedSender<String>>,
    ) -> Result<ChatResponse>;

    async fn completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        Ok(self.chat_simple(system_prompt, user_prompt, None).await?.content)
    }

    async fn stream_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        token_tx: UnboundedSender<String>,
    ) -> Result<String> {
        Ok(self.chat_simple(system_prompt, user_prompt, Some(token_tx)).await?.content)
    }
}

/// Wire format for reasoning effort. OpenAI-compatible servers differ:
/// Together/DeepSeek-native use top-level `reasoning_effort`, while
/// Netra Runtime uses a `reasoning` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningStyle {
    /// `"reasoning_effort": "low|high|max"` (Together AI).
    #[default]
    OpenAiEffort,
    /// `"reasoning": {"enabled": true, "effort": "...", "exclude": false}`
    /// (api.netraruntime.com).
    Netra,
}

/// Build the OpenAI-compatible `/chat/completions` request body.
/// Pure so it can be unit tested without network.
pub fn openai_chat_body(req: &ChatRequest, stream: bool, style: ReasoningStyle) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": req.model,
        "messages": req.messages.iter()
            .map(|m| serde_json::json!({"role": m.role.as_str(), "content": m.content}))
            .collect::<Vec<_>>(),
    });
    if let Some(max) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    match (style, req.reasoning_effort) {
        (ReasoningStyle::OpenAiEffort, Some(effort)) => {
            body["reasoning_effort"] = serde_json::json!(effort.as_str());
        }
        (ReasoningStyle::Netra, Some(effort)) => {
            body["reasoning"] = serde_json::json!({
                "enabled": true,
                "effort": effort.as_str(),
                "exclude": false,
            });
        }
        (_, None) => {}
    }
    if stream {
        body["stream"] = serde_json::json!(true);
        // Ask for a final usage-only chunk (OpenAI-compatible convention).
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }
    body
}

/// One parsed server-sent event from an OpenAI-compatible stream.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SseEvent {
    pub content_delta: Option<String>,
    pub usage: Option<TokenUsage>,
    pub done: bool,
}

/// Parse a single SSE `data:` payload line. Returns None for non-data lines.
/// Pure: unit tested with canned chunks.
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let payload = line.strip_prefix("data:")?;
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    if payload == "[DONE]" {
        return Some(SseEvent { done: true, ..Default::default() });
    }
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let mut event = SseEvent::default();
    if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
        if !delta.is_empty() {
            event.content_delta = Some(delta.to_string());
        }
    } else if let Some(content) = value["choices"][0]["message"]["content"].as_str() {
        if !content.is_empty() {
            event.content_delta = Some(content.to_string());
        }
    }
    if let Some(usage) = value.get("usage") {
        if usage.is_object() && !usage.is_null() {
            let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0);
            let cached = usage["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(usage["cached_tokens"].as_u64().unwrap_or(0));
            let completion = usage["completion_tokens"].as_u64().unwrap_or(0);
            event.usage = Some(TokenUsage::new(prompt, cached, completion));
        }
    }
    Some(event)
}

/// Parse a non-streamed OpenAI-compatible response body.
pub fn parse_openai_response(value: &serde_json::Value) -> ChatResponse {
    let content = value["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    let usage = value.get("usage").filter(|u| u.is_object()).map(|u| {
        TokenUsage::new(
            u["prompt_tokens"].as_u64().unwrap_or(0),
            u["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(u["cached_tokens"].as_u64().unwrap_or(0)),
            u["completion_tokens"].as_u64().unwrap_or(0),
        )
    });
    ChatResponse { content, usage }
}

pub fn create_provider(config: &crate::config::Config) -> Result<Box<dyn LlmProvider>> {
    use crate::config::ProviderPreset;
    // API key may come from the environment when unset in config.
    let mut config = config.clone();
    if config.api_key.is_empty() {
        if let Ok(key) = std::env::var("HUSK_API_KEY")
            .or_else(|_| std::env::var("NETRA_API_KEY"))
        {
            config.api_key = key;
        }
    }
    match config.provider {
        ProviderPreset::ZaiAnthropic | ProviderPreset::ZaiOpenAi => {
            Ok(Box::new(zai::ZaiProvider::new(config)))
        }
        ProviderPreset::MiniMaxOpenAi | ProviderPreset::MiniMaxAnthropic => {
            Ok(Box::new(minimax::MiniMaxProvider::new(config)))
        }
        ProviderPreset::OpenAi => Ok(Box::new(openai::OpenAiProvider::new(config))),
        ProviderPreset::Anthropic => Ok(Box::new(anthropic::AnthropicProvider::new(config))),
        // DeepSeek V4 Flash 0731 via Netra Runtime: OpenAI-compatible with
        // the `reasoning: {enabled, effort, exclude}` wire object.
        ProviderPreset::Deepseek => Ok(Box::new(deepseek::DeepseekProvider::new(config))),
        ProviderPreset::Custom => Ok(Box::new(openai::OpenAiProvider::new(config))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_body_contains_reasoning_effort_only_when_set() {
        let mut req = ChatRequest::simple(
            "deepseek/deepseek-v4-flash-0731",
            "be terse",
            "debug the panic",
        )
        .with_max_tokens(512)
        .with_effort(ReasoningEffort::Max);

        let body = openai_chat_body(&req, false, ReasoningStyle::OpenAiEffort);
        assert_eq!(body["model"], "deepseek/deepseek-v4-flash-0731");
        assert_eq!(body["reasoning_effort"], "max");
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("stream").is_none());

        req.reasoning_effort = None;
        let body = openai_chat_body(&req, false, ReasoningStyle::OpenAiEffort);
        assert!(body.get("reasoning_effort").is_none(), "must not send effort when unset");
    }

    #[test]
    fn netra_reasoning_uses_the_reasoning_object() {
        // Verified against the user's netraruntime curl sample:
        // "reasoning": {"enabled": true, "effort": "low", "exclude": false}
        let req = ChatRequest::simple("deepseek/deepseek-v4-flash-0731", "s", "u")
            .with_effort(ReasoningEffort::Low);
        let body = openai_chat_body(&req, false, ReasoningStyle::Netra);
        assert_eq!(body["reasoning"]["enabled"], true);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["reasoning"]["exclude"], false);
        assert!(body.get("reasoning_effort").is_none(), "netra wire must not use reasoning_effort");

        // Unset effort → no reasoning object at all.
        let plain = ChatRequest::simple("m", "s", "u");
        let body = openai_chat_body(&plain, false, ReasoningStyle::Netra);
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn stream_body_requests_usage() {
        let req = ChatRequest::simple("m", "s", "u");
        let body = openai_chat_body(&req, true, ReasoningStyle::Netra);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn sse_lines_parse_deltas_usage_and_done() {
        let delta = parse_sse_line(
            r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#,
        )
        .unwrap();
        assert_eq!(delta.content_delta.as_deref(), Some("Hel"));
        assert!(delta.usage.is_none());

        let usage = parse_sse_line(
            r#"data: {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":80}}}"#,
        )
        .unwrap();
        assert_eq!(usage.usage, Some(TokenUsage::new(100, 80, 7)));

        let done = parse_sse_line("data: [DONE]").unwrap();
        assert!(done.done);

        assert!(parse_sse_line(": keep-alive comment").is_none());
        assert!(parse_sse_line("event: ping").is_none());
        assert!(parse_sse_line("data: ").is_none());
        // Non-stream-shaped payloads still parse their content.
        let full = parse_sse_line(r#"data: {"choices":[{"message":{"content":"hi"}}]}"#).unwrap();
        assert_eq!(full.content_delta.as_deref(), Some("hi"));
    }

    #[test]
    fn openai_response_parses_content_and_usage() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"choices":[{"message":{"content":"answer"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":4}}}"#,
        )
        .unwrap();
        let resp = parse_openai_response(&value);
        assert_eq!(resp.content, "answer");
        assert_eq!(resp.usage, Some(TokenUsage::new(10, 4, 2)));
    }
}
