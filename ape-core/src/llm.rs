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
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum Model {
    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,
    #[serde(rename = "gpt-5-mini")]
    #[default]
    Gpt5Mini,
    #[serde(rename = "gpt-5.4")]
    Gpt5_4,
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
            Self::Gpt5Nano | Self::Gpt5Mini | Self::Gpt5_4 => Provider::OpenAI,
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
a single json map with fields:
- file
- start_line
- end_line
- replacement (array of lines)

Important notes:
* Do not return the entire file. Only include lines that change.
* No need to include any explanation. Just return the json so that it can be parsed.
* Even if the changes are spread across different parts of the file, return a single json map in the above format.
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
        Provider::Claude => claude::send_message(config.model(), config.api_key(), prompt).await,
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

        llm_response.ok_or(Error::LLMResponseFormat)
    }
}

mod claude {
    use secret_string::SecretString;
    use serde::{Deserialize, Serialize};

    use crate::{Error, edit::Edit};

    use super::{Model, Prompt, clean_json};

    #[derive(Serialize)]
    struct Message {
        role: &'static str,
        content: String,
    }

    #[derive(Serialize)]
    struct RequestBody {
        model: String,
        max_tokens: u32,
        system: String,
        messages: Vec<Message>,
    }

    #[derive(Deserialize)]
    struct ContentBlock {
        text: String,
    }

    #[derive(Deserialize)]
    struct Response {
        content: Vec<ContentBlock>,
    }

    pub async fn send_message(
        model: &Model,
        api_key: &SecretString<String>,
        prompt: Prompt,
    ) -> Result<Edit, Error> {
        let body = RequestBody {
            model: model.to_string(),
            max_tokens: 1024,
            system: prompt.system,
            messages: vec![Message {
                role: "user",
                content: prompt.user,
            }],
        };

        let response = reqwest::Client::new()
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key.value())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::LLMResponse(status.as_u16(), text));
        }

        let response: Response = response.json().await?;

        let raw_text = response
            .content
            .into_iter()
            .next()
            .ok_or(Error::LLMResponseFormat)?
            .text;

        let json_str = clean_json(&raw_text);
        // println!("Json string = {json_str}");

        let edit = serde_json::from_str::<Edit>(json_str)?;

        Ok(edit)
    }
}
