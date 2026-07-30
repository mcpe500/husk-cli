use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub session_id: String,
    pub timestamp: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    pub codebase_subgraph_size: usize,
    pub final_status: String,
}

pub struct TelemetryLogger;

impl TelemetryLogger {
    pub fn log_run(
        prompt: &str,
        provider: &str,
        model: &str,
        subgraph_size: usize,
        status: &str,
    ) -> Result<()> {
        let dir = PathBuf::from(".husk/telemetry");
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }

        let record = TelemetryRecord {
            session_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            prompt: prompt.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            codebase_subgraph_size: subgraph_size,
            final_status: status.to_string(),
        };

        let file_path = dir.join(format!("harvest_{}.jsonl", Utc::now().format("%Y%m%d")));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        let json = serde_json::to_string(&record)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }
}
