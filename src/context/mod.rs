//! Context assembly and dynamic pruning (prompt1.md layers 2 & 3).
//!
//! Two jobs, both deterministic (no LLM decisions here):
//! 1. Prompt-cache-friendly ordering: stable blocks first, dynamic last, so
//!    the provider's prefix KV-cache survives across turns.
//! 2. Pruning: token window cap, tool-output dedup, old-tool purge — always
//!    protecting worker summaries and the last N turns.

use crate::db::Role;
use crate::tokenutil::estimate_tokens;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// CLAUDE.md/AGENTS.md, tool defs, skill router — byte-stable across turns.
    Stable,
    /// Dates, session state, ephemeral handoffs — always last.
    Dynamic,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
}

/// Assemble a system prompt: every stable block first (in order), every
/// dynamic block last. The stable prefix is byte-identical across turns as
/// long as the stable blocks are unchanged — that is the cache contract.
pub fn assemble_system(blocks: &[Block]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for b in blocks.iter().filter(|b| b.kind == BlockKind::Stable) {
        parts.push(b.text.as_str());
    }
    for b in blocks.iter().filter(|b| b.kind == BlockKind::Dynamic) {
        parts.push(b.text.as_str());
    }
    parts.join("\n\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MsgKind {
    Normal,
    /// Tool output: tool name + hash of arguments for dedup.
    ToolOutput { tool: String, args_hash: u64 },
    /// Worker summary artifact — never pruned, never purged.
    Summary,
}

#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub kind: MsgKind,
}

#[derive(Debug, Default)]
pub struct PrunedContext {
    pub kept: Vec<ContextMessage>,
    pub tokens_before: u64,
    pub tokens_after: u64,
    pub dropped_ids: Vec<String>,
    pub deduped_ids: Vec<String>,
    pub purged_ids: Vec<String>,
}

pub struct PrunePolicy {
    /// Hard token ceiling for the assembled conversation window.
    pub max_context_tokens: usize,
    /// Newest N messages are untouchable.
    pub protect_last_n: usize,
    /// Tool outputs older than this many turns and larger than
    /// `purge_tool_tokens` get replaced by a stub.
    pub purge_after_turns: usize,
    pub purge_tool_tokens: usize,
}

impl Default for PrunePolicy {
    fn default() -> Self {
        // Tuned for a small resident window on low-RAM machines.
        Self { max_context_tokens: 8_000, protect_last_n: 4, purge_after_turns: 4, purge_tool_tokens: 300 }
    }
}

fn stub(text: &str, head_chars: usize) -> String {
    let total = text.chars().count();
    if total <= head_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(head_chars).collect();
    format!("{head}\n[... husk: {} chars purged, full output in husk.db]", total - head_chars)
}

