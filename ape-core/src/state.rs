use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

#[derive(Serialize, Deserialize)]
enum MacroStatus {
    Recording,
    Recorded,
}

#[derive(Serialize, Deserialize)]
struct MacroMetadata {
    file_path: PathBuf,
    repo_path: PathBuf,
    status: MacroStatus,
}

fn state_dir_path(id: &Uuid) -> PathBuf {
    PathBuf::from("~/.ape").join(id.to_string())
}

fn find_git_root(file_path: &Path) -> Option<PathBuf> {
    let mut current_path = file_path.canonicalize().ok()?;
    // If it's a file, start with its parent directory
    if current_path.is_file() {
        current_path.pop();
    }
    loop {
        let git_dir = current_path.join(".git");
        if fs::metadata(&git_dir).is_ok() {
            return Some(current_path);
        }
        // Move up to the parent directory
        if !current_path.pop() {
            break; // Reached root, no .git found
        }
    }
    None
}

pub struct MacroState {
    pub(crate) id: Uuid,
    dir_path: PathBuf,
    metadata: MacroMetadata,
}

impl MacroState {
    pub fn new(file_path: &Path, opt_repo_path: Option<&Path>) -> Result<Self, Error> {
        let repo_path = match opt_repo_path {
            Some(p) => {
                if !file_path.starts_with(p) {
                    return Err(Error::InvalidRepoPath(p.to_path_buf()));
                }
                p.to_path_buf()
            }
            None => find_git_root(file_path).ok_or(Error::RepoNotFound(file_path.to_path_buf()))?,
        };

        // Generate an id
        let id = Uuid::new_v4();

        // Create the directory
        let dir_path = state_dir_path(&id);

        // SAFE to use unwrap because of the checks around repo_path above
        let file_name = file_path.file_name().unwrap();
        fs::create_dir_all(&dir_path)?;
        fs::copy(file_path, dir_path.join(file_name))?;

        let metadata = MacroMetadata {
            file_path: file_path.to_path_buf(),
            repo_path,
            status: MacroStatus::Recording,
        };
        Ok(Self {
            id,
            dir_path,
            metadata,
        })
    }

    pub fn load(id: &Uuid) -> Result<Self, Error> {
        let dir_path = state_dir_path(id);
        let metadata_file_path = dir_path.join("metadata.json");
        match fs::exists(&metadata_file_path) {
            Ok(true) => {}
            _ => return Err(Error::MacroNotFound(id.clone())),
        }
        let file = File::open(metadata_file_path)?;
        let reader = BufReader::new(file);
        let metadata: MacroMetadata = serde_json::from_reader(reader)?;
        Ok(Self {
            id: id.clone(),
            dir_path,
            metadata,
        })
    }

    fn original_file(&self) -> PathBuf {
        let file_name = self.metadata.file_path.file_name().unwrap();
        self.dir_path.join(file_name)
    }

    pub fn original_file_contents(&self) -> Result<String, Error> {
        let contents = fs::read_to_string(self.original_file())?;
        Ok(contents)
    }

    pub fn current_file(&self) -> &Path {
        &self.metadata.file_path
    }

    pub fn current_file_contents(&self) -> Result<String, Error> {
        let contents = fs::read_to_string(self.current_file())?;
        Ok(contents)
    }

    pub fn diff_file(&self) -> PathBuf {
        self.dir_path.join("changes.diff")
    }

    pub fn add_diff(&mut self, diff: String) -> Result<(), Error> {
        fs::write(self.diff_file(), diff)?;
        self.metadata.status = MacroStatus::Recorded;
        Ok(())
    }

    pub fn flush(&self) -> Result<(), Error> {
        let metadata_file_path = self.dir_path.join("metadata.json");
        let file = File::create(metadata_file_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.metadata)?;
        Ok(())
    }
}
