//! Cloudflare Workers AI with account-level free neuron allocation.
//! Uses its own REST format (NOT OpenAI-compatible).

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

#[derive(Serialize)]
struct CfRequest<'a> {
    messages: &'a [CfMessage],
}

#[derive(Serialize)]
struct CfMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CfResponse {
    result: Option<CfResult>,
    success: bool,
    errors: Option<Vec<CfError>>,
}

#[derive(Deserialize)]
struct CfResult {
    response: Option<String>,
}

#[derive(Deserialize)]
struct CfError {
    message: String,
}

pub struct CloudflareProvider {
    account_id: Option<String>,
    api_token: Option<String>,
    model: String,
}

impl CloudflareProvider {
    pub fn new() -> Self {
        Self {
            account_id: None,
            api_token: None,
            model: "@cf/meta/llama-3.1-8b-instruct-fp8".into(),
        }
    }

    pub fn load_key(&mut self, config: &AppConfig) {
        if let Ok(v) = std::env::var("CLOUDFLARE_ACCOUNT_ID") {
            self.account_id = Some(v);
        } else if let Some(v) = config.get_key("cloudflare-account-id") {
            self.account_id = Some(v.to_string());
        }

        if let Ok(v) = std::env::var("CF_API_TOKEN") {
            self.api_token = Some(v);
        } else if let Ok(v) = std::env::var("CLOUDFLARE_API_KEY") {
            self.api_token = Some(v);
        } else if let Some(v) = config.get_key("cloudflare") {
            self.api_token = Some(v.to_string());
        }
    }
}

#[async_trait]
impl Provider for CloudflareProvider {
    fn id(&self) -> &str {
        "cloudflare"
    }
    fn name(&self) -> &str {
        "Cloudflare Workers AI"
    }
    fn model(&self) -> &str {
        &self.model
    }

    async fn is_configured(&self) -> bool {
        self.api_token.is_some() && self.account_id.is_some()
    }

    async fn setup(&mut self) -> Result<()> {
        if self.is_configured().await {
            return Ok(());
        }

        println!();
        println!("  ┌─────────────────────────────────────────────────────┐");
        println!("  │  Cloudflare Workers AI — free neuron allocation     │");
        println!("  │  1. dash.cloudflare.com → get Account ID from URL   │");
        println!("  │  2. dash.cloudflare.com/profile/api-tokens          │");
        println!("  └─────────────────────────────────────────────────────┘");

        let mut config = AppConfig::load()?;

        if self.account_id.is_none() {
            print!("    Account ID: ");
            io::stdout().flush()?;
            let mut id = String::new();
            io::stdin().read_line(&mut id)?;
            let id = id.trim().to_string();
            config.set_key("cloudflare-account-id", id.clone());
            self.account_id = Some(id);
        }

        if self.api_token.is_none() {
            print!("    API Token: ");
            io::stdout().flush()?;
            let mut tok = String::new();
            io::stdin().read_line(&mut tok)?;
            let tok = tok.trim().to_string();
            config.set_key("cloudflare", tok.clone());
            self.api_token = Some(tok);
        }

        config.save()?;
        Ok(())
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let token = self
            .api_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Cloudflare not configured"))?;
        let acct = self
            .account_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No Cloudflare account ID"))?;

        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            acct, self.model
        );

        let cf_msgs: Vec<CfMessage> = messages
            .iter()
            .map(|m| CfMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .bearer_auth(token)
            .json(&CfRequest { messages: &cf_msgs })
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Cloudflare HTTP {}: {}", status.as_u16(), text);
        }

        let data: CfResponse = resp.json().await?;
        if !data.success {
            let msg = data
                .errors
                .and_then(|e| e.first().map(|e| e.message.clone()))
                .unwrap_or_else(|| "Unknown error".into());
            bail!("Cloudflare error: {}", msg);
        }

        let content = data
            .result
            .and_then(|r| r.response)
            .unwrap_or_else(|| "(empty)".into());

        Ok(ChatResponse {
            provider: "Cloudflare Workers AI".into(),
            model: self.model.clone(),
            content,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "@cf/meta/llama-3.1-8b-instruct-fp8".to_string(),
                name: "Llama 3.1 8B Instruct FP8".to_string(),
                description: "Current default, fast inference".to_string(),
                is_default: self.model == "@cf/meta/llama-3.1-8b-instruct-fp8",
            },
            ModelInfo {
                id: "@cf/meta/llama-3.3-70b-instruct-fp8-fast".to_string(),
                name: "Llama 3.3 70B Instruct FP8 Fast".to_string(),
                description: "High capability, optimized".to_string(),
                is_default: self.model == "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
            },
            ModelInfo {
                id: "@cf/mistral/mistral-7b-instruct-v0.1".to_string(),
                name: "Mistral 7B Instruct v0.1".to_string(),
                description: "Efficient reasoning".to_string(),
                is_default: self.model == "@cf/mistral/mistral-7b-instruct-v0.1",
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Free neuron allocation when eligible; account limits apply".to_string()
    }
}
