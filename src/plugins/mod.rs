use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub hooks: HashMap<String, String>,
}

#[allow(dead_code)]
pub struct PluginManager {
    pub skills: HashMap<String, Skill>,
    pub plugins: Vec<PluginManifest>,
}

#[allow(dead_code)]
impl PluginManager {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            plugins: Vec::new(),
        }
    }

    pub fn discover_all(&mut self) -> Result<()> {
        let local_plugins = PathBuf::from(".husk/plugins");
        if local_plugins.exists() {
            self.load_plugins_from_dir(&local_plugins)?;
        }
        Ok(())
    }

    pub fn load_plugins_from_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<()> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let manifest_path = entry.path().join("plugin.json");
                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content) {
                            self.plugins.push(manifest);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn list_skills(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }
}
