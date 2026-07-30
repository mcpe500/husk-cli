use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentRole {
    Orchestrator,
    DevAction,
    Validation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Running,
    Success,
    Failed { error: String },
    NeedsRepair { traceback: String, iteration: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionNode {
    pub id: String,
    pub role: AgentRole,
    pub instruction: String,
    pub status: NodeStatus,
    pub assigned_subgraph_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionGraph {
    pub nodes: Vec<ExecutionNode>,
    pub dependencies: Vec<(usize, usize)>, // (parent_idx, child_idx)
    pub max_retries: usize,
    pub current_iteration: usize,
}

impl ExecutionGraph {
    pub fn new_default_pipeline(prompt: &str, codebase_subgraph: Vec<String>, max_retries: usize) -> Self {
        let root = ExecutionNode {
            id: "node-0-orchestrator".to_string(),
            role: AgentRole::Orchestrator,
            instruction: format!("Analyze user goal and propose execution plan: {}", prompt),
            status: NodeStatus::Pending,
            assigned_subgraph_nodes: codebase_subgraph.clone(),
        };

        let dev_agent = ExecutionNode {
            id: "node-1-dev-action".to_string(),
            role: AgentRole::DevAction,
            instruction: format!("Synthesize and apply code changes for goal: {}", prompt),
            status: NodeStatus::Pending,
            assigned_subgraph_nodes: codebase_subgraph.clone(),
        };

        let val_agent = ExecutionNode {
            id: "node-2-validation".to_string(),
            role: AgentRole::Validation,
            instruction: "Verify code syntax, test cases, and quality assertions using real system tools.".to_string(),
            status: NodeStatus::Pending,
            assigned_subgraph_nodes: codebase_subgraph,
        };

        Self {
            nodes: vec![root, dev_agent, val_agent],
            dependencies: vec![(0, 1), (1, 2)],
            max_retries,
            current_iteration: 1,
        }
    }

    #[allow(dead_code)]
    pub fn build_dag(&self) -> DiGraph<ExecutionNode, ()> {
        let mut dag = DiGraph::new();
        let mut indices = Vec::new();

        for node in &self.nodes {
            let idx = dag.add_node(node.clone());
            indices.push(idx);
        }

        for &(src, tgt) in &self.dependencies {
            if src < indices.len() && tgt < indices.len() {
                dag.add_edge(indices[src], indices[tgt], ());
            }
        }

        dag
    }
}