/// Prune a conversation window. Order matters:
/// 1. tool-output dedup (same tool + args → keep newest only),
/// 2. old large tool outputs → stubs,
/// 3. token window cap (oldest dropped first, newest N protected,
///    summaries never dropped).
pub fn prune(messages: &[ContextMessage], policy: &PrunePolicy) -> PrunedContext {
    let tokens_before: u64 = messages.iter().map(|m| estimate_tokens(&m.content) as u64).sum();
    let mut out: Vec<Option<ContextMessage>> = messages.iter().cloned().map(Some).collect();
    let mut result = PrunedContext { tokens_before, ..Default::default() };

    // --- 1. dedup repeated tool calls (keep newest) ---
    // Walk newest→oldest: insert() returns the previous occupant — Some
    // means an older duplicate just met a newer winner.
    let mut seen: HashMap<(String, u64), ()> = HashMap::new();
    for (idx, msg) in messages.iter().enumerate().rev() {
        if let MsgKind::ToolOutput { tool, args_hash } = &msg.kind {
            let key = (tool.clone(), *args_hash);
            if seen.insert(key, ()).is_some() {
                result.deduped_ids.push(msg.id.clone());
                out[idx] = Some(ContextMessage {
                    id: msg.id.clone(),
                    role: msg.role,
                    content: format!("[husk: superseded by later identical `{tool}` call; full output in husk.db]"),
                    kind: MsgKind::ToolOutput { tool: tool.clone(), args_hash: *args_hash },
                });
            }
        }
    }

    // --- 2. purge old, large tool outputs ---
    let n = messages.len();
    for (idx, msg) in messages.iter().enumerate() {
        if let MsgKind::ToolOutput { .. } = msg.kind {
            let age_from_end = n - 1 - idx;
            if age_from_end >= policy.purge_after_turns
                && estimate_tokens(&msg.content) > policy.purge_tool_tokens
            {
                result.purged_ids.push(msg.id.clone());
                let slot = out.get_mut(idx).unwrap();
                let m = slot.take().unwrap();
                *slot = Some(ContextMessage { content: stub(&m.content, 400), ..m });
            }
        }
    }

    // --- 3. token window cap ---
    let mut kept: Vec<ContextMessage> = Vec::new();
    let mut running = 0u64;
    let protected_from = n.saturating_sub(policy.protect_last_n);
    for (idx, slot) in out.iter().enumerate().rev() {
        let msg = match slot {
            Some(m) => m,
            None => continue, // already replaced; still count below
        };
        let tk = estimate_tokens(&msg.content) as u64;
        let is_protected = idx >= protected_from || msg.kind == MsgKind::Summary;
        if running + tk <= policy.max_context_tokens as u64 || is_protected {
            running += tk;
            kept.push(msg.clone());
        } else {
            result.dropped_ids.push(msg.id.clone());
        }
    }
    kept.reverse();
    result.kept = kept;
    result.tokens_after = running;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, role: Role, content: &str, kind: MsgKind) -> ContextMessage {
        ContextMessage { id: id.to_string(), role, content: content.to_string(), kind }
    }

    #[test]
    fn stable_blocks_precede_dynamic_and_prefix_is_cache_stable() {
        let day1 = vec![
            Block { kind: BlockKind::Stable, text: "AGENTS.md rules v1".into() },
            Block { kind: BlockKind::Dynamic, text: "date: 2026-09-14".into() },
        ];
        let day2 = vec![
            Block { kind: BlockKind::Stable, text: "AGENTS.md rules v1".into() },
            Block { kind: BlockKind::Dynamic, text: "date: 2026-09-15".into() },
        ];
        let a = assemble_system(&day1);
        let b = assemble_system(&day2);
        assert!(a.starts_with("AGENTS.md rules v1"));
        assert!(a.ends_with("date: 2026-09-14"));
        // Shared stable prefix: both start with the exact same bytes.
        assert!(b.starts_with(&a[..a.len() - "date: 2026-09-14".len()]));
        assert!(a.find("date").unwrap() > a.find("rules").unwrap());
    }

    #[test]
    fn window_cap_drops_oldest_first_and_protects_recent() {
        let policy = PrunePolicy { max_context_tokens: 300, protect_last_n: 2, ..Default::default() };
        let mut messages = Vec::new();
        for i in 0..20 {
            messages.push(msg(&format!("m{i}"), Role::User, &format!("content {}", "y".repeat(60)), MsgKind::Normal));
        }
        let p = prune(&messages, &policy);
        assert!(p.tokens_after <= 300 + 3 * 20, "after={}", p.tokens_after);
        assert!(!p.dropped_ids.is_empty());
        // Newest two are always kept.
        let kept_ids: Vec<&str> = p.kept.iter().map(|m| m.id.as_str()).collect();
        assert!(kept_ids.contains(&"m19"));
        assert!(kept_ids.contains(&"m18"));
        assert!(!kept_ids.contains(&"m0"));
        assert!(p.tokens_before > p.tokens_after);
    }

    #[test]
    fn repeated_tool_calls_are_deduped_keeping_newest() {
        let policy = PrunePolicy { max_context_tokens: 10_000, ..Default::default() };
        let package_json = MsgKind::ToolOutput { tool: "read".into(), args_hash: 0xABCD };
        let messages = vec![
            msg("a1", Role::Tool, "{\"name\":\"old\"}", package_json.clone()),
            msg("a2", Role::User, "hmm what changed?", MsgKind::Normal),
            msg("a3", Role::Tool, "{\"name\":\"new\"}", package_json.clone()),
        ];
        let p = prune(&messages, &policy);
        let a1 = p.kept.iter().find(|m| m.id == "a1").unwrap();
        assert!(a1.content.contains("superseded by later identical `read`"), "{}", a1.content);
        assert!(p.deduped_ids.contains(&"a1".to_string()));
        let a3 = p.kept.iter().find(|m| m.id == "a3").unwrap();
        assert_eq!(a3.content, "{\"name\":\"new\"}");
    }

    #[test]
    fn summaries_are_never_dropped_even_over_budget() {
        let policy = PrunePolicy { max_context_tokens: 50, protect_last_n: 0, ..Default::default() };
        let summary = msg("sum", Role::System, "session summary: user refactored auth module", MsgKind::Summary);
        let mut messages = vec![summary];
        for i in 0..10 {
            messages.push(msg(&format!("m{i}"), Role::User, &"z".repeat(200), MsgKind::Normal));
        }
        let p = prune(&messages, &policy);
        assert!(p.kept.iter().any(|m| m.id == "sum"), "summary must survive");
    }

    #[test]
    fn old_large_tool_outputs_are_purged_to_stubs() {
        let policy = PrunePolicy { protect_last_n: 2, purge_after_turns: 4, purge_tool_tokens: 100, ..Default::default() };
        let big = "log line | ".repeat(200); // ~2000 chars ≈ 500 tokens
        let kind = MsgKind::ToolOutput { tool: "bash".into(), args_hash: 1 };
        let messages = vec![
            msg("old_big", Role::Tool, &big, kind.clone()),
            msg("u1", Role::User, &"q".repeat(30), MsgKind::Normal),
            msg("u2", Role::User, &"q".repeat(30), MsgKind::Normal),
            msg("u3", Role::User, &"q".repeat(30), MsgKind::Normal),
            msg("u4", Role::User, &"q".repeat(30), MsgKind::Normal),
            msg("u5", Role::User, &"q".repeat(30), MsgKind::Normal),
            msg("recent_tool", Role::Tool, &big, kind.clone()),
        ];
        let p = prune(&messages, &policy);
        let old = p.kept.iter().find(|m| m.id == "old_big").unwrap();
        assert!(old.content.contains("full output in husk.db"), "{}", old.content.len());
        assert!(old.content.len() < 600);
        assert!(p.purged_ids.contains(&"old_big".to_string()));
        let recent = p.kept.iter().find(|m| m.id == "recent_tool").unwrap();
        assert!(!recent.content.contains("husk.db"), "recent tool output must stay intact");
    }
}
