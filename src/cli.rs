use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "husk", author = "Husk Team", version = "0.1.0", about = "Token-cheap agentic coding harness: DeepSeek supervisor + embedded MiniCPM context worker")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch interactive Terminal User Interface (TUI)
    Tui,

    /// Interactive chat session (REPL, or one-shot with a prompt argument)
    Chat {
        /// Optional one-shot prompt (omit to enter REPL)
        prompt: Option<String>,

        /// Resume an existing session id
        #[arg(short, long)]
        session: Option<String>,

        /// Enable the local MiniCPM worker for this run (overrides config)
        #[arg(long = "local")]
        local: bool,

        /// Attach context files up-front (repeatable, implies worker value)
        #[arg(short = 'f', long = "file")]
        files: Vec<PathBuf>,
    },

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

        /// Model provider preset (zai-anthropic, zai-openai, minimax-openai, minimax-anthropic, openai, anthropic, deepseek)
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

    /// Session history stored in husk.db
    Sessions {
        #[command(subcommand)]
        action: SessionCommands,
    },

    /// Token usage & savings statistics for a session
    Stats {
        /// Session id (defaults to the most recent session)
        #[arg(short, long)]
        session: Option<String>,
    },

    /// Full-text search over session history (FTS5, index-backed)
    Search {
        /// Query text (FTS5 MATCH syntax, falls back to LIKE)
        query: String,

        /// Maximum results
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },

    /// Local embedded worker (MiniCPM5-2B) management
    Local {
        #[command(subcommand)]
        action: LocalCommands,
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
pub enum SessionCommands {
    /// List recent sessions
    List,

    /// Print a session transcript
    Show { id: String },

    /// Delete a session and its history
    Rm { id: String },
}

#[derive(Subcommand, Debug)]
pub enum LocalCommands {
    /// Show local worker availability & configuration
    Info,

    /// Pre-load the local model and report the estimated footprint
    Warm,
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
