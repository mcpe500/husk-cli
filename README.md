# Husk-CLI: Graph-Native Code CLI & Multi-Agent Orchestration System

[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Husk-CLI is a **Graph-Native, High-Performance Multi-Agent Coding CLI** built in **Rust** with an interactive **Ratatui Terminal User Interface (TUI)**. It orchestrates complex software development workflows by decomposing user prompts into dynamic **Looping Execution Graphs (DAGs)** run by specialized subagents while indexing target repositories into a semantic **Codebase Graph** (AST, call graphs, import dependencies).

---

## 🌟 Key Features

- **Interactive Ratatui TUI Dashboard**:
  - Launch an interactive terminal dashboard by running `husk-cli` directly in your terminal.
  - Interactive tabs for **Dashboard Overview**, **Multi-Agent Run**, and **Provider Presets Selector**.
- **Global Terminal Binary Installation**:
  - Install globally using `cargo install --path .` so `husk-cli` is accessible from anywhere in your shell.
- **Dynamic QA Looping Feedback Engine**:
  - The Execution Graph **loops continuously** between the Dev Agent and Validation/QA Agent.
  - If a validation check (`cargo test`, static linter, build script) fails, stdout/stderr error tracebacks loop back to the Dev Agent for automatic repair until 100% verified or max retries are reached.
- **Anti-Cheating Protocol**:
  - Validation Agents execute real system tools and evaluate actual exit codes. No fake fallbacks or dummy mock outputs.
- **Dual-Graph Paradigm**:
  - **Codebase Graph**: AST and import dependency indexing via `petgraph`.
  - **Execution Graph**: Multi-agent DAG scheduling with Orchestrator, Dev/Action, and Validation Agents.
- **Provider Support**:
  - **GLM Coding Plan Z.AI** (`https://api.z.ai/api/anthropic` & `https://api.z.ai/api/coding/paas/v4`)
  - **MiniMax** (`https://api.minimax.io/v1` & `https://api.minimax.io/anthropic` with model `MiniMax-M3`)
  - **OpenAI** (`https://api.openai.com/v1`)
  - **Anthropic** (`https://api.anthropic.com/v1`)

---

## 🚀 Installation & Global Setup

Install `husk-cli` globally to your `$HOME/.cargo/bin` PATH:

```bash
cargo install --path .
```

After installation, `husk-cli` can be executed directly from any directory in your terminal!

---

## 🖥️ Interactive TUI Dashboard

To launch the interactive Terminal User Interface (TUI):

```bash
husk-cli
# or: husk-cli tui
```

### TUI Navigation:
- Press `1` for **Dashboard Overview** (Status of Codebase Graph, active Provider, API Key).
- Press `2` for **Multi-Agent Execution** (Interactive prompt entry).
- Press `3` for **Provider Presets Selector** (Switch between GLM Z.AI, MiniMax, OpenAI, Anthropic via arrow keys).
- Press `q` or `Esc` to Exit TUI.

---

## ⚡ Provider Configuration Guide (CLI Mode)

### 1. GLM Coding Plan Z.AI

#### Anthropic Protocol (Recommended)
```bash
husk-cli config preset zai-anthropic
husk-cli config set api_key "your-z.ai-api-key"
```

#### OpenAI Protocol
```bash
husk-cli config preset zai-openai
husk-cli config set api_key "your-z.ai-api-key"
```

### 2. MiniMax (`MiniMax-M3`)

#### OpenAI Compatible Protocol (Default)
```bash
husk-cli config preset minimax-openai
husk-cli config set api_key "sk-cp-your-minimax-token-plan-key"
```

---

## 📖 CLI Commands Reference

```bash
# Launch TUI Dashboard
husk-cli

# Index codebase into graph
husk-cli index

# Run multi-agent prompt with QA feedback loop
husk-cli run "Refactor authentication module" --max-retries 5
```

---

## 🧪 Testing

```bash
cargo test
```

---

## 📄 License

MIT License.
