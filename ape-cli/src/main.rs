use std::{path::PathBuf, process};

use clap::{Parser, Subcommand};
use uuid::Uuid;

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

impl Cli {
    fn execute(&self) -> Result<i32, String> {
        println!("Subcommand called: {:?}", self.command);
        Ok(0)
    }
}

fn main() {
    let cli = Cli::parse();
    let result = cli.execute();
    match result {
        Ok(status) => process::exit(status),
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}
