//! Perplexity AI — Agent API launched 2026, online and offline models.
//! OpenAI-compatible API format with web search capabilities.

use anyhow::Result;
use async_trait::async_trait;

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};
use crate::providers::openai_compat::OpenAICompatProvider;

pub struct PerplexityProvider {
    inner: OpenAICompatProvider,
}

impl PerplexityProvider {
    pub fn new() -> Self {
        let inner = OpenAICompatProvider::new(
            "perplexity",
            "Perplexity AI",
            "https://api.perplexity.ai",
            "llama-3.1-sonar-small-128k-online",
            "https://www.perplexity.ai/settings/api",
        );
        Self { inner }
    }

    pub fn load_key(&mut self, config: &AppConfig) {
        self.inner.load_key(config);
    }
}

#[async_trait]
impl Provider for PerplexityProvider {
    fn id(&self) -> &str {
        "perplexity"
    }

    fn name(&self) -> &str {
        "Perplexity AI"
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
        resp.provider = "Perplexity AI".into();
        Ok(resp)
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "llama-3.1-sonar-small-128k-online".to_string(),
                name: "Llama 3.1 Sonar Small Online".to_string(),
                description: "Current default, web search enabled".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "llama-3.1-sonar-large-128k-online".to_string(),
                name: "Llama 3.1 Sonar Large Online".to_string(),
                description: "High capability, web search enabled".to_string(),
                is_default: false,
            },
            ModelInfo {
                id: "llama-3.1-sonar-huge-128k-online".to_string(),
                name: "Llama 3.1 Sonar Huge Online".to_string(),
                description: "Highest capability, web search enabled".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Credit-based API; limited access varies by account tier".to_string()
    }
}
