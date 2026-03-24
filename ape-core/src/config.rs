use std::{
    env::VarError,
    fs::{self, File},
    io::Write,
};

use secret_string::SecretString;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    Error, ape_dir,
    llm::{Model, Provider},
};

#[derive(Clone, Deserialize, Serialize)]
struct Settings {
    provider: Provider,
    // @TODO: Validation model against provider as part of
    // deserialization
    model: Model,
}

impl Default for Settings {
    fn default() -> Self {
        let model = Model::default();
        Self {
            provider: model.provider(),
            model,
        }
    }
}

#[derive(Clone)]
struct Credentials {
    api_key: SecretString<String>,
}

fn read_secret(name: &str) -> Result<SecretString<String>, VarError> {
    std::env::var(name).map(SecretString::new)
}

#[derive(Clone)]
pub struct Config {
    settings: Settings,
    creds: Credentials,
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        let config_file = ape_dir().join("config.json");
        let settings = match fs::exists(&config_file) {
            Ok(true) => {
                let json =
                    fs::read_to_string(config_file).map_err(|e| Error::Config(e.to_string()))?;
                serde_json::from_str(&json).map_err(|e| Error::Config(e.to_string()))?
            }
            Ok(false) => {
                // If the file doesn't exist, create it with default settings
                info!(
                    "Creating config file with defaults at {}",
                    config_file.display()
                );
                let settings = Settings::default();
                let json = serde_json::to_string_pretty(&settings)?;
                let mut file = File::create(&config_file)?;
                file.write_all(json.as_bytes())?;
                settings
            }
            Err(e) => return Err(Error::Config(e.to_string())),
        };
        let api_key_var = match &settings.provider {
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Claude => "ANTHROPIC_API_KEY",
        };
        let api_key =
            read_secret(api_key_var).map_err(|_| Error::Credential(api_key_var.to_string()))?;
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
