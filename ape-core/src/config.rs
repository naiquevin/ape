use std::{env::VarError, fs};

use secret_string::SecretString;
use serde::Deserialize;

use crate::{
    Error, ape_dir,
    llm::{Model, Provider},
};

#[derive(Deserialize)]
struct Settings {
    provider: Provider,
    // @TODO: Validation model against provider as part of
    // deserialization
    model: Model,
}

struct Credentials {
    api_key: SecretString<String>,
}

fn read_secret(name: &str) -> Result<SecretString<String>, VarError> {
    std::env::var(name).map(SecretString::new)
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
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Claude => "ANTHROPIC_API_KEY",
        };
        let api_key = read_secret(api_key_var)
            .map_err(|_| Error::Credential(api_key_var.to_string()))?;
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

    pub fn model(&self) -> &Model {
        &self.settings.model
    }

    pub fn api_key(&self) -> &SecretString<String> {
        &self.creds.api_key
    }
}
