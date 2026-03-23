use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{Error, config::Config, edit::Edit};

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
        let value = serde_json::to_value(self).unwrap();
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
instructions in the user message that follows. Return the "edit" as
json with fields:
- file
- start_line
- end_line
- replacement (array of lines)

Do not return the entire file. Only include lines that change.
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

pub async fn send(
    config: &Config,
    curr_file: &Path,
    diff_file: &Path,
    user_message: Option<&str>,
) -> Result<Edit, Error> {
    let prompt = make_prompt(curr_file, diff_file, user_message)?;
    match config.provider() {
        Provider::OpenAI => openai::send_message(config.model(), config.api_key(), prompt).await,
        Provider::Claude => unimplemented!(),
    }
}

mod openai {
    use async_openai::{
        Client,
        config::OpenAIConfig,
        types::responses::{
            AssistantRole, CreateResponseArgs, EasyInputContent, EasyInputMessage, InputItem,
            InputParam, MessageType, OutputItem, OutputMessageContent, OutputStatus, Role,
        },
    };
    use secret_string::SecretString;

    use crate::{Error, edit::Edit};

    use super::{Model, Prompt, clean_json};

    pub async fn send_message(
        model: &Model,
        api_key: &SecretString<String>,
        prompt: Prompt,
    ) -> Result<Edit, Error> {
        let config = OpenAIConfig::new().with_api_key(api_key.value());
        let client = Client::with_config(config);
        let request = CreateResponseArgs::default()
            .model(model.to_string())
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

        let mut llm_response: Option<Edit> = None;

        for output in response.output {
            if let OutputItem::Message(output_message) = output
                && let (AssistantRole::Assistant, OutputStatus::Completed) =
                    (output_message.role, output_message.status)
            {
                if output_message.content.len() == 1 {
                    if let OutputMessageContent::OutputText(output_text) =
                        &output_message.content[0]
                    {
                        let cleaned_text = clean_json(&output_text.text);
                        llm_response = serde_json::from_str(cleaned_text)?;
                    }
                } else {
                    println!(
                        "Multiple content objects received in response: {:?}",
                        output_message.content
                    );
                }
            }
        }

        llm_response.ok_or(Error::LLMResponse)
    }
}
