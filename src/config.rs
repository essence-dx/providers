use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Persistent config stored at `~/.providers/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// provider_id → api_key
    pub api_keys: HashMap<String, String>,

    /// Google OAuth tokens (if user chose "Login with Google").
    pub google_oauth: Option<GoogleOAuthTokens>,

    /// Qwen OAuth tokens (if using device flow).
    pub qwen_oauth: Option<QwenOAuthTokens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleOAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    /// Which auth method: "oauth", "api-key", "vertex-ai"
    pub auth_method: String,
    /// For Vertex AI: project + location
    pub cloud_project: Option<String>,
    pub cloud_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QwenOAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

impl AppConfig {
    fn config_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Cannot find home directory")
            .join(".providers")
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), data)?;
        Ok(())
    }

    pub fn get_key(&self, id: &str) -> Option<&str> {
        self.api_keys.get(id).map(String::as_str)
    }

    pub fn set_key(&mut self, id: &str, key: String) {
        self.api_keys.insert(id.to_string(), key);
    }
}
