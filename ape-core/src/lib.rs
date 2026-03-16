use std::path::{Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use similar::TextDiff;
use uuid::Uuid;

pub use crate::config::Config;
pub use crate::error::Error;
use crate::state::{MacroState, list_recorded_macros};

mod config;
mod edit;
mod error;
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

pub fn start_recording(file_path: &Path, repo_path: Option<&Path>) -> Result<Uuid, Error> {
    let state = MacroState::new(file_path, repo_path)?;
    state.flush()?;
    Ok(state.id)
}

pub fn stop_recording(id: &Uuid) -> Result<(), Error> {
    let mut state = MacroState::load(id)?;
    let original = state.original_file_contents()?;
    let current = state.current_file_contents()?;
    let file_name = state.original_file_name();
    let diff = generate_diff(&original, &current, &file_name, &file_name);
    state.add_diff(diff)?;
    state.flush()?;
    Ok(())
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

/// Approve change does nothing for now besides printing a log
pub fn approve_change(id: &Uuid, change_id: &Uuid) {
    // Replace with logs
    println!("[Macro: {id}] Change id was approved: {change_id}");
}

/// Reject change does nothing for now besides printing a log
pub fn reject_change(id: &Uuid, change_id: &Uuid) {
    println!("[Macro: {id}] Change id was rejected: {change_id}");
}

pub fn list_macros(repo_path: Option<&Path>) -> Result<Vec<Uuid>, Error> {
    Ok(list_recorded_macros(repo_path)?)
}
