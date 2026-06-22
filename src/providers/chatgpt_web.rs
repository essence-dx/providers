// ChatGPT Web (Free) - Cookie-based authentication
use crate::auth::{AuthToken, Authenticator, TokenType, cookie_extractor, token_store};
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;

pub struct ChatGptWeb {
    #[allow(dead_code)]
    client: Client,
}

impl ChatGptWeb {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()
                .unwrap(),
        }
    }
}

#[async_trait]
impl Authenticator for ChatGptWeb {
    fn provider_name(&self) -> &str {
        "chatgpt-web"
    }

    async fn authenticate(&self) -> Result<AuthToken, anyhow::Error> {
        // Try to retrieve from keychain first
        if let Ok(stored) = token_store::retrieve_token(self.provider_name())
            && !token_store::is_token_expired(&stored)
        {
            return Ok(AuthToken {
                token: stored.token,
                token_type: TokenType::SessionToken,
                expires_at: stored.expires_at,
                metadata: stored.metadata,
            });
        }

        // Extract from Chrome cookies
        let session_token = cookie_extractor::extract_chatgpt_session()?;

        // Store in keychain (ChatGPT tokens typically last 14-30 days)
        let expires_at = Some(chrono::Utc::now() + chrono::Duration::days(14));
        let stored = token_store::StoredToken {
            token: session_token.clone(),
            expires_at,
            metadata: HashMap::new(),
        };
        token_store::store_token(self.provider_name(), &stored)?;

        Ok(AuthToken {
            token: session_token,
            token_type: TokenType::SessionToken,
            expires_at,
            metadata: HashMap::new(),
        })
    }

    async fn refresh_if_needed(
        &self,
        token: &AuthToken,
    ) -> Result<Option<AuthToken>, anyhow::Error> {
        if let Some(expires_at) = token.expires_at
            && chrono::Utc::now() >= expires_at
        {
            return Ok(Some(self.authenticate().await?));
        }
        Ok(None)
    }
}

#[async_trait]
impl Provider for ChatGptWeb {
    fn id(&self) -> &str {
        "chatgpt-web"
    }

    fn name(&self) -> &str {
        "ChatGPT Web (Free)"
    }

    fn model(&self) -> &str {
        "gpt-4o-mini"
    }

    async fn is_configured(&self) -> bool {
        // Check if we can extract cookies from Chrome
        cookie_extractor::extract_chatgpt_session().is_ok()
    }

    async fn setup(&mut self) -> anyhow::Result<()> {
        println!("  ChatGPT Web uses cookie extraction from your browser.");
        println!("  Please ensure you are logged into chat.openai.com in Opera or Chrome.");
        println!("  Then this provider will automatically extract your session.");

        // Try to authenticate
        match self.authenticate().await {
            Ok(_) => {
                println!("  ✅ Successfully extracted ChatGPT session from browser!");
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to extract ChatGPT session: {}. Please log into chat.openai.com in Opera or Chrome first.",
                    e
                )
            }
        }
    }

    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<ChatResponse> {
        // Authenticate first
        let _auth_token = self.authenticate().await?;

        // For simplicity, just use the last user message
        let prompt = messages
            .iter()
            .rfind(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Note: The actual ChatGPT backend API is complex and changes frequently
        // This is a simplified implementation that may need updates
        let response_text = format!(
            "ChatGPT Web provider is configured but the backend API implementation is pending. Your message was: {}",
            prompt
        );

        Ok(ChatResponse {
            provider: self.name().to_string(),
            model: self.model().to_string(),
            content: response_text,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                description: "Fast and efficient model".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                description: "Most capable model".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Free tier with rate limits - requires Chrome login to chat.openai.com".to_string()
    }
}
