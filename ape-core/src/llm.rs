use std::{fs, io, path::Path};

use rsllm::{ChatMessage, Client, MessageRole};
use serde::Deserialize;

use crate::{Error, config::Config};

fn make_prompt(
    curr_file: &Path,
    diff_file: &Path,
    user_message: Option<&str>,
) -> Result<Vec<ChatMessage>, io::Error> {
    let file_name = curr_file.file_name().unwrap().to_string_lossy();
    let src_code = fs::read_to_string(curr_file)?;
    let diff = fs::read_to_string(diff_file)?;

    let example_json_format = r#"{"diff": ["<diff1>", "<diff2>"]}"#;
    let sys_prompt = format!(
        r#"Go through the two files attached below (contents included inline):

File: {file_name}
-----------------
{src_code}

File: changes.diff
------------------
{diff}

changes.diff represents an example change made to the source code.
You have to make similar changes to the {file_name}. If there are
multiple changes to be made in the file, return each diff separately
in the following json structure.

```
{example_json_format}
```

Note that the response format must be json string like above.
"#
    );
    let mut messages = vec![ChatMessage::new(MessageRole::System, sys_prompt)];
    if let Some(msg) = user_message {
        messages.push(ChatMessage::new(MessageRole::User, msg));
    }
    Ok(messages)
}

#[derive(Deserialize)]
struct DiffResponse {
    diffs: Vec<String>,
}

fn clean_json(s: &str) -> &str {
    s.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

pub async fn send(
    config: &Config,
    curr_file: &Path,
    diff_file: &Path,
    user_message: Option<&str>,
) -> Result<Vec<String>, Error> {
    let client = Client::builder()
        .provider(config.provider().clone())
        .api_key(config.api_key().value())
        .model(config.model())
        .build()?;
    let messages = make_prompt(curr_file, diff_file, user_message)?;
    let response = client.chat_completion(messages).await?;
    let cleaned = clean_json(&response.content);
    let parsed: DiffResponse = serde_json::from_str(cleaned)?;
    Ok(parsed.diffs)
}
