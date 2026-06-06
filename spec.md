# Husk-CLI: Agentic Proxy & Telemetry Specification
Repository: github.com/mcpe500/husk-cli.git

## 1. Executive Summary
Husk-CLI operates as the primary agentic proxy and interaction layer for the system. Guided by an intentional minimalism aesthetic, it focuses on high-speed execution, multi-turn ReAct reasoning, and silent dataset harvesting. Crucially, Husk-CLI is the exclusive component that communicates with Large Language Models (LLMs) and actively queries the underlying Husk-RAG backend to fulfill user or systemic objectives.

## 2. Model Integration & Abstraction
The CLI is designed to dynamically route inference requests, strictly enforcing the separation of LLM logic from the storage backend.
- **Primary Inference Engine**: Integration with gemma-4-31b-it via Google AI Studio for robust, high-parameter reasoning capabilities during the initial MVP phase.
- **Compatibility Layer**: A native OpenAI-compatible API protocol implementation. This ensures drop-in replacement capabilities, allowing the system to seamlessly switch to other local or cloud endpoints without modifying the core proxy logic.

## 3. Context Window Architecture (32k Standard)
To prevent token overflow and context degradation, the CLI orchestrates a strict 32,000-token memory budget. The architecture relies on dynamic partitioning:

| Partition | Allocation Limit | Functional Responsibility |
| :--- | :--- | :--- |
| **Current Context** | 16k Tokens | Maintains the immediate multi-turn conversation, real-time ReAct scratchpad (thoughts/actions/observations), and system tool definitions. |
| **Sliding Context** | 16k Tokens | Injects exact vectorized text chunks retrieved from Husk-RAG and manages auto-compacted summaries of older conversational turns. |

## 4. Multi-Turn ReAct Agent Workflow
The CLI utilizes a Reason-Act-Observe loop to handle complex, multi-step operations (e.g., executing side projects). Following a "Show, Don't Tell" logging principle, internal states are clearly structured.

### [Workflow Sequence]
1. **USER INSTRUCTION** -> Husk-CLI
2. **CLI REASONING**    -> Analyzes if external context is needed.
3. **CLI ACTION**       -> Queries Husk-RAG via /api/v1/query.
4. **RAG RESPONSE**     -> Returns top-K chunks to CLI.
5. **PROMPT ASSEMBLY**  -> CLI structures 32k context (Chunks + History + Prompts).
6. **LLM CALL**         -> Dispatches prompt to Gemma 4 31b IT / OpenAI API.
7. **OBSERVATION**      -> Evaluates LLM output, executes tools, or returns answer.

## 5. Dataset Harvesting (Telemetry) Pipeline
The proxy operates concurrently as a data aggregator. All multi-turn interactions, specifically the mapping of the context state to the chosen tool call and final output, are silently formatted and stored locally. This generates a high-resolution, organic dataset intended for the future fine-tuning of smaller, extensible architectures (e.g., LFM 2.5).

## 6. Development & Deployment Methodology
- **Engineering Cycle**: Execution utilizing the Spiral Model. The CLI proxy will be iteratively refined, testing LLM context stability and tool invocation accuracy at each loop.
- **Environment Optimization**: Configured for deployment flexibility, prioritizing stable execution in local WSL2 environments and headless Ubuntu VPS setups.
