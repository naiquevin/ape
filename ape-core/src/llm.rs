use std::{fs, io, path::Path};

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::responses::{
        AssistantRole, CreateResponseArgs, EasyInputContent, EasyInputMessage, InputItem,
        InputParam, MessageType, OutputItem, OutputMessageContent, OutputStatus, Role,
    },
};
use secret_string::SecretString;
use serde::{Deserialize, Serialize};

use crate::{Error, config::Config};

/// Supported LLM providers
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Provider {
    /// OpenAI (GPT models)
    OpenAI,
    /// Anthropic Claude
    Claude,
}

/// Supported models
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Model {
    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,
    #[serde(rename = "gpt-5-mini")]
    Gpt5Mini,
    #[serde(rename = "claude-haiku-4-5")]
    Haiku4_5,
    #[serde(rename = "claude-sonnet-4-6")]
    Sonnet4_6,
    #[serde(rename = "claude-opus-4-6")]
    Opus4_6,
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let value = serde_json::to_value(&self).unwrap();
        let s = value.as_str().unwrap();
        write!(f, "{s}")
    }
}

impl Model {
    pub fn provider(&self) -> Provider {
        match &self {
            Self::Gpt5Nano | Self::Gpt5Mini => Provider::OpenAI,
            Self::Haiku4_5 | Self::Sonnet4_6 | Self::Opus4_6 => Provider::Claude,
        }
    }
}

struct Prompt {
    system: String,
    user: String,
}

fn make_prompt(
    curr_file: &Path,
    diff_file: &Path,
    user_message: Option<&str>,
) -> Result<Prompt, io::Error> {
    let file_name = curr_file.file_name().unwrap().to_string_lossy();
    let src_code = fs::read_to_string(curr_file)?;
    let diff = fs::read_to_string(diff_file)?;

    let example_json_format = r#"{"diff": "<..diff..>"}"#;
    let sys_prompt = format!(
        r#"Go through the two files attached below (contents included inline):

File: {file_name}
-----------------
{src_code}

File: changes.diff
------------------
{diff}

changes.diff represents an example change made to the source
code. Understand the change and make additional changes as per the
instructions in the user message that follows. Return the diff inside
a json string as follows,

```
{example_json_format}
```

Note that the response format must be json string exactly like above.
"#
    );
    let user_prompt = match user_message {
        Some(msg) => msg.to_string(),
        None => "Find all occurrences where a change similar to the example change can be made and do it".to_string(),
    };
    Ok(Prompt {
        system: sys_prompt,
        user: user_prompt,
    })
}

fn clean_json(s: &str) -> &str {
    s.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

#[derive(Deserialize)]
pub struct DiffResponse {
    pub diff: String,
}

async fn send_openai(
    model: &Model,
    api_key: &SecretString<String>,
    prompt: Prompt,
) -> Result<DiffResponse, Error> {
    let config = OpenAIConfig::new().with_api_key(api_key.value());
    let client = Client::with_config(config);
    let request = CreateResponseArgs::default()
        .model(&model.to_string())
        .input(InputParam::Items(vec![
            InputItem::EasyMessage(EasyInputMessage {
                r#type: MessageType::Message,
                role: Role::System,
                content: EasyInputContent::Text(prompt.system),
            }),
            InputItem::EasyMessage(EasyInputMessage {
                r#type: MessageType::Message,
                role: Role::User,
                content: EasyInputContent::Text(prompt.user),
            }),
        ]))
        .build()?;

    let response = client.responses().create(request).await?;

    let mut llm_response: Option<DiffResponse> = None;

    for output in response.output {
        if let OutputItem::Message(output_message) = output {
            if let (AssistantRole::Assistant, OutputStatus::Completed) =
                (output_message.role, output_message.status)
            {
                if output_message.content.len() == 1 {
                    if let OutputMessageContent::OutputText(output_text) =
                        &output_message.content[0]
                    {
                        let cleaned_text = clean_json(&output_text.text);
                        // @TODO: Remove unwrap
                        llm_response = serde_json::from_str(&cleaned_text).unwrap();
                    }
                } else {
                    println!(
                        "Multiple content objects received in response: {:?}",
                        output_message.content
                    );
                }
            }
        }
    }

    // @TODO: Handle errors
    Ok(llm_response.unwrap())
}

pub async fn send(
    config: &Config,
    curr_file: &Path,
    diff_file: &Path,
    user_message: Option<&str>,
) -> Result<DiffResponse, Error> {
    let prompt = make_prompt(curr_file, diff_file, user_message)?;
    match config.provider() {
        Provider::OpenAI => send_openai(config.model(), config.api_key(), prompt).await,
        Provider::Claude => unimplemented!(),
    }
}
