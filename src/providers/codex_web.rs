// OpenAI Codex Web (Free with ChatGPT Plus/Pro) - OAuth authentication
use crate::auth::{AuthToken, Authenticator, TokenType, cookie_extractor, token_store};
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;

pub struct CodexWeb {
    #[allow(dead_code)]
    client: Client,
}

impl CodexWeb {
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
impl Authenticator for CodexWeb {
    fn provider_name(&self) -> &str {
        "codex-web"
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

        // Extract from Chrome cookies (same as ChatGPT)
        let session_token = cookie_extractor::extract_chatgpt_session()?;

        // Store in keychain (Codex uses same auth as ChatGPT)
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
impl Provider for CodexWeb {
    fn id(&self) -> &str {
        "codex-web"
    }

    fn name(&self) -> &str {
        "OpenAI Codex (Free with ChatGPT Plus/Pro)"
    }

    fn model(&self) -> &str {
        "gpt-5.3-codex"
    }

    async fn is_configured(&self) -> bool {
        // Check if we can extract cookies from Chrome
        cookie_extractor::extract_chatgpt_session().is_ok()
    }

    async fn setup(&mut self) -> anyhow::Result<()> {
        println!("  OpenAI Codex uses OAuth authentication via ChatGPT.");
        println!("  Please ensure you are logged into chat.openai.com in Opera or Chrome.");
        println!("  Requires ChatGPT Plus ($20/mo) or Pro ($200/mo) subscription.");
        println!("  Then this provider will automatically extract your session.");

        // Try to authenticate
        match self.authenticate().await {
            Ok(_) => {
                println!("  ✅ Successfully extracted Codex session from browser!");
                println!("  Note: Codex API access requires ChatGPT Plus or Pro subscription.");
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to extract Codex session: {}. Please log into chat.openai.com in Opera or Chrome first.",
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

        // Note: The actual Codex API is complex and requires ChatGPT Plus/Pro
        // This is a simplified implementation that may need updates
        let response_text = format!(
            "OpenAI Codex provider is configured but the backend API implementation is pending. Your message was: {}",
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
                id: "gpt-5.3-codex".to_string(),
                name: "GPT-5.3 Codex".to_string(),
                description: "Specialized coding model (requires Plus/Pro)".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "gpt-5.4".to_string(),
                name: "GPT-5.4".to_string(),
                description: "Latest general model (requires Plus/Pro)".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Requires ChatGPT Plus ($20/mo) or Pro ($200/mo) - OAuth via chat.openai.com".to_string()
    }
}
