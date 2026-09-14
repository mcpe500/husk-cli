//! `husk local` — inspect / pre-load the embedded MiniCPM worker.

use anyhow::Result;
use colored::Colorize;
use crate::cli::LocalCommands;
use crate::config::Config;
use crate::local::{self, embedded};

pub fn run(config: &Config, action: LocalCommands) -> Result<()> {
    match action {
        LocalCommands::Info => {
            println!("{}", "▶ Local worker (MiniCPM5-2B)".bold().cyan());
            println!(
                "   {} {} (feature `local` compiled in: {})",
                "build:".dimmed(),
                if cfg!(feature = "local") { "llama.cpp backend" } else { "NOT compiled (use --features local)" },
                cfg!(feature = "local")
            );
            println!(
                "   {} {}",
                "vulkan:".dimmed(),
                if cfg!(feature = "vulkan") { "enabled (Intel iGPU offload available)" } else { "off" }
            );
            println!("   {}", config.local.describe_lines());
            match embedded::payload() {
                Some((chunks, name, sha)) => println!(
                    "   {} {} embedded in executable ({} chunk(s), sha {}…)",
                    "payload:".dimmed(),
                    name,
                    chunks.len(),
                    &sha[..sha.len().min(12)]
                ),
                None => println!("   {} none baked in (HUSK_EMBED_MODEL at build time)", "payload:".dimmed()),
            }
            match embedded::ensure_model_available(&config.local) {
                Ok(path) => println!("   {} {}", "model path:".dimmed(), path.display()),
                Err(e) => println!("   {} {e:#}", "unavailable:".yellow()),
            }
        }
        LocalCommands::Warm => {
            if !cfg!(feature = "local") {
                println!("{} build with --features local to enable the embedded worker", "error:".red().bold());
                return Ok(());
            }
            let mut cfg = config.local.clone();
            cfg.enabled = true;
            let Some(engine) = local::create_local_engine(&cfg) else {
                println!("{} worker unavailable (see local info)", "error:".red().bold());
                return Ok(());
            };
            println!("{}", "▶ Loading model (lazy → first use)...".bold().cyan());
            engine.warm_up()?;
            println!("{} {}", "✔ warm:".green().bold(), engine.describe());
        }
    }
    Ok(())
}

impl crate::local::LocalConfig {
    fn describe_lines(&self) -> String {
        format!(
            "enabled={} ctx_tokens={} gpu_layers={} threads={:?} memory_cap_mb={}",
            self.enabled, self.ctx_tokens, self.gpu_layers, self.threads, self.memory_cap_mb
        )
    }
}
