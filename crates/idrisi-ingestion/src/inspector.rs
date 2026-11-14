//! Structural inspection of OSM PBF inputs using the pinned osmium-tool binary.
//!
//! The extended fileinfo mode reads the complete input, rather than trusting
//! only the PBF header. Keep osmium-tool version pinned in the worker image.

use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

#[derive(Debug, Clone)]
pub struct InspectionPolicy {
    pub executable: PathBuf,
    pub deadline: Duration,
    pub max_json_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PbfInspection {
    pub file_size_bytes: u64,
    pub node_count: u64,
    pub way_count: u64,
    pub relation_count: u64,
    pub data_bbox: Option<Value>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    pub replication_timestamp: Option<String>,
    pub replication_sequence: Option<u64>,
    /// Retain the bounded tool report for provenance and diagnostics.
    pub report: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum InspectionError {
    #[error("inspection policy is invalid")]
    InvalidPolicy,
    #[error("input file does not exist or is not a regular file")]
    InvalidInput,
    #[error("could not start osmium fileinfo: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("osmium fileinfo exceeded its deadline")]
    TimedOut,
    #[error("osmium fileinfo failed with exit code {0:?}")]
    ToolFailed(Option<i32>),
    #[error("osmium fileinfo output exceeded the configured limit")]
    OutputTooLarge,
    #[error("osmium fileinfo returned invalid JSON")]
    InvalidJson,
    #[error("input is not an OSM PBF file")]
    NotPbf,
    #[error("PBF contains no OSM objects")]
    EmptyPbf,
    #[error("PBF inspection report is missing required fields")]
    MissingMetadata,
}

pub async fn inspect(
    path: &Path,
    policy: &InspectionPolicy,
) -> Result<PbfInspection, InspectionError> {
    if policy.executable.as_os_str().is_empty()
        || policy.deadline.is_zero()
        || policy.max_json_bytes == 0
    {
        return Err(InspectionError::InvalidPolicy);
    }
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| InspectionError::InvalidInput)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(InspectionError::InvalidInput);
    }

    let mut child = Command::new(&policy.executable)
        .arg("fileinfo")
        .arg("--extended")
        .arg("--json")
        .arg("--no-progress")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(InspectionError::Spawn)?;

    let inspection = timeout(policy.deadline, async {
        let stdout = child.stdout.take().ok_or(InspectionError::InvalidJson)?;
        let mut reader = stdout.take(policy.max_json_bytes.saturating_add(1) as u64);
        let mut bytes = Vec::with_capacity(policy.max_json_bytes.min(64 * 1024));
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(InspectionError::Spawn)?;
        drop(reader);
        if bytes.len() > policy.max_json_bytes {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(InspectionError::OutputTooLarge);
        }
        let status = child.wait().await.map_err(InspectionError::Spawn)?;
        if !status.success() {
            return Err(InspectionError::ToolFailed(status.code()));
        }
        let report: Value =
            serde_json::from_slice(&bytes).map_err(|_| InspectionError::InvalidJson)?;
        parse_report(metadata.len(), report)
    })
    .await;

    match inspection {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(InspectionError::TimedOut)
        }
    }
}

fn parse_report(file_size_bytes: u64, report: Value) -> Result<PbfInspection, InspectionError> {
    let file = report.get("file").ok_or(InspectionError::MissingMetadata)?;
    let format = file
        .get("format")
        .and_then(Value::as_str)
        .ok_or(InspectionError::MissingMetadata)?;
    if !format.eq_ignore_ascii_case("PBF") {
        return Err(InspectionError::NotPbf);
    }

    let data = report.get("data").ok_or(InspectionError::MissingMetadata)?;
    let counts = data.get("count").ok_or(InspectionError::MissingMetadata)?;
    let node_count = counts
        .get("nodes")
        .and_then(Value::as_u64)
        .ok_or(InspectionError::MissingMetadata)?;
    let way_count = counts
        .get("ways")
        .and_then(Value::as_u64)
        .ok_or(InspectionError::MissingMetadata)?;
    let relation_count = counts
        .get("relations")
        .and_then(Value::as_u64)
        .ok_or(InspectionError::MissingMetadata)?;
    if node_count.saturating_add(way_count).saturating_add(relation_count) == 0 {
        return Err(InspectionError::EmptyPbf);
    }

    let header = report.get("header");
    let header_option = |key: &str| -> Option<&Value> {
        header?.get("option")?.get(key)
    };

    Ok(PbfInspection {
        file_size_bytes,
        node_count,
        way_count,
        relation_count,
        data_bbox: data.get("bbox").cloned(),
        first_timestamp: data
            .get("timestamp")
            .and_then(|v| v.get("first"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        last_timestamp: data
            .get("timestamp")
            .and_then(|v| v.get("last"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        replication_timestamp: header_option("osmosis_replication_timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned),
        replication_sequence: header_option("osmosis_replication_sequence_number")
            .and_then(Value::as_u64),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(format: &str, nodes: u64, ways: u64, relations: u64) -> Value {
        serde_json::json!({
            "file": {"format": format},
            "header": {"option": {
                "osmosis_replication_timestamp": "2026-11-14T00:00:00Z",
                "osmosis_replication_sequence_number": 123
            }},
            "data": {
                "count": {"nodes": nodes, "ways": ways, "relations": relations},
                "bbox": [[106.7, -6.9], [107.0, -6.7]],
                "timestamp": {"first": "2026-11-13T00:00:00Z", "last": "2026-11-14T00:00:00Z"}
            }
        })
    }

    #[test]
    fn parses_valid_pbf_metadata() {
        let parsed = parse_report(4096, report("PBF", 100, 20, 3)).unwrap();
        assert_eq!(parsed.file_size_bytes, 4096);
        assert_eq!(parsed.node_count, 100);
        assert_eq!(parsed.way_count, 20);
        assert_eq!(parsed.relation_count, 3);
        assert_eq!(parsed.replication_sequence, Some(123));
        assert!(parsed.data_bbox.is_some());
    }

    #[test]
    fn rejects_non_pbf_input() {
        assert!(matches!(
            parse_report(10, report("XML", 1, 1, 0)),
            Err(InspectionError::NotPbf)
        ));
    }

    #[test]
    fn rejects_empty_pbf() {
        assert!(matches!(
            parse_report(10, report("PBF", 0, 0, 0)),
            Err(InspectionError::EmptyPbf)
        ));
    }

    #[test]
    fn rejects_missing_counts() {
        let invalid = serde_json::json!({"file": {"format": "PBF"}, "data": {}});
        assert!(matches!(
            parse_report(10, invalid),
            Err(InspectionError::MissingMetadata)
        ));
    }
}
