use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum ManagerError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Internal(String),
    #[error("{message}")]
    WithRecoveryPoint { message: String, recovery_dir: String },
}
