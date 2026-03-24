use std::{path::PathBuf, process};

use ape_core::{
    Config, RecordedMacro, cancel_recording, create_macro, execute_macro, list_macros,
    set_macro_name, start_recording, stop_recording,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

fn init_logging(verbosity: u8) {
    let base_level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(base_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(true)
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
        #[arg(long, help = "Specify name for the macro")]
        name: Option<String>,
    },
    #[command(about = "Stop recording")]
    Stop {
        /// Macro id that was returned by the start command
        id: Uuid,
    },
    #[command(about = "Cancel recording")]
    Cancel {
        /// Macro id that was returned by the start command
        id: Uuid,
    },
    #[command(about = "Create a macro from git diff")]
    Create {
        /// Path to the file in which changes are to be
        /// considered. Must be an absolute path
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
        #[arg(long, help = "Specify name for the macro")]
        name: Option<String>,
        #[arg(
            long,
            default_value_t = false,
            help = "Consider staged changed for diff"
        )]
        staged: bool,
    },
    Execute {
        /// Macro id that was returned by the stop command
        id: Uuid,
        /// Path to the file on which the macro has to be executed
        file_path: PathBuf,
        #[arg(long, help = "Additional message from the user")]
        user_msg: Option<String>,
    },
    List {
        #[arg(long, help = "Filter recordings from this repo only")]
        repo_path: Option<PathBuf>,
    },
    #[command(about = "Set name for a macro")]
    SetName { id: Uuid, name: String },
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
    macros: Vec<RecordedMacro>,
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
                name,
            }) => {
                let id = start_recording(file_path, repo_path.as_deref(), name.as_deref())?;
                Ok(CliResponse::Success(json!({ "id": id  })))
            }
            Some(Command::Stop { id }) => {
                stop_recording(id)?;
                Ok(CliResponse::default())
            }
            Some(Command::Cancel { id }) => {
                cancel_recording(id)?;
                Ok(CliResponse::default())
            }
            Some(Command::Create {
                file_path,
                repo_path,
                staged,
                name,
            }) => {
                let id = create_macro(file_path, repo_path.as_deref(), name.as_deref(), *staged)?;
                Ok(CliResponse::Success(json!({ "id": id  })))
            }
            Some(Command::Execute {
                id,
                file_path,
                user_msg,
            }) => {
                let diff = execute_macro(&config, id, file_path, user_msg.as_deref()).await?;
                let resp = serde_json::to_value(diff)?;
                Ok(CliResponse::Success(resp))
            }
            Some(Command::List { repo_path }) => {
                let macros = list_macros(repo_path.as_deref())?;
                let resp = serde_json::to_value(ListResult { macros })?;
                Ok(CliResponse::Success(resp))
            }
            Some(Command::SetName { id, name }) => {
                set_macro_name(id, name)?;
                Ok(CliResponse::default())
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
