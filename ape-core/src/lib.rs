use std::path::{Path, PathBuf};

use diffy::create_patch;
use serde::Serialize;
use uuid::Uuid;

pub use crate::config::Config;
pub use crate::error::Error;
use crate::state::{MacroState, list_recorded_macros};

mod config;
mod error;
mod llm;
mod state;

fn ape_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Problem resolving home dir")
        .join(".ape")
}

#[derive(Serialize)]
pub struct ProposedChange {
    pub id: Uuid,
    pub diff: String,
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
    let diff = create_patch(&original, &current).to_string();
    state.add_diff(diff)?;
    state.flush()?;
    Ok(())
}

pub async fn execute_macro(
    config: &Config,
    id: &Uuid,
    user_message: Option<&str>,
) -> Result<ProposedChange, Error> {
    let state = MacroState::load(id)?;
    let curr_file = state.current_file();
    let diff_file = state.diff_file();
    let resp = llm::send(config, &curr_file, &diff_file, user_message).await?;
    let change = ProposedChange {
        id: Uuid::new_v4(),
        diff: resp.diff,
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
