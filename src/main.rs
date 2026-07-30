mod agents;
mod cli;
mod config;
mod graph;
mod mcp;
mod plugins;
mod providers;
mod telemetry;
mod tui;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::path::PathBuf;

use agents::AgentOrchestrator;
use cli::{Cli, Commands, ConfigCommands, McpCommands, PluginCommands, SkillCommands};
use config::{Config, ProviderPreset};
use graph::{CodebaseGraph, ExecutionGraph};
use plugins::PluginManager;
use providers::create_provider;
use telemetry::TelemetryLogger;
use tui::run_tui;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load().unwrap_or_default();

    let command = match cli.command {
        Some(cmd) => cmd,
        None => Commands::Tui,
    };

    match command {
        Commands::Tui => {
            run_tui()?;
        }

        Commands::Index { path, force } => {
            println!("{}", "▶ Indexing Codebase into Graph...".bold().cyan());
            let target_path = if path == PathBuf::from(".") {
                std::env::current_dir()?
            } else {
                path
            };

            let out_file = Config::husk_dir().join("graph.json");
            if out_file.exists() && !force {
                println!(
                    "{}",
                    "ℹ Codebase graph already exists. Use --force to re-index."
                        .yellow()
                );
            }

            let mut cb_graph = CodebaseGraph::new();
            let count = cb_graph.index_directory(&target_path)?;
            cb_graph.save_to_file(&out_file)?;

            println!(
                "{} Indexed {} files/nodes into {:?}",
                "✔ Success:".green().bold(),
                count,
                out_file
            );
        }

        Commands::Run {
            prompt,
            provider,
            model,
            max_retries,
            graph_out,
            auto_approve: _,
        } => {
            println!("{}", "▶ Initializing Husk Graph Execution Engine...".bold().magenta());

            if let Some(p_str) = provider {
                match p_str.as_str() {
                    "zai" | "zai-anthropic" => config.apply_preset(ProviderPreset::ZaiAnthropic),
                    "zai-openai" | "glm-openai" => config.apply_preset(ProviderPreset::ZaiOpenAi),
                    "minimax" | "minimax-openai" => config.apply_preset(ProviderPreset::MiniMaxOpenAi),
                    "minimax-anthropic" => config.apply_preset(ProviderPreset::MiniMaxAnthropic),
                    "openai" => config.apply_preset(ProviderPreset::OpenAi),
                    "anthropic" => config.apply_preset(ProviderPreset::Anthropic),
                    _ => {}
                }
            }

            if let Some(m_str) = model {
                config.model = m_str;
            }

            println!(
                "   {} {:?} | Model: {} | Max Retries: {}",
                "Provider Preset:".dimmed(),
                config.provider,
                config.model.cyan(),
                max_retries
            );

            // Load Codebase Graph if indexed
            let graph_file = Config::husk_dir().join("graph.json");
            let subgraph_nodes = if graph_file.exists() {
                if let Ok(cb_graph) = CodebaseGraph::load_from_file(&graph_file) {
                    cb_graph.get_subgraph_nodes(&prompt)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            println!(
                "   {} Found {} context nodes",
                "Codebase Subgraph:".dimmed(),
                subgraph_nodes.len()
            );

            // Construct Execution DAG with Max Retries
            let mut exec_graph = ExecutionGraph::new_default_pipeline(&prompt, subgraph_nodes.clone(), max_retries);

            if let Some(out) = graph_out {
                let json = serde_json::to_string_pretty(&exec_graph)?;
                std::fs::write(&out, json)?;
                println!("   {} Saved graph DAG to {:?}", "Graph Export:".dimmed(), out);
            }

            // Create LLM Provider
            let provider_instance = create_provider(&config)?;
            let mut orchestrator = AgentOrchestrator::new(provider_instance);

            let success = orchestrator.execute_graph(&mut exec_graph).await?;

            if config.telemetry_enabled {
                let status_str = if success { "Success" } else { "Failed" };
                let _ = TelemetryLogger::log_run(
                    &prompt,
                    &format!("{:?}", config.provider),
                    &config.model,
                    subgraph_nodes.len(),
                    status_str,
                );
            }
        }

        Commands::Plugin { action } => match action {
            PluginCommands::List => {
                let mut pm = PluginManager::new();
                let _ = pm.discover_all();
                println!("{}", "▶ Installed Plugins:".bold().cyan());
                if pm.plugins.is_empty() {
                    println!("   No plugins installed in .husk/plugins/");
                } else {
                    for p in pm.plugins {
                        println!("   - {} (v{}): {}", p.name.bold(), p.version, p.description);
                    }
                }
            }
            PluginCommands::Install { path } => {
                println!("Installing plugin from: {}", path.cyan());
            }
            PluginCommands::Remove { name } => {
                println!("Removing plugin: {}", name.cyan());
            }
        },

        Commands::Skill { action } => match action {
            SkillCommands::List => {
                println!("{}", "▶ Available Skills:".bold().cyan());
                println!("   - a11y-debugging: Web accessibility auditing");
                println!("   - rust-best-practices: High performance idiomatic Rust guidelines");
            }
            SkillCommands::Show { name } => {
                println!("Skill: {}", name.bold().cyan());
            }
        },

        Commands::Mcp { action } => match action {
            McpCommands::List => {
                println!("{}", "▶ Configured MCP Servers:".bold().cyan());
                println!("   - filesystem: stdio local fs host");
            }
            McpCommands::Add { name, command, args } => {
                println!("Added MCP server: {} -> {} {:?}", name.cyan(), command, args);
            }
        },

        Commands::Config { action } => match action {
            ConfigCommands::Show => {
                println!("{}", "▶ Active Husk Configuration:".bold().cyan());
                println!("{}", toml::to_string_pretty(&config)?);
            }
            ConfigCommands::Set { key, value } => {
                match key.as_str() {
                    "api_key" => config.api_key = value,
                    "model" => config.model = value,
                    "base_url" => config.base_url = value,
                    _ => println!("Unknown config key: {}", key),
                }
                config.save()?;
                println!("✔ Configuration updated.");
            }
            ConfigCommands::Preset { name } => {
                match name.as_str() {
                    "zai" | "zai-anthropic" => config.apply_preset(ProviderPreset::ZaiAnthropic),
                    "zai-openai" | "glm-openai" => config.apply_preset(ProviderPreset::ZaiOpenAi),
                    "minimax" | "minimax-openai" => config.apply_preset(ProviderPreset::MiniMaxOpenAi),
                    "minimax-anthropic" => config.apply_preset(ProviderPreset::MiniMaxAnthropic),
                    "openai" => config.apply_preset(ProviderPreset::OpenAi),
                    "anthropic" => config.apply_preset(ProviderPreset::Anthropic),
                    _ => println!("Unknown preset: {}", name),
                }
                config.save()?;
                println!("✔ Switched preset to: {}", name.cyan());
            }
        },
    }

    Ok(())
}
