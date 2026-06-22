//! Core `Provider` trait definition.
//!
//! We use `#[async_trait]` because native `async fn` in traits (Rust 1.75+)
//! is NOT dyn-compatible. The `async-trait` crate desugars into
//! `Pin<Box<dyn Future + Send>>`, enabling `Vec<Box<dyn Provider>>`.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::provider_metadata::{FreemiumMetadata, ProviderIdentity, metadata_for_provider_id};

/// A single chat message (OpenAI format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Unified response returned from any provider.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub provider: String,
    pub model: String,
    pub content: String,
}

impl fmt::Display for ChatResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}] {}", self.provider, self.model, self.content)
    }
}

/// Model information for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

/// Provider information including available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub models: Vec<ModelInfo>,
    pub free_tier: String,
    pub identity: Option<ProviderIdentity>,
    pub freemium: Option<FreemiumMetadata>,
}

/// Every AI provider implements this trait.
///
/// Because we store providers as `Vec<Box<dyn Provider>>` for the
/// heterogeneous CLI roster, we need dyn-compatibility. Native
/// `async fn in trait` is not dyn-compatible (Rust 1.75 limitation),
/// so we use `#[async_trait]` which desugars async fns into
/// `-> Pin<Box<dyn Future<Output = T> + Send + '_>>`.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique machine-readable identifier (e.g. "groq", "google-gemini").
    fn id(&self) -> &str;

    /// Human-readable name of this provider.
    fn name(&self) -> &str;

    /// The default model being used.
    fn model(&self) -> &str;

    /// Whether this provider is configured / authenticated.
    async fn is_configured(&self) -> bool;

    /// Interactive setup — prompts for API key or runs OAuth flow.
    async fn setup(&mut self) -> Result<()>;

    /// Send a chat completion request.
    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse>;

    /// Get all available models for this provider.
    fn get_models(&self) -> Vec<ModelInfo>;

    /// Get provider information including models and free tier details.
    async fn get_provider_info(&self) -> ProviderInfo {
        let metadata = metadata_for_provider_id(self.id());
        ProviderInfo {
            id: self.id().to_string(),
            name: self.name().to_string(),
            status: if self.is_configured().await {
                "ready".to_string()
            } else {
                "not_configured".to_string()
            },
            models: self.get_models(),
            free_tier: self.get_free_tier_info(),
            identity: metadata.map(|value| value.identity()),
            freemium: metadata.map(|value| value.freemium()),
        }
    }

    /// Get free tier information for this provider.
    fn get_free_tier_info(&self) -> String;
}
