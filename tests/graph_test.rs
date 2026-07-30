use husk_cli::graph::codebase::{CodeNode, CodebaseGraph, NodeKind};
use husk_cli::graph::execution::ExecutionGraph;
use std::path::PathBuf;

#[test]
fn test_codebase_graph_add_node() {
    let mut cb_graph = CodebaseGraph::new();
    let node = CodeNode {
        id: "src/main.rs".to_string(),
        name: "main.rs".to_string(),
        kind: NodeKind::File,
        path: PathBuf::from("src/main.rs"),
    };

    let idx = cb_graph.add_node(node);
    assert_eq!(cb_graph.graph.node_count(), 1);
    assert_eq!(cb_graph.graph[idx].name, "main.rs");
}

#[test]
fn test_execution_graph_dag_construction() {
    let subgraphs = vec!["src/main.rs".to_string(), "src/config.rs".to_string()];
    let exec_graph = ExecutionGraph::new_default_pipeline("Implement OAuth2", subgraphs, 5);

    assert_eq!(exec_graph.nodes.len(), 3);
    assert_eq!(exec_graph.dependencies.len(), 2);
    assert_eq!(exec_graph.max_retries, 5);

    let dag = exec_graph.build_dag();
    assert_eq!(dag.node_count(), 3);
    assert_eq!(dag.edge_count(), 2);
}
