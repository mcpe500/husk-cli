//! Deterministic task routing — the harness, not the model, decides where a
//! task goes (prompt1.md: "hard thinking" ALWAYS goes to DeepSeek).
//!
//! Zero LLM calls: keyword/regex heuristics classify a prompt into a worker
//! context op (local MiniCPM, free) or a supervisor reasoning task (cloud,
//! with a reasoning_effort budget). No LLM ever picks the model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    High,
    Max,
}

impl ReasoningEffort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningEffort::Low => "low",
            ReasoningEffort::High => "high",
            ReasoningEffort::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteTarget {
    /// Local MiniCPM5-2B — context ops only, never hard reasoning.
    Worker,
    /// DeepSeek V4 Flash 0731 — all reasoning, with an effort budget.
    Supervisor,
}

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub target: RouteTarget,
    pub effort: Option<ReasoningEffort>,
    pub reason: &'static str,
}

const HARD_REASONING: &[&str] = &[
    "debug", "bug", "fix", "broken", "fails", "failing", "error in", "architecture", "refactor",
    "redesign", "design", "review", "security", "vulnerab", "plan", "optimize", "root cause",
    "why does", "kenapa", "mengapa", "perbaiki", "debugging", "race condition", "deadlock",
    "migration", "threat model", "trade-off", "tradeoff",
];

const CRITICAL: &[&str] = &[
    "security", "vulnerab", "migration", "architecture", "race condition", "deadlock",
    "root cause", "threat model",
];

const MAX_EFFORT: &[&str] = &["deeply", "thorough", "exhaustive", "step by step", "carefully", "max effort"];

const WORKER_OPS: &[&str] = &[
    "summarize", "summarise", "summary", "ringkas", "rangkum", "tldr",
    "read file", "baca file", "extract", "ekstrak", "list functions", "list all",
    "count", "hitung", "filter log", "log filter", "grep", "diff summary",
    "compress context", "fold context", "entity extraction", "what changed",
    "apa isi", "cari di file",
];

pub struct TaskRouter;

impl TaskRouter {
    /// Classify a user prompt. Deterministic and instant.
    pub fn classify(prompt: &str) -> RouteDecision {
        let lower = prompt.to_lowercase();

        // Worker ops win only when the prompt is *just* a context op —
        // no reasoning keyword anywhere in the text.
        let wants_worker = WORKER_OPS.iter().any(|k| lower.contains(k));
        let wants_reasoning = HARD_REASONING.iter().any(|k| lower.contains(k));
        if wants_worker && !wants_reasoning {
            return RouteDecision { target: RouteTarget::Worker, effort: None, reason: "context-op" };
        }

        let effort = if CRITICAL.iter().any(|k| lower.contains(k))
            || MAX_EFFORT.iter().any(|k| lower.contains(k))
        {
            ReasoningEffort::Max
        } else if wants_reasoning {
            ReasoningEffort::High
        } else if lower.chars().count() > 400 {
            // Long, unstructured prompts get real deliberation.
            ReasoningEffort::High
        } else {
            ReasoningEffort::Low
        };
        RouteDecision { target: RouteTarget::Supervisor, effort: Some(effort), reason: "reasoning" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_ops_go_to_worker() {
        for prompt in [
            "summarize the diff between main and HEAD",
            "ringkas isi file src/main.rs",
            "list all functions in src/providers",
            "extract the entity names from this log",
            "count occurrences of TODO in the repo",
        ] {
            let d = TaskRouter::classify(prompt);
            assert_eq!(d.target, RouteTarget::Worker, "{prompt} -> {d:?}");
            assert!(d.effort.is_none());
        }
    }

    #[test]
    fn hard_reasoning_never_reaches_worker_even_with_worker_words() {
        // Contains both "summarize" and "debug": reasoning wins. Always.
        let d = TaskRouter::classify("summarize this crash log and then debug the panic for me");
        assert_eq!(d.target, RouteTarget::Supervisor);
        assert_eq!(d.effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn effort_mapping_matches_complexity() {
        assert_eq!(TaskRouter::classify("review this security migration plan").effort, Some(ReasoningEffort::Max));
        assert_eq!(TaskRouter::classify("fix the race condition in the scheduler").effort, Some(ReasoningEffort::Max));
        assert_eq!(TaskRouter::classify("think step by step and fix this bug").effort, Some(ReasoningEffort::Max));
        assert_eq!(TaskRouter::classify("refactor the storage layer").effort, Some(ReasoningEffort::High));
        assert_eq!(TaskRouter::classify("hi").effort, Some(ReasoningEffort::Low));
    }

    #[test]
    fn long_prompts_get_high_effort() {
        let long = "please look at this ".repeat(60);
        assert_eq!(TaskRouter::classify(&long).effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn router_is_deterministic() {
        let p = "debug the failing cargo test in src/db";
        let a = TaskRouter::classify(p);
        let b = TaskRouter::classify(p);
        assert_eq!(a.target, b.target);
        assert_eq!(a.effort, b.effort);
        assert_eq!(a.reason, b.reason);
    }
}
