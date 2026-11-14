//! Bounded structural inspection of OpenStreetMap PBF artifacts via osmium-tool.
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use thiserror::Error;
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

#[derive(Debug, Clone)]
pub struct InspectionPolicy {
    pub executable: String,
    pub deadline: Duration,
    pub max_json_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct PbfInspection {
    pub file_size_bytes: u64,
    pub nodes: u64,
    pub ways: u64,
    pub relations: u64,
    pub bbox: Option<Value>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    pub replication_timestamp: Option<String>,
    pub replication_sequence: Option<u64>,
}

#[derive(Debug, Error)]
pub enum InspectionError {
    #[error("inspection policy is invalid")]
    InvalidPolicy,
    #[error("input PBF path is not a regular non-empty file")]
    InvalidInput,
    #[error("could not start osmium fileinfo: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("osmium fileinfo exceeded its deadline")]
    TimedOut,
    #[error("osmium fileinfo returned a non-zero status")]
    ToolFailed,
    #[error("osmium JSON report exceeded the configured size limit")]
    OutputTooLarge,
    #[error("osmium returned invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("input is not an OSM PBF file")]
    NotPbf,
    #[error("PBF contains no OSM objects")]
    EmptyPbf,
    #[error("PBF report omitted required feature counts")]
    MissingCounts,
}

pub async fn inspect(
    path: &Path,
    policy: &InspectionPolicy,
) -> Result<PbfInspection, InspectionError> {
    if policy.executable.trim().is_empty()
        || policy.deadline.is_zero()
        || policy.max_json_bytes == 0
    {
        return Err(InspectionError::InvalidPolicy);
    }
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(InspectionError::InvalidInput);
    }

    let mut child = Command::new(&policy.executable)
        .args(["fileinfo", "--extended", "--json", "--no-progress"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let mut stdout = child.stdout.take().ok_or(InspectionError::ToolFailed)?;
    let limit = policy.max_json_bytes.saturating_add(1);
    let read = async {
        let mut output = Vec::with_capacity(limit.min(64 * 1024));
        stdout.take(limit as u64).read_to_end(&mut output).await?;
        Ok::<_, std::io::Error>(output)
    };
    let output = match timeout(policy.deadline, read).await {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(InspectionError::TimedOut);
        }
    };
    if output.len() > policy.max_json_bytes {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(InspectionError::OutputTooLarge);
    }
    let status = match timeout(policy.deadline, child.wait()).await {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(InspectionError::TimedOut);
        }
    };
    if !status.success() {
        return Err(InspectionError::ToolFailed);
    }

    let report: Value = serde_json::from_slice(&output)?;
    if report.pointer("/file/format").and_then(Value::as_str) != Some("PBF") {
        return Err(InspectionError::NotPbf);
    }
    let counts = report.pointer("/data/count").ok_or(InspectionError::MissingCounts)?;
    let nodes = counts.get("nodes").and_then(Value::as_u64).ok_or(InspectionError::MissingCounts)?;
    let ways = counts.get("ways").and_then(Value::as_u64).ok_or(InspectionError::MissingCounts)?;
    let relations = counts.get("relations").and_then(Value::as_u64).ok_or(InspectionError::MissingCounts)?;
    if nodes.saturating_add(ways).saturating_add(relations) == 0 {
        return Err(InspectionError::EmptyPbf);
    }

    Ok(PbfInspection {
        file_size_bytes: metadata.len(),
        nodes,
        ways,
        relations,
        bbox: report.pointer("/data/bbox").cloned(),
        first_timestamp: report.pointer("/data/timestamp/first").and_then(Value::as_str).map(str::to_owned),
        last_timestamp: report.pointer("/data/timestamp/last").and_then(Value::as_str).map(str::to_owned),
        replication_timestamp: report.pointer("/header/option/osmosis_replication_timestamp").and_then(Value::as_str).map(str::to_owned),
        replication_sequence: report.pointer("/header/option/osmosis_replication_sequence").and_then(Value::as_u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_required_counts_and_optional_metadata() {
        let report = json!({
            "file": {"format": "PBF"},
            "data": {"count": {"nodes": 10, "ways": 4, "relations": 1},
                     "bbox": {"left": 95.0, "right": 141.0}},
            "header": {"option": {"osmosis_replication_sequence": 123}}
        });
        assert_eq!(report.pointer("/data/count/nodes").and_then(Value::as_u64), Some(10));
        assert_eq!(report.pointer("/header/option/osmosis_replication_sequence").and_then(Value::as_u64), Some(123));
    }

    #[test]
    fn rejects_non_pbf_report() {
        let report = json!({"file": {"format": "XML"}});
        assert_ne!(report.pointer("/file/format").and_then(Value::as_str), Some("PBF"));
    }
}
