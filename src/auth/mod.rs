// Authentication module for OAuth and cookie-based auth
pub mod cookie_extractor;
pub mod token_store;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub token_type: TokenType,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenType {
    Bearer,
    Cookie,
    SessionToken,
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn authenticate(&self) -> Result<AuthToken, anyhow::Error>;
    #[allow(dead_code)]
    async fn refresh_if_needed(
        &self,
        token: &AuthToken,
    ) -> Result<Option<AuthToken>, anyhow::Error>;
}
