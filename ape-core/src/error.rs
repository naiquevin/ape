use std::{io, path::PathBuf};

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Macro not found: {0}")]
    MacroNotFound(Uuid),
    #[error("Invalid repo path: {0}")]
    InvalidRepoPath(PathBuf),
    #[error("Could not find repo for file path: {0}")]
    RepoNotFound(PathBuf),
    #[error("Serde Json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Rsllm error: {0}")]
    Rsllm(#[from] rsllm::error::RsllmError),
    #[error("Failed to load config: {0}")]
    Config(String),
}
