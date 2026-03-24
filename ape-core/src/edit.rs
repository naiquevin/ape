use std::{fs, path::Path};

use serde::Deserialize;

use crate::{Error, generate_diff};

/// Cleans json string by removing markdown fences if present and
/// stripping any whitespaces.
fn clean_json(s: &str) -> &str {
    s.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

#[derive(Debug, Deserialize)]
pub struct Edit {
    #[allow(unused)]
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub replacement: Vec<String>,
}

impl TryFrom<&str> for Edit {
    type Error = serde_json::Error;

    fn try_from(json_str: &str) -> Result<Self, Self::Error> {
        let cleaned_text = clean_json(json_str);
        let inst = serde_json::from_str(cleaned_text)?;
        Ok(inst)
    }
}

impl Edit {
    fn apply(&self, original: &str) -> String {
        let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
        let start = self.start_line - 1;
        let end = self.end_line;
        lines.splice(start..end, self.replacement.clone());
        lines.join("\n") + "\n"
    }

    pub fn diff(&self, original_file: &Path) -> Result<String, Error> {
        let file_name = original_file.file_name().unwrap().to_string_lossy();
        let original = fs::read_to_string(original_file)?;
        let modified = self.apply(&original);
        Ok(generate_diff(&original, &modified, &file_name, &file_name))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::Edit;

    #[test]
    fn test_diff_from_edit() {
        let resp = r#"{"end_line":41,"file":"backups.py","id":"4defa09a-d6ba-4ed9-8cf1-7dd24b9729eb","replacement":["def backup_network_config():","    return backup_file(\"network_config.json\")","",""],"start_line":31}"#;
        let edit: Edit = serde_json::from_str(resp).unwrap();
        let diff = edit
            .diff(Path::new(
                "/home/vineet/code/ape/examples/python/backups.py",
            ))
            .unwrap();
        fs::write("/tmp/example2.diff", diff).unwrap();
    }
}
