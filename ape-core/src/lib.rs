use std::path::{Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use similar::TextDiff;
use uuid::Uuid;

pub use crate::config::Config;
pub use crate::error::Error;
pub use crate::llm::Prompt;
use crate::{
    edit::Edit,
    state::{MacroState, MacroStatus, list_recorded_macros},
};

mod config;
mod edit;
mod error;
mod git;
mod llm;
mod state;

fn ape_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Problem resolving home dir")
        .join(".ape")
}

/// Generate a diff in standard format
fn generate_diff(old: &str, new: &str, a_file: &str, b_file: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    diff.unified_diff()
        .context_radius(3)
        .header(&format!("a/{}", a_file), &format!("b/{}", b_file))
        .to_string()
}

#[derive(Serialize)]
pub struct ProposedChange {
    pub id: Uuid,
    pub diff_b64: String,
}

pub fn start_recording(
    file_path: &Path,
    repo_path: Option<&Path>,
    name: Option<&str>,
) -> Result<Uuid, Error> {
    let state = MacroState::new(file_path, repo_path, name)?;
    state.flush()?;
    Ok(state.id)
}

pub fn stop_recording(id: &Uuid) -> Result<(), Error> {
    let mut state = MacroState::load(id)?;
    let macro_status = state.macro_status();
    if matches!(macro_status, MacroStatus::Recorded) {
        return Err(Error::MacroAlreadyRecorded);
    }
    let original = state.original_file_contents()?;
    let current = state.current_file_contents()?;
    if original == current {
        return Err(Error::NoChanges);
    }
    let file_name = state.original_file_name();
    let diff = generate_diff(&original, &current, &file_name, &file_name);
    state.add_diff(diff)?;
    state.flush()?;
    Ok(())
}

pub fn cancel_recording(id: &Uuid) -> Result<(), Error> {
    let state = MacroState::load(id)?;
    let macro_status = state.macro_status();
    if matches!(macro_status, MacroStatus::Recorded) {
        return Err(Error::MacroAlreadyRecorded);
    }
    state.delete();
    Ok(())
}

/// Creates a macro from the git diff
pub fn create_macro(
    file_path: &Path,
    repo_path: Option<&Path>,
    name: Option<&str>,
    staged: bool,
) -> Result<Uuid, Error> {
    let state = MacroState::new_from_git_diff(file_path, repo_path, name, staged)?;
    state.flush()?;
    Ok(state.id)
}

pub async fn execute_macro(
    config: &Config,
    id: &Uuid,
    file_path: &Path,
    user_message: Option<&str>,
) -> Result<ProposedChange, Error> {
    let state = MacroState::load(id)?;
    let diff_file = state.diff_file();
    let edit = llm::send(config, file_path, &diff_file, user_message).await?;
    let diff = edit.diff(file_path)?;
    let change = ProposedChange {
        id: Uuid::new_v4(),
        diff_b64: STANDARD.encode(diff.as_bytes()),
    };
    Ok(change)
}

pub fn execute_macro_sampling_prompt(
    id: &Uuid,
    file_path: &Path,
    user_message: Option<&str>,
) -> Result<Prompt, Error> {
    let state = MacroState::load(id)?;
    let diff_file = state.diff_file();
    let prompt = Prompt::new(file_path, &diff_file, user_message)?;
    Ok(prompt)
}

pub fn process_execute_macro_sampling_response(
    file_path: &Path,
    llm_response: &str,
) -> Result<ProposedChange, Error> {
    let edit = Edit::try_from(llm_response)?;
    let diff = edit.diff(file_path)?;
    let change = ProposedChange {
        id: Uuid::new_v4(),
        diff_b64: STANDARD.encode(diff.as_bytes()),
    };
    Ok(change)
}

pub fn set_macro_name(id: &Uuid, name: &str) -> Result<(), Error> {
    let mut state = MacroState::load(id)?;
    state.set_name(name);
    state.flush()?;
    Ok(())
}

#[derive(Serialize)]
pub struct RecordedMacro {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    file_path: PathBuf,
    repo_path: PathBuf,
}

pub fn list_macros(repo_path: Option<&Path>) -> Result<Vec<RecordedMacro>, Error> {
    let macros = list_recorded_macros(repo_path)?
        .into_iter()
        .map(|(id, metadata)| RecordedMacro {
            id,
            name: metadata.name,
            file_path: metadata.file_path,
            repo_path: metadata.repo_path,
        })
        .collect();
    Ok(macros)
}
