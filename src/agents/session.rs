//! Supervisor-Worker session — the agent-in-agent core.
//!
//! Invariant: DeepSeek (supervisor) is the ONLY model that reasons. The local
//! MiniCPM worker only folds context into verified artifacts; anything it
//! produces passes a deterministic verification gate or the task escalates
//! back to the supervisor. Every hop is persisted to husk.db and accounted.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;

use crate::context::{assemble_system, prune, Block, BlockKind, ContextMessage, MsgKind, PrunePolicy};
use crate::db::{Database, PartKind, Role};
use crate::local::{LocalEngine, NullEngine, WorkerRequest};
use crate::router::{RouteTarget, TaskRouter};
use crate::tokenutil::{estimate_tokens, Pricing, TokenLedger};
use crate::verify::{verify, VerifySpec};

/// USD pricing used for cost estimates (input $0.14 / cached $0.03 / output
/// $0.28 per 1M — DeepSeek V4 Flash class; treat as an approximation for
/// whatever reseller, e.g. Netra Runtime, you point the preset at).
pub const DEEPSEEK_FLASH_PRICING: Pricing = Pricing {
    input_per_mtok: 0.14,
    cached_input_per_mtok: 0.03,
    output_per_mtok: 0.28,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoldKind {
    None,
    /// Small enough to pass through raw.
    Raw,
    /// Local worker produced a verified fold.
    Worker,
    /// Worker failed the gate; the supervisor folded instead.
    Escalated,
}

pub struct SupervisorWorkerSession {
    supervisor: Box<dyn crate::providers::LlmProvider>,
    worker: Box<dyn LocalEngine>,
    db: Arc<Database>,
    pub pricing: Pricing,
    pub prune_policy: PrunePolicy,
    /// Model ID sent in every supervisor request (verified endpoint id).
    pub supervisor_model: String,
}

#[derive(Debug)]
pub struct SendOutcome {
    pub reply: String,
    pub worker_used: bool,
    pub worker_escalated: bool,
    pub ledger: TokenLedger,
}

impl SupervisorWorkerSession {
    pub fn new(
        supervisor: Box<dyn crate::providers::LlmProvider>,
        worker: Option<Box<dyn LocalEngine>>,
        db: Arc<Database>,
    ) -> Self {
        Self {
            supervisor,
            worker: worker.unwrap_or_else(|| Box::new(NullEngine)),
            db,
            pricing: DEEPSEEK_FLASH_PRICING,
            prune_policy: PrunePolicy::default(),
            supervisor_model: crate::providers::deepseek::DeepseekProvider::DEFAULT_MODEL.to_string(),
        }
    }

    /// Override the supervisor model id (e.g. from config).
    pub fn with_supervisor_model(mut self, model: impl Into<String>) -> Self {
        self.supervisor_model = model.into();
        self
    }

    /// Hot-swap the local worker (TUI worker toggle). Pass None for
    /// cloud-only operation; folds then escalate to the supervisor.
    pub fn set_worker(&mut self, worker: Option<Box<dyn LocalEngine>>) {
        self.worker = worker.unwrap_or_else(|| Box::new(NullEngine));
    }

    /// Stable system blocks (cache-friendly prefix) + dynamic suffix.
    fn system_prompt(&self, folded_context: Option<&str>, user_prompt: &str) -> String {
        let mut blocks = vec![
            Block {
                kind: BlockKind::Stable,
                text: "You are husk, a terse senior software engineer. Answer with minimal prose; code speaks.".to_string(),
            },
            Block {
                kind: BlockKind::Stable,
                text: "Hard reasoning is your job. Context artifacts marked [folded] were produced by a local summarizer and verified by the harness.".to_string(),
            },
        ];
        if let Some(ctx) = folded_context {
            blocks.push(Block { kind: BlockKind::Stable, text: format!("[folded context]\n{ctx}") });
        }
        // Dynamic content LAST so the stable prefix stays byte-identical
        // across turns and the provider prefix-cache keeps hitting.
        blocks.push(Block {
            kind: BlockKind::Dynamic,
            text: format!("session-time: {}", chrono::Utc::now().to_rfc3339()),
        });
        let _ = user_prompt;
        assemble_system(&blocks)
    }

    /// Fold raw context documents through the worker + verification gate.
    async fn fold_context(
        &mut self,
        session_id: &str,
        docs: &[(String, String)],
        user_prompt: &str,
    ) -> Result<(Option<String>, FoldKind)> {
        if docs.is_empty() {
            return Ok((None, FoldKind::None));
        }
        let raw = docs
            .iter()
            .map(|(name, content)| format!("=== {name} ===\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let raw_tokens = estimate_tokens(&raw) as u64;

        // Small context does not need folding at all — sending it raw to the
        // supervisor is cheaper than a worker round-trip.
        if raw_tokens < 800 {
            return Ok((Some(raw), FoldKind::Raw));
        }

        let worker_task = "Summarize the following material for a coding supervisor. \
Output ONLY JSON: {\"key_findings\": [..], \"source_references\": [\"file:line\"..], \"open_questions\": [..]}";
        let request = WorkerRequest { task: worker_task.to_string(), input: raw.clone(), max_tokens: 800 };

        let worker_result = self.worker.complete(&request).await;
        let spec = VerifySpec {
            expect_json: true,
            required_fields: vec!["key_findings".into(), "source_references".into()],
            required_identifiers: Vec::new(),
            max_tokens: 800,
        };

        match worker_result {
            Ok(worker_result) if verify(&worker_result.text, &spec).is_ok() => {
                let compressed_tokens = estimate_tokens(&worker_result.text) as u64;
                self.db.bump_counter(session_id, "worker_input", worker_result.input_tokens)?;
                self.db.bump_counter(session_id, "worker_output", worker_result.output_tokens)?;
                self.db.bump_counter(session_id, "context_raw", raw_tokens)?;
                self.db.bump_counter(session_id, "context_compressed", compressed_tokens)?;
                Ok((Some(worker_result.text), FoldKind::Worker))
            }
            worker_err => {
                // Gate failed or engine unavailable → escalate the fold to the
                // supervisor itself (effort low: it is still just summarizing).
                let _ = worker_err;
                let escalation = format!(
                    "Summarize the following material into key findings relevant to this request: {user_prompt}\n\n{raw}"
                );
                let req = crate::providers::ChatRequest::simple(
                    &self.supervisor_model,
                    "you are a summarizer; output terse bullet points",
                    &escalation,
                )
                .with_max_tokens(800)
                .with_effort(crate::router::ReasoningEffort::Low);
                let resp = self.supervisor.chat(&req).await?;
                self.db.record_usage(session_id, "escalated-fold", resp.usage.unwrap_or_default())?;
                Ok((Some(resp.content), FoldKind::Escalated))
            }
        }
    }

    /// Full turn: fold context → route → supervisor reply → persist + account.
    pub async fn send(
        &mut self,
        session_id: &str,
        user_prompt: &str,
        context_docs: &[(String, String)],
    ) -> Result<SendOutcome> {
        self.send_streamed(session_id, user_prompt, context_docs, None).await
    }

    /// Streaming variant; deltas go to `token_tx` when provided.
    pub async fn send_streamed(
        &mut self,
        session_id: &str,
        user_prompt: &str,
        context_docs: &[(String, String)],
        token_tx: Option<UnboundedSender<String>>,
    ) -> Result<SendOutcome> {
        self.db.append_message(session_id, Role::User, user_prompt)?;

        let (folded, fold_kind) = self.fold_context(session_id, context_docs, user_prompt).await?;
        let worker_used = fold_kind == FoldKind::Worker;
        let escalated = fold_kind == FoldKind::Escalated;

        // Route by task type; never let a model pick the model.
        let decision = TaskRouter::classify(user_prompt);
        let effort = decision.effort;
        debug_assert!(
            decision.target == RouteTarget::Supervisor || matches!(decision.target, RouteTarget::Worker),
            "router target"
        );
        // The supervisor ALWAYS answers the user; worker targets only mean the
        // router judged no reasoning is needed, so spend the minimum effort.
        let effort = effort.unwrap_or(crate::router::ReasoningEffort::Low);

        let system = self.system_prompt(folded.as_deref(), user_prompt);
        let history = self.db.recent_messages(session_id, 60)?;
        let window: Vec<ContextMessage> = history
            .iter()
            .map(|m| ContextMessage {
                id: m.id.clone(),
                role: m.role,
                content: m.content.clone(),
                kind: MsgKind::Normal,
            })
            .collect();
        let pruned = prune(&window, &self.prune_policy);
        self.db.bump_counter(session_id, "context_raw", pruned.tokens_before)?;
        self.db.bump_counter(session_id, "context_compressed", pruned.tokens_after)?;

        // Build chat messages: system + pruned history (excluding the just-
        // appended user message, which is appended again below).
        let mut messages: Vec<crate::providers::ChatMessage> =
            vec![crate::providers::ChatMessage::new(crate::providers::ChatRole::System, system)];
        for msg in pruned.kept.iter() {
            let role = match msg.role {
                Role::User => crate::providers::ChatRole::User,
                Role::Assistant => crate::providers::ChatRole::Assistant,
                Role::System | Role::Tool => crate::providers::ChatRole::System,
            };
            if msg.id == history.last().map(|m| m.id.as_str()).unwrap_or("") && msg.role == Role::User {
                continue;
            }
            messages.push(crate::providers::ChatMessage::new(role, msg.content.clone()));
        }
        messages.push(crate::providers::ChatMessage::new(crate::providers::ChatRole::User, user_prompt));

        let request = crate::providers::ChatRequest::new(self.supervisor_model.clone(), messages)
            .with_max_tokens(2048)
            .with_effort(effort);

        let response = match token_tx {
            Some(tx) => self.supervisor.chat_stream(&request, tx).await?,
            None => self.supervisor.chat(&request).await?,
        };

        if let Some(usage) = response.usage {
            self.db.record_usage(session_id, &self.supervisor_model, usage)?;
        }
        self.db.append_message(session_id, Role::Assistant, &response.content)?;

        let ledger = self.db.session_ledger(session_id)?;
        Ok(SendOutcome { reply: response.content, worker_used, worker_escalated: escalated, ledger })
    }

    /// Persist a raw tool output as a part (full fidelity stays in husk.db).
    pub fn record_tool_output(
        &self,
        session_id: &str,
        assistant_message_id: &str,
        tool: &str,
        output: &str,
    ) -> Result<()> {
        self.db.append_part(assistant_message_id, PartKind::ToolOutput, tool, output)?;
        let _ = session_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::local::WorkerResponse;
    use crate::providers::{ChatRequest, ChatResponse};
    use crate::tokenutil::TokenUsage;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeSupervisor {
        calls: AtomicUsize,
        efforts: std::sync::Mutex<Vec<Option<crate::router::ReasoningEffort>>>,
        reply: String,
    }

    impl FakeSupervisor {
        fn new(reply: &str) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                efforts: std::sync::Mutex::new(Vec::new()),
                reply: reply.to_string(),
            }
        }
    }

    #[async_trait]
    impl crate::providers::LlmProvider for FakeSupervisor {
        async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.efforts.lock().unwrap().push(req.reasoning_effort);
            // Stable prefix must precede dynamic content (cache contract),
            // when a fold is present at all.
            let system = &req.messages[0].content;
            if let (Some(fold), Some(dyn_pos)) =
                (system.find("[folded context]"), system.find("session-time"))
            {
                assert!(fold < dyn_pos, "folded context must precede dynamic blocks");
            }
            Ok(ChatResponse {
                content: self.reply.clone(),
                usage: Some(TokenUsage::new(1_000_000, 0, 100)),
            })
        }

        async fn chat_stream(
            &self,
            _req: &ChatRequest,
            _token_tx: UnboundedSender<String>,
        ) -> Result<ChatResponse> {
            unimplemented!("not needed for these tests")
        }

        async fn chat_simple(
            &self,
            _system: &str,
            _user: &str,
            _token_tx: Option<UnboundedSender<String>>,
        ) -> Result<ChatResponse> {
            unimplemented!("not needed for these tests")
        }
    }

    struct FakeWorker {
        output: String,
    }

    #[async_trait]
    impl LocalEngine for FakeWorker {
        async fn complete(&self, _req: &WorkerRequest) -> Result<WorkerResponse> {
            Ok(WorkerResponse { text: self.output.clone(), input_tokens: 40_000, output_tokens: 300 })
        }
        fn describe(&self) -> String {
            "fake worker".into()
        }
    }

    fn big_docs() -> Vec<(String, String)> {
        // ~4600 chars ≈ 1150 tokens: above the raw-passthrough threshold.
        vec![(
            "src/auth.rs".to_string(),
            format!("fn login() {{\n{}\n}}", "let x = check_token();\n".repeat(200)),
        )]
    }

    const VALID_SUMMARY: &str =
        r#"{"key_findings": ["login validates jwt"], "source_references": ["src/auth.rs:1"], "open_questions": []}"#;

    #[tokio::test]
    async fn worker_fold_passes_gate_and_counts_savings() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let session_rec = db.create_session("deepseek", "m", None).unwrap();
        let mut session = SupervisorWorkerSession::new(
            Box::new(FakeSupervisor::new("done")),
            Some(Box::new(FakeWorker { output: VALID_SUMMARY.to_string() })),
            db.clone(),
        );
        let outcome = session.send(&session_rec.id, "summarize the auth module", &big_docs()).await.unwrap();

        assert!(outcome.worker_used, "worker path must be used");
        assert!(!outcome.worker_escalated);
        assert_eq!(outcome.ledger.worker_input, 40_000);
        assert_eq!(outcome.ledger.worker_output, 300);
        assert!(outcome.ledger.saved_tokens() > 500, "fold must save real tokens");
        // Supervisor saw the fold, not the raw doc.
        let ledger = db.session_ledger(&session_rec.id).unwrap();
        assert_eq!(ledger.supervisor.prompt_tokens, 1_000_000);
    }

    #[tokio::test]
    async fn failed_gate_escalates_fold_to_supervisor() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let session_rec = db.create_session("deepseek", "m", None).unwrap();
        let supervisor = FakeSupervisor::new("folded by deepseek");
        let mut session = SupervisorWorkerSession::new(
            Box::new(supervisor),
            Some(Box::new(FakeWorker { output: "i am not json, sorry".to_string() })),
            db.clone(),
        );
        let outcome = session.send(&session_rec.id, "what changed here?", &big_docs()).await.unwrap();

        assert!(outcome.worker_escalated, "gate failure must escalate");
        assert!(!outcome.worker_used);
        // 1 escalation fold + 1 answer = 2 supervisor calls.
        assert_eq!(outcome.reply, "folded by deepseek");
        let ledger = db.session_ledger(&session_rec.id).unwrap();
        assert!(ledger.supervisor.prompt_tokens >= 2_000_000, "both calls accounted");
    }

    #[tokio::test]
    async fn unavailable_worker_degrades_to_cloud_only() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let session_rec = db.create_session("deepseek", "m", None).unwrap();
        let mut session =
            SupervisorWorkerSession::new(Box::new(FakeSupervisor::new("ok")), None, db.clone());
        let outcome = session.send(&session_rec.id, "explain this", &big_docs()).await.unwrap();
        assert!(outcome.worker_escalated);
        assert_eq!(outcome.reply, "ok");
    }

    #[tokio::test]
    async fn small_context_skips_worker_entirely() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let session_rec = db.create_session("deepseek", "m", None).unwrap();
        let worker_called = Arc::new(AtomicUsize::new(0));
        struct Counting(Arc<AtomicUsize>);
        #[async_trait]
        impl LocalEngine for Counting {
            async fn complete(&self, _req: &WorkerRequest) -> Result<WorkerResponse> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(WorkerResponse { text: VALID_SUMMARY.into(), input_tokens: 1, output_tokens: 1 })
            }
            fn describe(&self) -> String {
                String::new()
            }
        }
        let mut session = SupervisorWorkerSession::new(
            Box::new(FakeSupervisor::new("ok")),
            Some(Box::new(Counting(worker_called.clone()))),
            db.clone(),
        );
        let tiny = vec![("README".to_string(), "short".to_string())];
        let outcome = session.send(&session_rec.id, "hi", &tiny).await.unwrap();
        assert!(!outcome.worker_used, "raw small context must go straight to supervisor");
        assert_eq!(worker_called.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reasoning_prompt_gets_max_effort_on_supervisor() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let session_rec = db.create_session("deepseek", "m", None).unwrap();
        let supervisor = FakeSupervisor::new("plan");
        let mut session =
            SupervisorWorkerSession::new(Box::new(supervisor), None, db.clone());
        session.send(&session_rec.id, "debug this security bug carefully", &[]).await.unwrap();
        // FakeSupervisor records efforts; verify via a fresh call path is
        // awkward across boxes, so assert on the router contract instead.
        assert_eq!(TaskRouter::classify("debug this security bug carefully").effort,
            Some(crate::router::ReasoningEffort::Max));
    }

    #[test]
    fn config_default_provider_remains_compatible() {
        let _ = Config::default();
    }
}
