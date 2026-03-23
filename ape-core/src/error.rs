use std::{io, path::PathBuf};

use async_openai::error::OpenAIError;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Macro not found: {0}")]
    MacroNotFound(Uuid),
    #[error("Macro already recorded")]
    MacroAlreadyRecorded,
    #[error("No changes detected")]
    NoChanges,
    #[error("Invalid repo path: {0}")]
    InvalidRepoPath(PathBuf),
    #[error("Could not find repo for file path: {0}")]
    RepoNotFound(PathBuf),
    #[error("Serde Json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("OpenAI error: {0}")]
    OpenAI(#[from] OpenAIError),
    #[error("Failed to load config: {0}")]
    Config(String),
    #[error("Credential not set in env var: {0}")]
    Credential(String),
    #[error("LLM Http response failed: [{0}] {1}")]
    LLMResponse(u16, String),
    #[error("Failed to obtain LLM response in expected format")]
    LLMResponseFormat,
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}
