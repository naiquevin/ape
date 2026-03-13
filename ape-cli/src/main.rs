use std::{path::PathBuf, process};

use ape_core::{
    Config, approve_change, execute_macro, list_macros, reject_change, start_recording,
    stop_recording,
};
use clap::{Parser, Subcommand};
use env_logger::WriteStyle;
use log::LevelFilter;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

fn init_logging(verbosity: u8) {
    let level = match verbosity {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    env_logger::Builder::new()
        .filter(None, level)
        .write_style(WriteStyle::Always)
        .init();
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("cli error: {0}")]
    Cli(String),
    #[error("ape error: {0}")]
    Core(#[from] ape_core::Error),
    #[error("Serde Json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Start recording")]
    Start {
        /// Path to the file in which changes are to be recorded. Must
        /// be an absolute path
        file_path: PathBuf,
        /// Explicitly specified repository root
        ///
        /// In most cases, the repository root can be inferred from
        /// the file path. But in certain cases it's not possible so
        /// it helps to have it optionally specified for e.g. when
        /// working with files outside a git repository or when there
        /// are git submodules inside a main repo.
        #[arg(long, help = "Explicitly specified repository root")]
        repo_path: Option<PathBuf>,
    },
    #[command(about = "Stop recording")]
    Stop {
        id: Uuid,
    },
    Execute {
        id: Uuid,
        #[arg(long, help = "Additional message from the user")]
        user_msg: Option<String>,
    },
    Approve {
        id: Uuid,
        diff_id: Uuid,
    },
    Reject {
        id: Uuid,
        diff_id: Uuid,
    },
    List {
        #[arg(long, help = "Filter recordings from this repo only")]
        repo_path: Option<PathBuf>,
    },
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, global = true, action = clap::ArgAction::Count, help = "Verbosity level (can be specified multiple times)")]
    verbosity: u8,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Serialize)]
struct ListResult {
    ids: Vec<Uuid>,
}

enum CliResponse {
    Success(serde_json::Value),
    #[allow(unused)]
    Failure(String),
}

impl Default for CliResponse {
    fn default() -> Self {
        Self::Success(serde_json::Value::Null)
    }
}

impl Cli {
    async fn execute(&self) -> Result<CliResponse, Error> {
        init_logging(self.verbosity);
        let config = Config::load()?;
        match &self.command {
            Some(Command::Start {
                file_path,
                repo_path,
            }) => {
                let id = start_recording(file_path, repo_path.as_deref())?;
                Ok(CliResponse::Success(json!({ "id": id  })))
            }
            Some(Command::Stop { id }) => {
                stop_recording(id)?;
                Ok(CliResponse::default())
            }
            Some(Command::Execute { id, user_msg }) => {
                let diff = execute_macro(&config, id, user_msg.as_deref()).await?;
                let resp = serde_json::to_value(diff)?;
                Ok(CliResponse::Success(resp))
            }
            Some(Command::Approve { id, diff_id }) => {
                approve_change(id, diff_id);
                Ok(CliResponse::default())
            }
            Some(Command::Reject { id, diff_id }) => {
                reject_change(id, diff_id);
                Ok(CliResponse::default())
            }
            Some(Command::List { repo_path }) => {
                let ids = list_macros(repo_path.as_deref())?;
                let resp = serde_json::to_value(ListResult { ids })?;
                Ok(CliResponse::Success(resp))
            }
            None => Err(Error::Cli("Please specify the command".to_string())),
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = cli.execute().await;
    match result {
        Ok(CliResponse::Success(json)) => {
            println!("{}", serde_json::to_string(&json).unwrap());
            process::exit(0);
        }
        Ok(CliResponse::Failure(msg)) => {
            eprintln!("{msg}");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}
