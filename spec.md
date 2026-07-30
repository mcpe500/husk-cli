# Husk-CLI: Graph-Native Code CLI & Multi-Agent Orchestration Specification
Repository: github.com/mcpe500/husk-cli.git

---

## 1. Executive Summary

Husk-CLI is a **Graph-Native, High-Performance Multi-Agent Coding CLI** built in **Rust**. Designed for minimal execution overhead, high memory safety, and cross-platform single-binary distribution, `husk-cli` decouples complex software engineering tasks into dynamic, graph-orchestrated micro-agent task graphs.

By default, all software engineering tasks are executed across a **Looping Agent Execution Graph** composed of an Orchestrator Node at the top, Action/Dev Worker Agents, and Validation/QA Agents. Simultaneously, Husk-CLI indexes target repositories into a semantic **Codebase Graph** (AST, call graphs, import dependencies) to deliver targeted subgraph context for code generation.

Extensibility is built natively into `husk-cli` via Model Context Protocol (MCP) integrations, folder-based Skills, modular Plugins, event lifecycle Hooks, and built-in support for major model providers including **GLM Coding Plan Z.AI** and **MiniMax**.

---

## 2. Dual-Graph Paradigm

Husk-CLI operates on two primary graph structures:

```
                  ┌───────────────────────────────┐
                  │    Codebase Graph (Context)   │
                  │   [AST, Files, Call Graph]    │
                  └──────────────┬────────────────┘
                                 │ Subgraph Context
                                 ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Looping Agent Execution Graph (Orchestration)            │
│                                                                        │
│                     ┌────────────────────────┐                         │
│                     │   Orchestrator Agent   │ (Top Node)              │
│                     └───────────┬────────────┘                         │
│                                 │ Dispatches DAG                       │
│                                 ▼                                      │
│                     ┌────────────────────────┐                         │
│             ┌──────►│  Dev / Action Worker   │                         │
│             │       └───────────┬────────────┘                         │
│             │                   │ Applies Code Changes                 │
│    Loop Back│                   ▼                                      │
│    Feedback │       ┌────────────────────────┐                         │
│    with Logs│       │ Validation / QA Agent  │ (Real Commands)         │
│             │       └───────────┬────────────┘                         │
│             │                   │                                      │
│             │  Fail / Error     │ Pass / 100% Tested                   │
│             └───────────────────┴──────────┐                           │
│                                            ▼                           │
│                                       [Complete]                       │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Codebase Graph (Context Engine)**: Represents codebase structural and semantic topology. Nodes represent files, modules, classes, functions, and symbols, while edges capture imports, function invocations, inheritance, and data flow.
2. **Agent Execution Graph (Orchestration Engine)**: A dynamic Directed Acyclic Graph (DAG) constructed per prompt with active **looping feedback edges** between Dev and Validation Agents.

---

## 3. Dynamic Looping Feedback & Anti-Cheating Protocol

### A. Looping Execution Loop
The execution graph does not stop after a single pass. It iterates continuously:
1. **Dev / Action Agent**: Generates and writes actual code modifications to local workspace files.
2. **Validation / QA Agent**: Executes real terminal checks (`cargo test`, static linters, syntax checks, build scripts).
3. **Loop Evaluation**:
   - If tests/checks **FAIL** or are incomplete: A feedback edge routes the exact stdout/stderr traceback back to the Dev Agent, starting **Loop $N+1$**.
   - If tests/checks **PASS 100%**: Execution terminates with a success status.
   - Max iteration guard: Enforces `--max-retries` (default: 5) to prevent infinite loops on unresolvable bugs.

### B. Anti-Cheating Verification Rules
- **No Mocking or Hardcoded Fallbacks**: Validation Agents MUST execute actual system tools and inspect true exit codes.
- **Persistent Disk Verification**: Dev Agents MUST write modifications to disk before triggering validation.
- **Traceback Preservation**: Every failed iteration appends historical error context to the next iteration prompt so the Dev Agent learns from previous failed attempts.

---

## 4. Codebase Graph Indexing Engine

The CLI provides native indexing capabilities to translate local code repositories into a queryable Codebase Graph stored at `.husk/graph.json`.

---

## 5. Model Integration & Provider Ecosystem

`husk-cli` supports modular LLM provider integration with first-class preset support for **GLM Coding Plan Z.AI**, **MiniMax**, **OpenAI**, and **Anthropic**.

| Provider Preset | Protocol | Default Base URL | Default Model ID |
| :--- | :--- | :--- | :--- |
| `zai` / `glm-anthropic` | Anthropic Messages | `https://api.z.ai/api/anthropic` | `glm-4` |
| `zai-openai` / `glm-openai` | OpenAI Chat | `https://api.z.ai/api/coding/paas/v4` | `glm-4` |
| `minimax` / `minimax-openai` | OpenAI Compatible | `https://api.minimax.io/v1` | `MiniMax-M3` |
| `minimax-anthropic` | Anthropic Compatible | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `openai` | OpenAI Chat | `https://api.openai.com/v1` | `gpt-4o` |
| `anthropic` | Anthropic Messages | `https://api.anthropic.com/v1` | `claude-3-5-sonnet-20241022` |

---

## 6. CLI Commands Reference

```bash
# Run multi-agent execution workflow with looping QA loop
husk run "<prompt>" [--provider <provider>] [--max-retries <n>]
```
