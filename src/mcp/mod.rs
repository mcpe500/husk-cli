use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[allow(dead_code)]
pub struct McpHost {
    pub servers: HashMap<String, McpServerConfig>,
}

#[allow(dead_code)]
impl McpHost {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    pub fn add_server(&mut self, name: String, command: String, args: Vec<String>) {
        self.servers.insert(
            name.clone(),
            McpServerConfig {
                name,
                command,
                args,
            },
        );
    }

    pub fn list_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.values().collect()
    }
}
