//! Replicate — Popular AI model hosting platform.
//! OpenAI-compatible API format for language models.

use anyhow::Result;
use async_trait::async_trait;

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};
use crate::providers::openai_compat::OpenAICompatProvider;

pub struct ReplicateProvider {
    inner: OpenAICompatProvider,
}

impl ReplicateProvider {
    pub fn new() -> Self {
        let inner = OpenAICompatProvider::new(
            "replicate",
            "Replicate",
            "https://api.replicate.com/v1",
            "meta/llama-2-70b-chat",
            "https://replicate.com/account/api-tokens",
        );
        Self { inner }
    }

    pub fn load_key(&mut self, config: &AppConfig) {
        self.inner.load_key(config);
    }
}

#[async_trait]
impl Provider for ReplicateProvider {
    fn id(&self) -> &str {
        "replicate"
    }

    fn name(&self) -> &str {
        "Replicate"
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
        resp.provider = "Replicate".into();
        Ok(resp)
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "meta/llama-2-70b-chat".to_string(),
                name: "Llama 2 70B Chat".to_string(),
                description: "Current default, stable chat model".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "meta/llama-2-13b-chat".to_string(),
                name: "Llama 2 13B Chat".to_string(),
                description: "Smaller, faster variant".to_string(),
                is_default: false,
            },
            ModelInfo {
                id: "mistralai/mistral-7b-instruct-v0.1".to_string(),
                name: "Mistral 7B Instruct".to_string(),
                description: "Efficient reasoning model".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Paid usage; credits or promotions vary by account".to_string()
    }
}
