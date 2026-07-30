use anyhow::Result;
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Module,
    Class,
    Function,
    Struct,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeKind {
    Imports,
    Calls,
    DependsOn,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodebaseGraphData {
    pub nodes: Vec<CodeNode>,
    pub edges: Vec<(usize, usize, EdgeKind)>,
}

pub struct CodebaseGraph {
    pub graph: UnGraph<CodeNode, EdgeKind>,
    node_map: HashMap<String, NodeIndex>,
}

impl CodebaseGraph {
    pub fn new() -> Self {
        Self {
            graph: UnGraph::new_undirected(),
            node_map: HashMap::new(),
        }
    }

    pub fn index_directory<P: AsRef<Path>>(&mut self, root_path: P) -> Result<usize> {
        let root = root_path.as_ref();
        let fn_regex = Regex::new(r"(?m)^\s*(?:pub\s+)?fn\s+([a-zA-Z0-9_]+)").unwrap();
        let struct_regex = Regex::new(r"(?m)^\s*(?:pub\s+)?struct\s+([a-zA-Z0-9_]+)").unwrap();
        let import_regex = Regex::new(r"(?m)^\s*(?:use|import)\s+([a-zA-Z0-9_:]+)").unwrap();

        let mut file_count = 0;

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "target" && name != "node_modules"
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_file() {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

                if matches!(ext, "rs" | "py" | "js" | "ts" | "go" | "c" | "cpp" | "h" | "md") {
                    file_count += 1;
                    let file_id = path.to_string_lossy().to_string();
                    let file_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| file_id.clone());

                    let file_node = CodeNode {
                        id: file_id.clone(),
                        name: file_name,
                        kind: NodeKind::File,
                        path: path.to_path_buf(),
                    };

                    let file_idx = self.add_node(file_node);

                    if let Ok(content) = fs::read_to_string(path) {
                        // Extract Functions
                        for cap in fn_regex.captures_iter(&content) {
                            if let Some(fn_name) = cap.get(1) {
                                let fn_node = CodeNode {
                                    id: format!("{}::{}", file_id, fn_name.as_str()),
                                    name: fn_name.as_str().to_string(),
                                    kind: NodeKind::Function,
                                    path: path.to_path_buf(),
                                };
                                let fn_idx = self.add_node(fn_node);
                                self.graph.add_edge(file_idx, fn_idx, EdgeKind::DependsOn);
                            }
                        }

                        // Extract Structs
                        for cap in struct_regex.captures_iter(&content) {
                            if let Some(st_name) = cap.get(1) {
                                let st_node = CodeNode {
                                    id: format!("{}::{}", file_id, st_name.as_str()),
                                    name: st_name.as_str().to_string(),
                                    kind: NodeKind::Struct,
                                    path: path.to_path_buf(),
                                };
                                let st_idx = self.add_node(st_node);
                                self.graph.add_edge(file_idx, st_idx, EdgeKind::DependsOn);
                            }
                        }

                        // Extract Imports
                        for cap in import_regex.captures_iter(&content) {
                            if let Some(imp_name) = cap.get(1) {
                                let imp_node = CodeNode {
                                    id: format!("import::{}", imp_name.as_str()),
                                    name: imp_name.as_str().to_string(),
                                    kind: NodeKind::Module,
                                    path: path.to_path_buf(),
                                };
                                let imp_idx = self.add_node(imp_node);
                                self.graph.add_edge(file_idx, imp_idx, EdgeKind::Imports);
                            }
                        }
                    }
                }
            }
        }

        Ok(file_count)
    }

    pub fn add_node(&mut self, node: CodeNode) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&node.id) {
            idx
        } else {
            let id = node.id.clone();
            let idx = self.graph.add_node(node);
            self.node_map.insert(id, idx);
            idx
        }
    }

    pub fn export_data(&self) -> CodebaseGraphData {
        let mut nodes = Vec::new();
        for idx in self.graph.node_indices() {
            nodes.push(self.graph[idx].clone());
        }

        let mut edges = Vec::new();
        for edge in self.graph.edge_references() {
            edges.push((
                edge.source().index(),
                edge.target().index(),
                edge.weight().clone(),
            ));
        }

        CodebaseGraphData { nodes, edges }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let data = self.export_data();
        let json = serde_json::to_string_pretty(&data)?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let data: CodebaseGraphData = serde_json::from_str(&json)?;

        let mut graph = UnGraph::new_undirected();
        let mut node_map = HashMap::new();

        for node in data.nodes {
            let id = node.id.clone();
            let idx = graph.add_node(node);
            node_map.insert(id, idx);
        }

        for (src, tgt, weight) in data.edges {
            graph.add_edge(NodeIndex::new(src), NodeIndex::new(tgt), weight);
        }

        Ok(Self { graph, node_map })
    }

    pub fn get_subgraph_nodes(&self, target: &str) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(&idx) = self.node_map.get(target) {
            result.push(self.graph[idx].id.clone());
            for neighbor in self.graph.neighbors(idx) {
                result.push(self.graph[neighbor].id.clone());
            }
        }
        result
    }
}
