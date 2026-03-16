use std::{io, path::PathBuf};

use async_openai::error::OpenAIError;
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
    #[error("OpenAI error: {0}")]
    OpenAI(#[from] OpenAIError),
    #[error("Not configured")]
    NotConfigured,
    #[error("Failed to load config: {0}")]
    Config(String),
    #[error("Credential not set in env var: {0}")]
    Credential(String),
}
