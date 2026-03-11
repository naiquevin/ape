use std::fs;

use rsllm::Provider;
use secret_string::SecretString;
use serde::Deserialize;

use crate::{Error, ape_dir};

#[derive(Deserialize)]
struct Settings {
    provider: rsllm::Provider,
    // @TODO: Validation model against provider as part of
    // deserialization
    model: String,
}

struct Credentials {
    api_key: SecretString<String>,
}

fn read_secret(name: &str) -> SecretString<String> {
    SecretString::new(std::env::var(name).unwrap())
}

pub struct Config {
    settings: Settings,
    creds: Credentials,
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        let config_file = ape_dir().join("config.json");
        match fs::exists(&config_file) {
            Ok(true) => {}
            Ok(false) => return Err(Error::NotConfigured),
            Err(e) => return Err(Error::Config(e.to_string())),
        };
        let json = fs::read_to_string(config_file).map_err(|e| Error::Config(e.to_string()))?;
        let settings: Settings =
            serde_json::from_str(&json).map_err(|e| Error::Config(e.to_string()))?;
        let api_key_var = match &settings.provider {
            rsllm::Provider::OpenAI => "OPENAI_API_KEY",
            rsllm::Provider::Claude => "ANTHROPIC_API_KEY",
            rsllm::Provider::Ollama => unimplemented!(),
        };
        let api_key = read_secret(api_key_var);
        Ok(Self {
            settings,
            creds: Credentials { api_key },
        })
    }
}

impl Config {
    pub fn provider(&self) -> &Provider {
        &self.settings.provider
    }

    pub fn model(&self) -> &str {
        &self.settings.model
    }

    pub fn api_key(&self) -> &SecretString<String> {
        &self.creds.api_key
    }
}
