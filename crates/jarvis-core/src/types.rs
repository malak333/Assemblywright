use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type JarvisResult<T> = Result<T, JarvisError>;

#[derive(Debug, thiserror::Error)]
pub enum JarvisError {
    #[error("blocked by policy: {0}")]
    PolicyBlocked(String),
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("plugin error: {0}")]
    Plugin(String),
    #[error("model error: {0}")]
    Model(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Workspace,
    Personal,
    Private,
    CredentialAdjacent,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    Low,
    Notify,
    Confirm,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    NotRequired,
    Pending,
    Approved,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Created,
    Running,
    WaitingForApproval,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_input: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub task_id: Option<Uuid>,
    pub event_type: String,
    pub summary: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl AuditEntry {
    pub fn new(
        task_id: Option<Uuid>,
        event_type: impl Into<String>,
        summary: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            event_type: event_type.into(),
            summary: summary.into(),
            payload,
            created_at: Utc::now(),
        }
    }
}
