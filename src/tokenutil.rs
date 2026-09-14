//! Token estimation, usage accounting, and cost math for the husk harness.
//!
//! Everything here is pure logic so it can be unit tested without I/O.
//! Estimation is a deterministic heuristic: ASCII text ≈ 4 chars/token,
//! CJK characters ≈ 1 token each. Provider-reported usage always wins
//! when available; estimates are only a fallback for pre-flight budgeting.

/// Deterministic token estimate for a string.
pub fn estimate_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut wide = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else if is_wide(ch) {
            wide += 1;
        } else {
            // Latin-1 supplements, emoji, etc. behave closer to ~2 chars/token.
            ascii += 2;
        }
    }
    ascii.div_ceil(4) + wide
}

fn is_wide(ch: char) -> bool {
    let c = ch as u32;
    // CJK Unified Ideographs + extensions A, Hangul, Kana, fullwidth forms.
    (0x1100..=0x11FF).contains(&c)
        || (0x2E80..=0x9FFF).contains(&c)
        || (0xAC00..=0xD7AF).contains(&c)
        || (0xF900..=0xFAFF).contains(&c)
        || (0xFF00..=0xFFEF).contains(&c)
        || (0x20000..=0x2FA1F).contains(&c)
}

/// Token usage as reported by a provider for one request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub cached_tokens: u64,
    pub completion_tokens: u64,
}

impl TokenUsage {
    pub fn new(prompt_tokens: u64, cached_tokens: u64, completion_tokens: u64) -> Self {
        Self { prompt_tokens, cached_tokens, completion_tokens }
    }

    /// Uncached (billable at full input rate) prompt tokens.
    pub fn uncached_prompt_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_sub(self.cached_tokens)
    }
}

/// USD price per 1M tokens.
#[derive(Debug, Clone, Copy)]
pub struct Pricing {
    pub input_per_mtok: f64,
    pub cached_input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl Pricing {
    pub fn cost_of(&self, usage: &TokenUsage) -> f64 {
        let m = 1_000_000f64;
        usage.uncached_prompt_tokens() as f64 / m * self.input_per_mtok
            + usage.cached_tokens as f64 / m * self.cached_input_per_mtok
            + usage.completion_tokens as f64 / m * self.output_per_mtok
    }
}

/// Per-session ledger separating supervisor (cloud, paid) from worker
/// (local MiniCPM, free) traffic, plus how many tokens context folding
/// kept away from the supervisor window.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenLedger {
    pub supervisor: TokenUsage,
    pub worker_input: u64,
    pub worker_output: u64,
    /// Raw context tokens that were compressed before reaching the supervisor.
    pub context_raw: u64,
    /// Compressed size of that same context as sent to the supervisor.
    pub context_compressed: u64,
}

impl TokenLedger {
    pub fn record_supervisor(&mut self, usage: TokenUsage) {
        self.supervisor.prompt_tokens += usage.prompt_tokens;
        self.supervisor.cached_tokens += usage.cached_tokens;
        self.supervisor.completion_tokens += usage.completion_tokens;
    }

    pub fn record_worker(&mut self, input_tokens: u64, output_tokens: u64) {
        self.worker_input += input_tokens;
        self.worker_output += output_tokens;
    }

    pub fn record_context_compression(&mut self, raw_tokens: u64, compressed_tokens: u64) {
        self.context_raw += raw_tokens;
        self.context_compressed += compressed_tokens.min(raw_tokens);
    }

    /// Tokens that never had to be processed by the paid model.
    pub fn saved_tokens(&self) -> u64 {
        self.context_raw.saturating_sub(self.context_compressed)
    }

    /// Cloud spend in USD. The local worker is free, so it never contributes.
    pub fn cloud_cost(&self, pricing: &Pricing) -> f64 {
        pricing.cost_of(&self.supervisor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_ascii_at_four_chars_per_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hi"), 1);
        // 12 ascii chars -> 3 tokens
        assert_eq!(estimate_tokens("hello world!"), 3);
    }

    #[test]
    fn estimates_cjk_as_one_token_per_char() {
        assert_eq!(estimate_tokens("你好世界"), 4);
        // Mixed: 2 CJK + "world!!" (7 ascii -> 2 tokens)
        assert_eq!(estimate_tokens("你好world!!"), 4);
    }

    #[test]
    fn uncached_prompt_excludes_cache() {
        let usage = TokenUsage::new(1000, 700, 200);
        assert_eq!(usage.uncached_prompt_tokens(), 300);
        // Cached can never exceed prompt in sane reports; saturate anyway.
        let bad = TokenUsage::new(100, 700, 0);
        assert_eq!(bad.uncached_prompt_tokens(), 0);
    }

    #[test]
    fn deepseek_flash_pricing_math() {
        // Verified Together pricing: $0.14 input, $0.03 cached, $0.28 output per 1M.
        let pricing = Pricing { input_per_mtok: 0.14, cached_input_per_mtok: 0.03, output_per_mtok: 0.28 };
        let usage = TokenUsage::new(1_000_000, 0, 1_000_000);
        let cost = pricing.cost_of(&usage);
        assert!((cost - 0.42).abs() < 1e-9, "cost was {cost}");

        let usage_cached = TokenUsage::new(1_000_000, 1_000_000, 0);
        assert!((pricing.cost_of(&usage_cached) - 0.03).abs() < 1e-9);
    }

    #[test]
    fn ledger_tracks_supervisor_worker_and_savings() {
        let mut ledger = TokenLedger::default();
        ledger.record_supervisor(TokenUsage::new(500, 100, 80));
        ledger.record_supervisor(TokenUsage::new(700, 200, 120));
        assert_eq!(ledger.supervisor.prompt_tokens, 1200);
        assert_eq!(ledger.supervisor.cached_tokens, 300);
        assert_eq!(ledger.supervisor.completion_tokens, 200);

        ledger.record_worker(50_000, 1_500);
        ledger.record_worker(20_000, 500);
        assert_eq!(ledger.worker_input, 70_000);
        assert_eq!(ledger.worker_output, 2_000);

        ledger.record_context_compression(40_000, 1_200);
        ledger.record_context_compression(10_000, 2_000); // compression can still shrink
        assert_eq!(ledger.saved_tokens(), 38_800 + 8_000);
    }

    #[test]
    fn worker_is_free_in_cloud_cost() {
        let mut ledger = TokenLedger::default();
        let pricing = Pricing { input_per_mtok: 0.14, cached_input_per_mtok: 0.03, output_per_mtok: 0.28 };
        ledger.record_worker(1_000_000, 1_000_000);
        assert_eq!(ledger.cloud_cost(&pricing), 0.0);
        ledger.record_supervisor(TokenUsage::new(1_000_000, 0, 0));
        assert!((ledger.cloud_cost(&pricing) - 0.14).abs() < 1e-9);
    }
}
