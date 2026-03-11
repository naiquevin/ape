use std::path::Path;

use diffy::create_patch;
use uuid::Uuid;

use crate::{config::Config, error::Error, state::MacroState};

mod config;
mod error;
mod llm;
mod state;

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
    user_message: Option<String>,
) -> Result<Vec<ProposedChange>, Error> {
    let state = MacroState::load(id)?;
    let curr_file = state.current_file();
    let diff_file = state.diff_file();
    let changes = llm::send(config, &curr_file, &diff_file, user_message.as_deref())
        .await?
        .into_iter()
        .map(|diff| ProposedChange {
            id: Uuid::new_v4(),
            diff,
        })
        .collect();
    Ok(changes)
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
