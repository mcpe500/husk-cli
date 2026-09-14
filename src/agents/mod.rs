pub mod prefix;
pub mod session;

use anyhow::Result;
use crate::graph::execution::{ExecutionGraph, NodeStatus};
use crate::providers::LlmProvider;
use std::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    Log(String),
    TokenStream(String),
    NodeStatusChanged { node_idx: usize, status: String },
    OrchestratorThought(String),
    DevAgentThought(String),
    ValidationThought(String),
    LoopIterationStarted { iteration: usize, max_retries: usize },
    ValidationResult { passed: bool, error: Option<String> },
    Finished { success: bool },
}

pub struct AgentOrchestrator {
    provider: Box<dyn LlmProvider>,
}

impl AgentOrchestrator {
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub async fn execute_graph(&mut self, graph: &mut ExecutionGraph) -> Result<bool> {
        self.execute_graph_with_sender(graph, None).await
    }

    pub async fn execute_graph_with_sender(
        &mut self,
        graph: &mut ExecutionGraph,
        tx: Option<UnboundedSender<ExecutionEvent>>,
    ) -> Result<bool> {
        let emit = |evt: ExecutionEvent| {
            if let Some(ref sender) = tx {
                let _ = sender.send(evt);
            }
        };

        emit(ExecutionEvent::Log("▶ Launching Multi-Agent Execution Graph...".to_string()));

        // Step 1: Run Orchestrator Node (Node 0)
        let orchestrator_node = &mut graph.nodes[0];
        orchestrator_node.status = NodeStatus::Running;
        emit(ExecutionEvent::NodeStatusChanged {
            node_idx: 0,
            status: "RUNNING".to_string(),
        });

        let msg = format!("⚡ Executing Node [{:?}] {}", orchestrator_node.role, orchestrator_node.id);
        emit(ExecutionEvent::Log(msg));

        let (token_tx, mut token_rx) = unbounded_channel::<String>();
        let emit_tx = tx.clone();

        tokio::spawn(async move {
            while let Some(chunk) = token_rx.recv().await {
                if let Some(ref sender) = emit_tx {
                    let _ = sender.send(ExecutionEvent::TokenStream(chunk));
                }
            }
        });

        let plan_resp = self
            .provider
            .stream_completion(
                "You are Husk-CLI Orchestrator. Formulate a step-by-step dev and test plan.",
                &orchestrator_node.instruction,
                token_tx.clone(),
            )
            .await?;

        emit(ExecutionEvent::OrchestratorThought(plan_resp.clone()));
        emit(ExecutionEvent::Log(format!("Orchestrator Plan:\n{}", plan_resp.trim())));

        orchestrator_node.status = NodeStatus::Success;
        emit(ExecutionEvent::NodeStatusChanged {
            node_idx: 0,
            status: "PASSED".to_string(),
        });

        // Step 2: Active Looping Loop between Dev Agent and Validation Agent
        let mut iteration = 1;
        let mut last_error_traceback: Option<String> = None;

        while iteration <= graph.max_retries {
            graph.current_iteration = iteration;
            emit(ExecutionEvent::LoopIterationStarted {
                iteration,
                max_retries: graph.max_retries,
            });

            let loop_header = format!("🔄 --- LOOP ITERATION {}/{} ---", iteration, graph.max_retries);
            emit(ExecutionEvent::Log(loop_header));

            // Dev/Action Agent Node Execution
            let dev_node = &mut graph.nodes[1];
            dev_node.status = NodeStatus::Running;
            emit(ExecutionEvent::NodeStatusChanged {
                node_idx: 1,
                status: format!("LOOP {} RUNNING", iteration),
            });

            let dev_msg = format!("⚡ Executing Node [{:?}] {}", dev_node.role, dev_node.id);
            emit(ExecutionEvent::Log(dev_msg));

            let mut dev_prompt = dev_node.instruction.clone();
            if let Some(ref error_log) = last_error_traceback {
                dev_prompt.push_str(&format!(
                    "\n\n[PREVIOUS VALIDATION FAILURE (Iteration {})]:\n{}",
                    iteration - 1,
                    error_log
                ));
            }

            let dev_resp = self
                .provider
                .stream_completion(
                    "You are Husk-CLI Dev/Action Agent. Write complete code and fix all reported errors without cheating.",
                    &dev_prompt,
                    token_tx.clone(),
                )
                .await?;

            emit(ExecutionEvent::DevAgentThought(dev_resp.clone()));
            emit(ExecutionEvent::Log(format!("Dev Action Output:\n{}", dev_resp.trim())));
            dev_node.status = NodeStatus::Success;

            // Validation / QA Agent Node Execution (Anti-Cheating Real Tool Check)
            let val_node = &mut graph.nodes[2];
            val_node.status = NodeStatus::Running;
            emit(ExecutionEvent::NodeStatusChanged {
                node_idx: 2,
                status: "VALIDATING".to_string(),
            });

            let val_msg = format!("⚡ Executing Node [{:?}] {}", val_node.role, val_node.id);
            emit(ExecutionEvent::Log(val_msg));

            emit(ExecutionEvent::Log("🔍 Running anti-cheating real tool validation (cargo check)...".to_string()));

            let test_output = Command::new("cargo")
                .arg("check")
                .output();

            let is_valid = match test_output {
                Ok(output) if output.status.success() => true,
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let combined_err = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);
                    let err_msg = format!("❌ Validation Check Failed! Errors detected:\n{}", combined_err);
                    emit(ExecutionEvent::Log(err_msg));
                    last_error_traceback = Some(combined_err);
                    false
                }
                Err(_err) => {
                    let val_resp = self
                        .provider
                        .stream_completion(
                            "You are Husk-CLI Validation Agent. Evaluate code syntax and verify if execution goal is met. Output PASS or FAIL.",
                            &format!("Goal: {}\nDev Output: {}", dev_prompt, dev_resp),
                            token_tx.clone(),
                        )
                        .await?;
                    emit(ExecutionEvent::ValidationThought(val_resp.clone()));
                    if val_resp.to_uppercase().contains("FAIL") {
                        last_error_traceback = Some(val_resp);
                        false
                    } else {
                        true
                    }
                }
            };

            emit(ExecutionEvent::ValidationResult {
                passed: is_valid,
                error: last_error_traceback.clone(),
            });

            if is_valid {
                val_node.status = NodeStatus::Success;
                emit(ExecutionEvent::NodeStatusChanged {
                    node_idx: 2,
                    status: "PASSED".to_string(),
                });
                let pass_msg = format!("✔ Validation Passed 100% on Iteration {}!", iteration);
                emit(ExecutionEvent::Log(pass_msg));
                emit(ExecutionEvent::Finished { success: true });
                return Ok(true);
            } else {
                val_node.status = NodeStatus::NeedsRepair {
                    traceback: last_error_traceback.clone().unwrap_or_default(),
                    iteration,
                };
                emit(ExecutionEvent::NodeStatusChanged {
                    node_idx: 2,
                    status: format!("REPAIRING LOOP {}", iteration),
                });
                let retry_msg = "⚠️ Validation failed. Triggering Feedback Loop to Dev Agent...".to_string();
                emit(ExecutionEvent::Log(retry_msg));
            }

            iteration += 1;
        }

        let fail_msg = format!("❌ Execution Graph failed after {} iterations.", graph.max_retries);
        emit(ExecutionEvent::Log(fail_msg));
        emit(ExecutionEvent::Finished { success: false });
        Ok(false)
    }
}
