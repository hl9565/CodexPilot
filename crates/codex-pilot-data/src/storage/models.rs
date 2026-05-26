use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRef {
    pub id: String,
    pub title: Option<String>,
}

impl SessionRef {
    pub fn new(id: impl Into<String>, title: impl Into<Option<String>>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
        }
    }

    pub fn normalized_id(&self) -> String {
        normalize_session_id(&self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeleteStatus {
    Deleted,
    Undone,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResult {
    pub status: DeleteStatus,
    pub session_id: String,
    pub message: String,
    pub undo_token: Option<String>,
    pub backup_path: Option<PathBuf>,
}

impl DeleteResult {
    pub fn deleted(&self) -> bool {
        self.status == DeleteStatus::Deleted
    }
}

pub(crate) fn normalize_session_id(session_id: &str) -> String {
    session_id
        .strip_prefix("local:")
        .unwrap_or(session_id)
        .to_string()
}

pub(super) fn deleted(session_id: &str, token: String, backup_path: PathBuf) -> DeleteResult {
    DeleteResult {
        status: DeleteStatus::Deleted,
        session_id: session_id.to_string(),
        message: "已删除本地会话".to_string(),
        undo_token: Some(token),
        backup_path: Some(backup_path),
    }
}

pub(super) fn not_found(session_id: &str, message: &str) -> DeleteResult {
    DeleteResult {
        status: DeleteStatus::NotFound,
        session_id: session_id.to_string(),
        message: message.to_string(),
        undo_token: None,
        backup_path: None,
    }
}

pub(super) fn failed(session_id: &str, message: impl Into<String>) -> DeleteResult {
    failed_with_backup(session_id, message, None, None)
}

pub(super) fn failed_with_backup(
    session_id: &str,
    message: impl Into<String>,
    backup_path: Option<PathBuf>,
    undo_token: Option<String>,
) -> DeleteResult {
    DeleteResult {
        status: DeleteStatus::Failed,
        session_id: session_id.to_string(),
        message: message.into(),
        undo_token,
        backup_path,
    }
}
