# Husk-CLI: Token-Cheap Agentic Coding Harness

Husk is a **Rust** single-binary coding agent designed around one idea:
**the harness, not the model, decides where tokens go.**

- **Supervisor (reasoning, paid):** `deepseek/deepseek-v4-flash-0731` via **Netra Runtime**
  (`https://api.netraruntime.com/v1`, OpenAI-compatible) — per-request
  reasoning toggle `{"enabled": true, "effort": "low|high|max", "exclude": false}`.
  *(Model IDs verified live: Z.AI's `glm-4` is retired — presets use `glm-5.2`.)*
- **Worker (context ops, free, embedded):** `MiniCPM5-2B` (Apache-2.0,
  `LlamaForCausalLM`, 2.5B) via llama.cpp — **optional**, lazily loaded, and
  only ever *folds context*. It never does hard reasoning: its output must
  pass a deterministic verification gate or the task escalates to DeepSeek.

**Goals:** cheaper than other harnesses for the same work, same or better
results, and gentle enough for an 8GB laptop that already runs a browser,
VSCode and Docker.

## Install

**1. Easiest — prebuilt binary (Linux x86_64):**

```bash
curl -fsSL https://raw.githubusercontent.com/mcpe500/husk-cli/main/install.sh | sh
```

(Windows: grab `husk-cli-x86_64-pc-windows-msvc.zip` from the
[Releases page](https://github.com/mcpe500/husk-cli/releases/latest).)

**2. From source via cargo (cloud-only, no model build):**

```bash
cargo install --git https://github.com/mcpe500/husk-cli.git
```

**3. Full laptop build — embedded MiniCPM worker (needs cmake + C toolchain):**

```bash
# CPU only
cargo install --git https://github.com/mcpe500/husk-cli.git --features local

# + Intel iGPU offload (Vulkan)
LLAMA_CMAKE_ARGS="-DGGML_VULKAN=ON" \
  cargo install --git https://github.com/mcpe500/husk-cli.git --features local,vulkan
```

**Then set up the Netra Runtime key:**

```bash
husk config preset deepseek
husk config set api_key "$NETRA_API_KEY"
husk chat
```

---

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│ husk kernel                                                    │
│                                                                │
│  TaskRouter (deterministic, zero LLM calls)                    │
│      │  context-op → Worker        reasoning → Supervisor      │
│      ▼                             ▼                           │
│  MiniCPM5-2B (local)        DeepSeek V4 Flash 0731 (cloud)     │
│  · fold/summarize/dedup     · ALL hard reasoning               │
│  · verification gate ──fail──▶ escalate to supervisor          │
│      │                             │                           │
│      ▼                             ▼                           │
│  ContextAssembler: stable prefix first (prompt-cache hits),    │
│  dynamic last · token window · tool dedup · old-tool purge     │
│      │                                                         │
│      ▼                                                         │
│  husk.db (SQLite): sessions · messages · full-fidelity parts   │
│  usage records · counters · FTS5 search index                  │
└────────────────────────────────────────────────────────────────┘
```

Token-saving layers (prompt1.md):

| Layer | Module | Effect |
|---|---|---|
| Output compression (RTK-style) | `compress` | git/cargo/npm/test output → errors + actionable lines, hard token cap |
| Prompt-cache ordering | `context` | stable blocks first, dynamic last → provider prefix-cache keeps hitting |
| Context pruning | `context` | window cap, repeated-tool dedup, old-tool purge (full output stays in husk.db) |
| Deterministic routing | `router` | zero-cost keyword heuristics pick worker vs supervisor + `reasoning_effort` |
| Verification gate | `verify` | JSON fields / identifiers / token budget — violations escalate |
| Accounting | `tokenutil` + `db` | per-session ledger: what the paid model saw vs what was folded away |
| Index-backed search | `db` (FTS5) | `husk search` over everything the session ever saw |

## Memory discipline (the 8GB-laptop contract)

- **Cloud-only by default.** The local model never loads unless you opt in
  (`--local` or `[local] enabled = true`); worker tasks then escalate to the
  supervisor instead.
- **Lazy + dedicated thread.** The llama.cpp backend is !Send, so it lives on
  one OS thread spawned on first use — zero model RAM at startup.
- **Hard memory cap.** Before load, husk estimates KV-cache (42-layer GQA
  geometry) + hot weights and *refuses to start* over `memory_cap_mb`
  (default 1792MB). Defaults: `ctx_tokens = 4096`, `gpu_layers = 0` (CPU).
- **mmap weights.** The GGUF is memory-mapped: pages are file-backed and
  evictable when Docker/browser need RAM back.
- **Bounded app footprint.** History lives in SQLite (windowed reads),
  scrollback capped at 500 lines, streaming everywhere — cloud-only RSS
  stays tiny.

## CPU + Intel integrated graphics

llama.cpp backends at build time:

```bash
# CPU only (works everywhere; needs cmake + a C toolchain)
cargo build --release --features local

# Intel iGPU via Vulkan (Mesa/ANV on Linux, Intel driver on Windows)
LLAMA_CMAKE_ARGS="-DGGML_VULKAN=ON" cargo build --release --features local,vulkan
```

Then opt in at runtime and tune for your shared-memory budget
(`.husk/config.toml`):

> Build note: llama.cpp's bindgen step needs libclang's C headers. On systems
> where clang cannot find them, set
> `BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/gcc/x86_64-linux-gnu/<ver>/include"`
> (Debian/Ubuntu path shown) for the build.

```toml
[local]
enabled = true
ctx_tokens = 4096      # KV ≈ 170MB f16 at 4096
gpu_layers = 99        # offload to the iGPU (0 = CPU only)
memory_cap_mb = 1792   # pre-flight ceiling; loader refuses to exceed
# model_path = "/path/to/MiniCPM5-2B-Q4_K_M.gguf"
```

**Single-file build with the model baked in:**

```bash
HUSK_EMBED_MODEL=$PWD/MiniCPM5-2B-Q4_K_M.gguf \
  cargo build --release --features local,embed-model
```

The weights are chunked into the executable; first worker use streams them
to the OS cache dir (sha256-verified) and llama.cpp mmaps from there.

## CLI

```bash
husk                     # opencode-style TUI (3-zone, Normal/Insert modal)
husk chat "prompt"       # one-shot; `husk chat` for a REPL
husk chat --local        # same, with the embedded MiniCPM worker enabled
husk run "refactor ..."  # legacy multi-agent graph loop
husk sessions list|show <id>|rm <id>
husk stats               # token ledger: supervisor vs worker, savings, cost
husk search "panic"      # FTS5 over all history
husk local info|warm     # inspect / pre-load the embedded worker
husk config preset deepseek && husk config set api_key "..."
# atau lewat environment: export NETRA_API_KEY=... (fallback HUSK_API_KEY)
```

Provider presets: `deepseek` (default supervisor, Netra Runtime),
`zai-anthropic`/`zai-openai` (glm-5.2), `minimax-*`, `openai`, `anthropic`.

### In-session (TUI & chat)

| Input | Meaning |
|---|---|
| `@src/main.rs` | attach a file as context (worker folds it before the supervisor sees it) |
| `!cargo test` | run a shell command; output is compressed, stored in husk.db, folded |
| `/new` `/sessions` `/stats` `/model <id>` `/quit` | session control |
| `Ctrl+X` then `n`/`t`/`q` | new session · cycle theme · quit (TUI) |
| `Esc` / `i` / `a` | drop to Normal scrollback · back to Insert (TUI) |
| `Tab` | toggle the local worker for subsequent turns (TUI) |

## Verification (TDD)

```bash
cargo test                      # 137 assertions across lib + integration
cargo test --features local     # + compiles the llama.cpp engine
cargo clippy --all-targets
```

Red-green discipline: every module (`tokenutil`, `db`, `compress`, `context`,
`router`, `verify`, `providers`, `agents/session`, `local`, `tui/input`,
`tui/theme`) carries unit tests written against its contract — including the
verification-gate escalation path, the cache-stable prefix ordering, and the
memory-cap pre-flight refusal.

## Honest limits

- The worker improves *cost*, not intelligence: all reasoning stays with
  DeepSeek; folds are verified and anything suspicious escalates.
- `husk run`'s graph loop still uses the legacy string-based path and the
  Dev agent does not yet write files; the chat/TUI session layer is the
  primary product surface.
- MCP/plugins remain facade stubs from the original scaffold.

## License

MIT
