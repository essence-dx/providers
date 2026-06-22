//! Anthropic Claude API.
//! OpenAI-compatible API format.

use anyhow::Result;
use async_trait::async_trait;

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};
use crate::providers::openai_compat::OpenAICompatProvider;

pub struct AnthropicProvider {
    inner: OpenAICompatProvider,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        let mut inner = OpenAICompatProvider::new(
            "anthropic",
            "Anthropic Claude",
            "https://api.anthropic.com/v1",
            "claude-3-5-sonnet-20241022",
            "https://console.anthropic.com/",
        );
        // Add required headers for Anthropic API
        inner = inner.with_header("anthropic-version", "2023-06-01");
        Self { inner }
    }

    pub fn load_key(&mut self, config: &AppConfig) {
        self.inner.load_key(config);
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn name(&self) -> &str {
        "Anthropic Claude"
    }

    fn model(&self) -> &str {
        self.inner.model()
    }

    async fn is_configured(&self) -> bool {
        self.inner.is_configured().await
    }

    async fn setup(&mut self) -> Result<()> {
        self.inner.setup().await
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let mut resp = self.inner.chat(messages).await?;
        resp.provider = "Anthropic Claude".into();
        Ok(resp)
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: "Current default, highest capability".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: "Fast, efficient".to_string(),
                is_default: false,
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: "Most capable, slower".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Requires purchased or prepaid API credits; promotions vary by account".to_string()
    }
}
