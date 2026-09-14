//! llama.cpp-backed local engine (MiniCPM5-2B GGUF). Compiled only with
//! `--features local` (optionally `vulkan` for Intel iGPU offload).
//!
//! Design constraints from the deployment target (8GB laptop that already
//! runs browser + VSCode + Docker):
//! - LAZY: the model loads on the first worker request, never at startup.
//! - Dedicated inference thread: `LlamaBackend` is !Send, so backend+model
//!   live on one thread and requests flow through channels.
//! - mmap loading (llama.cpp default): weights are file-backed and evictable.
//! - greedy sampling: deterministic folds for the verification gate.
//! - hard `memory_cap_mb` pre-flight: refuse to load rather than OOM the
//!   user's machine; KV budget is estimated before startup.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use anyhow::{anyhow, Context as _, Result};

use super::{LocalConfig, WorkerRequest, WorkerResponse};

/// MiniCPM5-2B KV-cache geometry (verified model card: 42 layers, GQA 2 KV
/// heads, head dim 128): bytes per token in f16 = 2(K+V) * layers * kv_heads
/// * head_dim * 2B.
const KV_BYTES_PER_TOKEN: u64 = 2 * 42 * 2 * 128 * 2;

struct Loaded {
    backend: llama_cpp_2::llama_backend::LlamaBackend,
    model: llama_cpp_2::model::LlamaModel,
    ctx_tokens: u32,
    threads: i32,
}

impl Loaded {
    fn init(config: &LocalConfig, model_path: &Path) -> Result<Self> {
        let model_size = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
        let kv_total = KV_BYTES_PER_TOKEN * config.ctx_tokens as u64;
        let cap = config.memory_cap_mb as u64 * 1_000_000;
        // Weights are mmap'd (page-cache, evictable) — count half of them as
        // resident-hot in the pre-flight estimate.
        let estimate = model_size / 2 + kv_total;
        if config.memory_cap_mb > 0 && estimate > cap {
            return Err(anyhow!(
                "refusing to load model: pre-flight estimate {} MB exceeds memory_cap_mb {} (lower local.ctx_tokens or raise local.memory_cap_mb)",
                estimate / 1_000_000,
                config.memory_cap_mb
            ));
        }

        let backend = llama_cpp_2::llama_backend::LlamaBackend::init()
            .map_err(|e| anyhow!("llama.cpp backend init failed: {e}"))?;

        let model_params = llama_cpp_2::model::params::LlamaModelParams::default()
            .with_n_gpu_layers(config.gpu_layers);
        let model = llama_cpp_2::model::LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| anyhow!("failed to load GGUF at {}: {e}", model_path.display()))?;

        let threads: usize = config.threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2)
                .min(4)
        });

        Ok(Self { backend, model, ctx_tokens: config.ctx_tokens, threads: threads as i32 })
    }

    fn run(&mut self, req: &WorkerRequest) -> Result<WorkerResponse> {
        use llama_cpp_2::context::params::LlamaContextParams;
        use llama_cpp_2::llama_batch::LlamaBatch;
        use llama_cpp_2::model::{AddBos, LlamaChatMessage};
        use llama_cpp_2::sampling::LlamaSampler;

        let prompt = match self.model.chat_template(None) {
            Ok(tmpl) => {
                let chat = vec![
                    LlamaChatMessage::new("system".to_string(), req.task.clone())
                        .map_err(|e| anyhow!("chat message: {e}"))?,
                    LlamaChatMessage::new("user".to_string(), req.input.clone())
                        .map_err(|e| anyhow!("chat message: {e}"))?,
                ];
                self.model
                    .apply_chat_template(&tmpl, &chat, true)
                    .map_err(|e| anyhow!("chat template: {e}"))?
            }
            Err(_) => format!("{}\n\n{}", req.task, req.input),
        };

        let tokens = self
            .model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| anyhow!("tokenize failed: {e}"))?;
        if tokens.is_empty() {
            return Err(anyhow!("empty prompt tokenization"));
        }

        let ctx_tokens = NonZeroU32::new(self.ctx_tokens)
            .ok_or_else(|| anyhow!("ctx_tokens must be > 0"))?;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(ctx_tokens))
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow!("context create failed: {e}"))?;

        let mut sampler = LlamaSampler::greedy();
        let mut batch = LlamaBatch::new(tokens.len().max(64), 1);

        let mut input_count = 0u64;
        let last = tokens.len() - 1;
        for (pos, token) in tokens.iter().enumerate() {
            batch.add(*token, pos as i32, &[0], pos == last)?;
            input_count += 1;
        }
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("prefill decode failed: {e}"))?;

        let mut output = String::new();
        let mut output_tokens = 0u64;
        let mut n_cur = tokens.len() as i32;
        let max_new = req.max_tokens.clamp(16, 2048);

        loop {
            let token = sampler.sample(&mut ctx, batch.n_tokens() - 1);
            if self.model.is_eog_token(token) || output_tokens >= max_new as u64 {
                break;
            }
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let piece = self
                .model
                .token_to_piece(token, &mut decoder, false, None)
                .map_err(|e| anyhow!("detokenize failed: {e}"))?;
            output.push_str(&piece);
            output_tokens += 1;

            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            ctx.decode(&mut batch)
                .map_err(|e| anyhow!("decode failed: {e}"))?;
            n_cur += 1;
        }

        Ok(WorkerResponse {
            text: output,
            input_tokens: input_count,
            output_tokens,
        })
    }
}

