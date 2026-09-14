//! Local embedded worker engine (MiniCPM5-2B) — trait, config, and the
//! always-available NullEngine.
//!
//! Memory discipline for an already-busy 8GB laptop (browser + VSCode +
//! Docker): the engine is LAZY and OPT-IN. Nothing loads until the first
//! worker task with `local.enabled = true`; the llama.cpp implementation
//! (behind the `local` cargo feature) maps the GGUF read-only so pages are
//! file-backed and evictable under memory pressure, and enforces
//! `memory_cap_mb`. When the engine is unavailable, every worker task
//! escalates to the supervisor — the CLI stays fully functional cloud-only.

use anyhow::{anyhow, Result};
use async_trait::async_trait;

pub mod embedded;

#[cfg(feature = "local")]
pub mod llama;

#[cfg(feature = "embed-model")]
mod embedded_payload {
    include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));
}

/// Build the local worker engine when the build/config allows it.
/// Returns None when the worker is disabled or unavailable — the session
/// then escalates every fold to the supervisor (cloud-only mode).
pub fn create_local_engine(config: &LocalConfig) -> Option<Box<dyn LocalEngine>> {
    if !config.enabled {
        return None;
    }
    #[cfg(feature = "local")]
    {
        let path = match embedded::ensure_model_available(config) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("husk: local worker disabled: {e:#}");
                return None;
            }
        };
        match llama::LlamaEngine::spawn(config.clone(), path) {
            Ok(engine) => Some(Box::new(engine)),
            Err(e) => {
                eprintln!("husk: local worker disabled: {e:#}");
                None
            }
        }
    }
    #[cfg(not(feature = "local"))]
    {
        let _ = config;
        None
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalConfig {
    /// Master switch. Default false: cloud-only, zero model RAM.
    #[serde(default)]
    pub enabled: bool,
    /// Path to the MiniCPM5-2B GGUF. When None, the embedded payload (if the
    /// binary was built with HUSK_EMBED_MODEL) is extracted to the cache dir.
    #[serde(default)]
    pub model_path: Option<String>,
    /// Context window for the local model. 4096 keeps the f16 KV cache
    /// (~170MB for MiniCPM5-2B's 42-layer GQA) well inside a ~2GB budget;
    /// raise it only if the device has headroom.
    #[serde(default = "default_ctx")]
    pub ctx_tokens: u32,
    /// GPU layers to offload to the Intel iGPU (Vulkan build). 0 = CPU only,
    /// 99 = max offload. Conservative default: CPU-first, opt into offload.
    #[serde(default)]
    pub gpu_layers: u32,
    #[serde(default)]
    pub threads: Option<usize>,
    /// Hard ceiling (MB) the local engine may occupy; the loader refuses to
    /// start when it cannot stay inside.
    #[serde(default = "default_memory_cap")]
    pub memory_cap_mb: u32,
}

fn default_ctx() -> u32 {
    4096
}
fn default_memory_cap() -> u32 {
    1792
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: None,
            ctx_tokens: default_ctx(),
            gpu_layers: 0,
            threads: None,
            memory_cap_mb: default_memory_cap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerRequest {
    /// Instruction for the worker, e.g. "summarize into key findings JSON".
    pub task: String,
    /// Raw context material (file contents, diffs, tool output).
    pub input: String,
    /// Output token budget (verified by the gate afterwards).
    pub max_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct WorkerResponse {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[async_trait]
pub trait LocalEngine: Send + Sync {
    /// Run one worker completion. Implementations must be lazy: the model
    /// loads on the first call, not on construction.
    async fn complete(&self, req: &WorkerRequest) -> Result<WorkerResponse>;
    /// Human-readable device/backend info for the TUI status bar.
    fn describe(&self) -> String;
    /// Force model load now (used by `husk local warm`). Default: unavailable.
    fn warm_up(&self) -> Result<()> {
        Err(anyhow!("warm-up not supported by this engine"))
    }
}

/// Placeholder engine: local inference is unavailable (feature off or
/// disabled). Every request fails so the session layer escalates cleanly.
pub struct NullEngine;

#[async_trait]
impl LocalEngine for NullEngine {
    async fn complete(&self, _req: &WorkerRequest) -> Result<WorkerResponse> {
        Err(anyhow!("local model unavailable (built without --features local or disabled in config)"))
    }

    fn describe(&self) -> String {
        "local: off (cloud-only)".to_string()
    }

    fn warm_up(&self) -> Result<()> {
        Err(anyhow!("local model unavailable"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_engine_always_escalates() {
        let engine = NullEngine;
        let req = WorkerRequest { task: "summarize".into(), input: "data".into(), max_tokens: 100 };
        assert!(engine.complete(&req).await.is_err());
        assert!(engine.describe().contains("cloud-only"));
    }

    #[test]
    fn local_config_defaults_are_memory_conservative() {
        let cfg = LocalConfig::default();
        assert!(!cfg.enabled, "local model must be opt-in");
        assert_eq!(cfg.ctx_tokens, 4096);
        assert!(cfg.memory_cap_mb <= 2048, "must fit a ~2GB shared-memory budget");
        assert_eq!(cfg.gpu_layers, 0, "CPU-first default");
    }

    #[test]
    fn local_config_parses_from_toml_snippet() {
        let cfg: LocalConfig = toml::from_str(
            r#"
            enabled = true
            ctx_tokens = 4096
            gpu_layers = 99
            memory_cap_mb = 1536
            "#,
        )
        .unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.ctx_tokens, 4096);
        assert_eq!(cfg.gpu_layers, 99);
        assert_eq!(cfg.memory_cap_mb, 1536);
        assert!(cfg.threads.is_none());
    }
}
