use std::{
    borrow::Cow,
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ape_dir,
    error::Error,
    git::{find_git_root, git_diff},
};

#[derive(Serialize, Deserialize)]
pub enum MacroStatus {
    Recording,
    Recorded,
}

#[derive(Serialize, Deserialize)]
pub struct MacroMetadata {
    pub file_path: PathBuf,
    pub repo_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    status: MacroStatus,
}

impl MacroMetadata {
    fn is_recorded(&self) -> bool {
        matches!(self.status, MacroStatus::Recorded)
    }
}

fn state_dir_path(id: &Uuid) -> PathBuf {
    ape_dir().join(id.to_string())
}

pub struct MacroState {
    pub(crate) id: Uuid,
    dir_path: PathBuf,
    metadata: MacroMetadata,
}

impl MacroState {
    pub fn new(
        file_path: &Path,
        opt_repo_path: Option<&Path>,
        name: Option<&str>,
    ) -> Result<Self, Error> {
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
            name: name.map(|s| s.to_owned()),
            status: MacroStatus::Recording,
        };
        Ok(Self {
            id,
            dir_path,
            metadata,
        })
    }

    pub fn new_from_git_diff(
        file_path: &Path,
        opt_repo_path: Option<&Path>,
        name: Option<&str>,
        staged: bool,
    ) -> Result<Self, Error> {
        let repo_path = match opt_repo_path {
            Some(p) => {
                if !file_path.starts_with(p) {
                    return Err(Error::InvalidRepoPath(p.to_path_buf()));
                }
                p.to_path_buf()
            }
            None => find_git_root(file_path).ok_or(Error::RepoNotFound(file_path.to_path_buf()))?,
        };

        // Obtain the diff first, so that if it fails, no macro id is
        // generated.
        let diff = git_diff(file_path, &repo_path, staged)?;

        if diff.is_empty() {
            return Err(Error::NoChanges);
        }

        // Generate an id
        let id = Uuid::new_v4();

        // Create the directory
        let dir_path = state_dir_path(&id);
        fs::create_dir_all(&dir_path)?;

        // Create the diff file
        let diff_file = dir_path.join("changes.diff");
        fs::write(diff_file, diff)?;

        let metadata = MacroMetadata {
            file_path: file_path.to_path_buf(),
            repo_path,
            name: name.map(|s| s.to_owned()),
            status: MacroStatus::Recorded,
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
            _ => return Err(Error::MacroNotFound(*id)),
        }
        let file = File::open(metadata_file_path)?;
        let reader = BufReader::new(file);
        let metadata: MacroMetadata = serde_json::from_reader(reader)?;
        Ok(Self {
            id: *id,
            dir_path,
            metadata,
        })
    }

    pub fn original_file_name(&self) -> Cow<'_, str> {
        self.metadata
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
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

    pub fn macro_status(&self) -> &MacroStatus {
        &self.metadata.status
    }

    pub fn add_diff(&mut self, diff: String) -> Result<(), Error> {
        fs::write(self.diff_file(), diff)?;
        self.metadata.status = MacroStatus::Recorded;
        Ok(())
    }

    pub fn set_name(&mut self, name: &str) {
        self.metadata.name = Some(name.to_string())
    }

    pub fn delete(self) {
        match self.dir_path.try_exists() {
            Ok(true) => {
                info!("Removing APE macro state: {}", self.id);
                fs::remove_dir_all(&self.dir_path).unwrap()
            }
            Ok(false) => {
                warn!(
                    "APE macro state dir doesn't exist: {}",
                    self.dir_path.display()
                );
            }
            Err(_) => {
                warn!(
                    "Couldn't check existence of dir: {}",
                    self.dir_path.display()
                )
            }
        }
    }

    pub fn flush(&self) -> Result<(), Error> {
        let metadata_file_path = self.dir_path.join("metadata.json");
        let file = File::create(metadata_file_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.metadata)?;
        Ok(())
    }
}

pub fn list_recorded_macros(
    repo_path: Option<&Path>,
) -> Result<Vec<(Uuid, MacroMetadata)>, io::Error> {
    let mut result = Vec::new();

    for entry in fs::read_dir(ape_dir())? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name();
        let id_str = match dir_name.to_str() {
            Some(name) => name,
            None => continue,
        };

        let id = match Uuid::parse_str(id_str) {
            Ok(u) => u,
            Err(_) => continue,
        };

        let metadata_path = path.join("metadata.json");
        if !metadata_path.exists() {
            continue;
        }

        let file = fs::File::open(metadata_path)?;
        let metadata: MacroMetadata = serde_json::from_reader(file)?;

        // Skip macros that are not recorded completely
        if !metadata.is_recorded() {
            continue;
        }

        // If repo_path is specified, filter by that too
        if repo_path.is_some_and(|p| metadata.repo_path != p) {
            continue;
        }

        result.push((id, metadata));
    }

    Ok(result)
}