enum Command {
    Run(Box<WorkerRequest>, Sender<Result<WorkerResponse>>),
}

/// Channel-fronted engine: `LocalEngine` methods are `Send + Sync`, the
/// inference state is not — it stays pinned to one OS thread.
pub struct LlamaEngine {
    tx: Sender<Command>,
    info: String,
}

impl LlamaEngine {
    /// Spawn the inference thread. Cheap: nothing loads until the first
    /// request (or until `warm_up` is called).
    pub fn spawn(config: LocalConfig, model_path: PathBuf) -> Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<Command>();
        let info = format!(
            "llama.cpp: {} (ctx {}, gpu_layers {}, cap {}MB)",
            model_path.display(),
            config.ctx_tokens,
            config.gpu_layers,
            config.memory_cap_mb
        );
        std::thread::Builder::new()
            .name("husk-llama".to_string())
            .spawn(move || {
                let mut loaded: Option<Loaded> = None;
                let mut init_error: Option<String> = None;
                for cmd in rx {
                    match cmd {
                        Command::Run(req, respond) => {
                            if loaded.is_none() && init_error.is_none() {
                                match Loaded::init(&config, &model_path) {
                                    Ok(l) => loaded = Some(l),
                                    Err(e) => init_error = Some(format!("{e:#}")),
                                }
                            }
                            let result = match (&mut loaded, &init_error) {
                                (_, Some(err)) => Err(anyhow!("{err}")),
                                (Some(l), None) => l.run(&req),
                                (None, None) => Err(anyhow!("engine not initialized")),
                            };
                            let _ = respond.send(result);
                        }
                    }
                }
            })
            .context("failed to spawn husk-llama thread")?;
        Ok(Self { tx, info })
    }

    /// Force model load now (used by `husk local warm`).
    pub fn warm_up_blocking(&self) -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Run(Box::new(WorkerRequest { task: "ping".into(), input: String::new(), max_tokens: 1 }), tx))
            .map_err(|_| anyhow!("inference thread died"))?;
        rx.recv().map_err(|_| anyhow!("inference thread died"))?.map(|_| ())
    }
}

#[async_trait::async_trait]
impl super::LocalEngine for LlamaEngine {
    async fn complete(&self, req: &WorkerRequest) -> Result<WorkerResponse> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Run(Box::new(req.clone()), tx))
            .map_err(|_| anyhow!("inference thread died"))?;
        // Blocking recv must not stall the async runtime: shift it to the
        // blocking pool.
        let response =
            tokio::task::spawn_blocking(move || rx.recv()).await.map_err(|e| anyhow!("{e}"))?;
        response.map_err(|_| anyhow!("inference thread died"))?
    }

    fn describe(&self) -> String {
        self.info.clone()
    }

    fn warm_up(&self) -> Result<()> {
        self.warm_up_blocking()
    }
}
