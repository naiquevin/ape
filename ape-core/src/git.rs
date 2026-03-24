use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tracing::error;

use crate::Error;

pub fn find_git_root(file_path: &Path) -> Option<PathBuf> {
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

pub fn git_diff(file_path: &Path, repo_path: &Path, staged: bool) -> Result<Vec<u8>, Error> {
    let mut args = vec!["--no-pager", "diff", "--no-color"];
    if staged {
        args.push("--staged");
    }
    let output = Command::new("git")
        .args(args)
        .arg("--")
        .arg(file_path)
        .current_dir(Path::new(repo_path))
        .output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("git diff failed:\n{}", stderr);
        Err(Error::Git(stderr.into_owned()))
    }
}
