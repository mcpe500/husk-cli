use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "husk", author = "Husk Team", version = "0.1.0", about = "Graph-Native Code CLI & Multi-Agent Orchestrator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch interactive Terminal User Interface (TUI)
    Tui,

    /// Index local codebase into a Codebase Graph (.husk/graph.json)
    Index {
        /// Target directory path to index
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Force re-indexing from scratch
        #[arg(short, long)]
        force: bool,
    },

    /// Run multi-agent execution workflow for a user prompt
    Run {
        /// User instruction or prompt
        prompt: String,

        /// Model provider preset (zai-anthropic, zai-openai, minimax-openai, minimax-anthropic, openai, anthropic)
        #[arg(short, long)]
        provider: Option<String>,

        /// Specific model ID override
        #[arg(short, long)]
        model: Option<String>,

        /// Maximum number of feedback loop retries
        #[arg(short, long, default_value_t = 5)]
        max_retries: usize,

        /// Export generated execution graph to JSON file
        #[arg(long)]
        graph_out: Option<PathBuf>,

        /// Automatically approve tool execution actions
        #[arg(long)]
        auto_approve: bool,
    },

    /// Plugin management commands
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },

    /// Skills management commands
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },

    /// MCP server management commands
    Mcp {
        #[command(subcommand)]
        action: McpCommands,
    },

    /// Configuration management commands
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginCommands {
    /// List installed plugins
    List,

    /// Install a plugin from directory or repo
    Install { path: String },

    /// Remove an installed plugin
    Remove { name: String },
}

#[derive(Subcommand, Debug)]
pub enum SkillCommands {
    /// List available skills
    List,

    /// Show details of a specific skill
    Show { name: String },
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// List configured MCP servers
    List,

    /// Add a new MCP server connection
    Add {
        name: String,
        #[arg(short, long)]
        command: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Display active configuration
    Show,

    /// Set a configuration key-value pair
    Set { key: String, value: String },

    /// Switch provider preset (zai-anthropic, zai-openai, minimax-openai, minimax-anthropic, openai, anthropic)
    Preset { name: String },
}
