use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Interrupted,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::TimedOut | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub tool: String,
    /// A display-safe representation of argv for dashboards and webhooks.
    #[serde(default)]
    pub command: String,
    /// Private execution specification. Stored separately with restricted file
    /// permissions and never returned by the API or webhook.
    #[serde(skip_serializing, default)]
    pub argv: Vec<String>,
    pub state: JobState,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub timeout_seconds: u64,
    pub return_code: Option<i32>,
    pub error: Option<String>,
    #[serde(default)]
    pub webhook_configured: bool,
    /// Private delivery destination. Persisted with argv in command.json and
    /// never returned through public APIs or webhook payloads.
    #[serde(skip_serializing, default)]
    pub webhook_url: Option<String>,
    /// Operator-selected analysis files written outside the durable job tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analysis_artifacts: Vec<AnalysisArtifact>,
    /// A stream export can fail independently after the scanner process exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_export_error: Option<String>,
    #[serde(skip_serializing, default)]
    pub stdout_export_path: Option<PathBuf>,
    #[serde(skip_serializing, default)]
    pub stderr_export_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisArtifact {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitJob {
    pub tool: Option<String>,
    pub argv: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub stdout_export_path: Option<PathBuf>,
    #[serde(default)]
    pub stderr_export_path: Option<PathBuf>,
    #[serde(default)]
    pub analysis_artifacts: Vec<AnalysisArtifact>,
}

#[derive(Serialize)]
pub struct OutputPage {
    pub job_id: Uuid,
    pub stream: String,
    pub offset: u64,
    pub next_offset: u64,
    pub truncated: bool,
    pub data: String,
}

#[derive(Debug, Serialize)]
pub struct JobArchivePreview {
    pub older_than_minutes: u64,
    pub cutoff: DateTime<Utc>,
    pub matched: usize,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct JobArchiveFailure {
    pub job_id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct JobArchiveResult {
    pub older_than_minutes: u64,
    pub cutoff: DateTime<Utc>,
    pub matched: usize,
    pub archived: usize,
    pub failed: usize,
    pub bytes_archived: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_file: Option<String>,
    pub failures: Vec<JobArchiveFailure>,
}

#[derive(Debug, Serialize)]
pub struct Health {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub queued: usize,
    pub running: usize,
    pub max_concurrency: usize,
}
