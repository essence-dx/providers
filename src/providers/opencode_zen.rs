//! OpenCode Zen provider with public-key access for current free models.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

const BASE_URL: &str = "https://opencode.ai/zen/v1";
const PRIMARY_ENV_VAR: &str = "OPENCODE_API_KEY";
const SECONDARY_ENV_VAR: &str = "OPENCODE_ZEN_API_KEY";
const PUBLIC_API_KEY: &str = "public";
const DEFAULT_FREE_MODEL: &str = "deepseek-v4-flash-free";

const FREE_MODELS: &[(&str, &str)] = &[
    ("big-pickle", "Big Pickle"),
    ("deepseek-v4-flash-free", "DeepSeek V4 Flash Free"),
    ("mimo-v2.5-free", "MiMo-V2.5 Free"),
    ("minimax-m3-free", "MiniMax M3 Free"),
    ("nemotron-3-super-free", "Nemotron 3 Super Free"),
    ("nemotron-3-ultra-free", "Nemotron 3 Ultra Free"),
];

const PAID_MODELS: &[(&str, &str)] = &[
    ("gpt-5.5", "GPT 5.5"),
    ("gpt-5.4", "GPT 5.4"),
    ("gpt-5.4-mini", "GPT 5.4 Mini"),
    ("gpt-5.3-codex", "GPT 5.3 Codex"),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6"),
    ("gemini-3.5-flash", "Gemini 3.5 Flash"),
];

pub(crate) fn public_free_models() -> &'static [(&'static str, &'static str)] {
    FREE_MODELS
}

#[derive(Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
    reasoning_content: Option<String>,
}

pub struct OpenCodeZenProvider {
    api_key: Option<String>,
    default_model: String,
}

impl OpenCodeZenProvider {
    pub fn new() -> Self {
        Self {
            api_key: None,
            default_model: DEFAULT_FREE_MODEL.to_string(),
        }
    }

    pub fn load_key(&mut self, config: &AppConfig) {
        if let Some(value) = env_value(PRIMARY_ENV_VAR).or_else(|| env_value(SECONDARY_ENV_VAR)) {
            self.api_key = Some(value);
            return;
        }

        if let Some(value) = config
            .get_key("opencode-zen")
            .or_else(|| config.get_key("opencode"))
        {
            self.api_key = Some(value.to_string());
        }
    }

    fn request_key(&self) -> &str {
        self.api_key.as_deref().unwrap_or(PUBLIC_API_KEY)
    }

    fn is_public_session(&self) -> bool {
        self.api_key.is_none()
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[async_trait]
impl Provider for OpenCodeZenProvider {
    fn id(&self) -> &str {
        "opencode-zen"
    }

    fn name(&self) -> &str {
        "OpenCode Zen"
    }

    fn model(&self) -> &str {
        &self.default_model
    }

    async fn is_configured(&self) -> bool {
        true
    }

    async fn setup(&mut self) -> Result<()> {
        println!();
        println!("  OpenCode Zen free models work with public access.");
        println!(
            "  Set {} or {} only when you want paid Zen models.",
            PRIMARY_ENV_VAR, SECONDARY_ENV_VAR
        );
        Ok(())
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        if self.is_public_session()
            && !FREE_MODELS
                .iter()
                .any(|(model_id, _)| *model_id == self.default_model)
        {
            bail!("OpenCode Zen paid models require {}", PRIMARY_ENV_VAR);
        }

        let body = CompletionRequest {
            model: &self.default_model,
            messages,
            max_tokens: Some(1024),
        };

        let resp = reqwest::Client::new()
            .post(format!("{BASE_URL}/chat/completions"))
            .header("Content-Type", "application/json")
            .bearer_auth(self.request_key())
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("OpenCode Zen HTTP {}: {}", status.as_u16(), text);
        }

        let data: CompletionResponse = resp.json().await?;
        let content = data
            .choices
            .first()
            .and_then(|choice| {
                choice
                    .message
                    .content
                    .clone()
                    .or_else(|| choice.message.reasoning_content.clone())
            })
            .unwrap_or_else(|| "(empty)".to_string());

        Ok(ChatResponse {
            provider: self.name().to_string(),
            model: self.default_model.clone(),
            content,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        FREE_MODELS
            .iter()
            .map(|(id, name)| ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                description: "Free OpenCode Zen model available through public access".to_string(),
                is_default: *id == self.default_model,
            })
            .chain(PAID_MODELS.iter().map(|(id, name)| ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                description: format!("Paid OpenCode Zen model; set {PRIMARY_ENV_VAR} to use"),
                is_default: *id == self.default_model,
            }))
            .collect()
    }

    fn get_free_tier_info(&self) -> String {
        "Public access for current free Zen models; paid models require OPENCODE_API_KEY"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_metadata::metadata_for_provider_id;

    #[tokio::test]
    async fn opencode_zen_is_configured_for_public_free_models() {
        let provider = OpenCodeZenProvider::new();

        assert!(provider.is_configured().await);
        assert_eq!(provider.model(), DEFAULT_FREE_MODEL);
        assert_eq!(provider.request_key(), PUBLIC_API_KEY);
    }

    #[test]
    fn opencode_zen_lists_public_free_models_before_paid_models() {
        let provider = OpenCodeZenProvider::new();
        let models = provider.get_models();

        assert_eq!(models[0].id, "big-pickle");
        assert!(models.iter().any(|model| model.id == DEFAULT_FREE_MODEL));
        assert!(models.iter().any(|model| model.id == "gpt-5.5"));
        assert!(
            models
                .iter()
                .find(|model| model.id == DEFAULT_FREE_MODEL)
                .expect("default free model")
                .is_default
        );
    }

    #[test]
    fn opencode_zen_public_models_match_provider_metadata() {
        let metadata = metadata_for_provider_id("opencode-zen").expect("OpenCode Zen metadata");
        let model_ids = public_free_models()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        assert_eq!(metadata.free_model_ids, model_ids);
    }
}
