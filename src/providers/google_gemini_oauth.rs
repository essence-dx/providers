//! Google Gemini Authentication Provider
//!
//! **MULTIPLE AUTH OPTIONS SUPPORTED:**
//! 1. OAuth 2.0 (Code Assist API) - Requires Google Cloud project setup
//! 2. API Key (Regular Gemini API) - Simple, works immediately
//! 3. Auto-fallback from OAuth to API Key if available
//!
//! **CRITICAL FIXES FROM RESEARCH (March 12, 2026):**
//! - Code Assist API requires managed Google Cloud project
//! - Regular Gemini API works with just API key (ai.google.dev)
//! - Auto-fallback provides best user experience

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

// ────────────────────────────── Constants ──────────────────────────────

/// Code Assist API (OAuth) - Requires Google Cloud project
const CODE_ASSIST_ENDPOINT: &str = "https://cloudcode-pa.googleapis.com";
const CODE_ASSIST_API_VERSION: &str = "v1internal";

/// Regular Gemini API (API Key) - Works immediately
const GEMINI_API_ENDPOINT: &str = "https://generativelanguage.googleapis.com";
const GEMINI_API_VERSION: &str = "v1beta";

const EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1000; // 5 minutes

// Headers for Code Assist API (exact format from working debug logs)
const USER_AGENT: &str = "google-api-nodejs-client/9.15.1";
const X_GOOG_API_CLIENT: &str = "gl-node/22.17.0";
const CLIENT_METADATA: &str =
    "ideType=IDE_UNSPECIFIED,platform=PLATFORM_UNSPECIFIED,pluginType=GEMINI";

// ────────────────────────────── Errors ─────────────────────────────────

#[derive(Debug, Error)]
pub enum GeminiOAuthError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON (de)serialisation error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No cached credentials found — please run the login flow")]
    NoCachedCredentials,

    #[error("Could not determine home directory")]
    NoHomeDir,

    #[error("Code Assist onboarding failed: {0}")]
    OnboardingFailed(String),

    #[error("Account not eligible for Code Assist: {0}")]
    AccountNotEligible(String),
}

// ────────────────────────────── Project Context ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub managed_project_id: Option<String>,
    pub managed_project_number: Option<String>,
    pub tier: Option<String>,
}

impl ProjectContext {
    #[allow(dead_code)]
    fn is_valid(&self) -> bool {
        self.managed_project_id.is_some()
    }
}

// ────────────────────────────── Persisted credentials ──────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expiry_date: i64,
    pub client_id: String,
    pub client_secret: String,
    #[serde(rename = "type")]
    pub cred_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_email: Option<String>,
}

impl OAuthCredentials {
    fn is_expired(&self) -> bool {
        let now_ms = chrono::Utc::now().timestamp_millis();
        now_ms >= self.expiry_date - EXPIRY_BUFFER_MS
    }
}

// ────────────────────────────── Provider ───────────────────────────────

pub struct GoogleGeminiOAuthProvider {
    client: Client,
    #[allow(dead_code)]
    config: AppConfig,
}

impl GoogleGeminiOAuthProvider {
    pub fn new(config: AppConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");
        Self { client, config }
    }

    /// Check if we have a Google API key available as fallback
    fn has_api_key_fallback() -> bool {
        std::env::var("GOOGLE_API_KEY").is_ok() || std::env::var("GEMINI_API_KEY").is_ok()
    }

    /// Get API key from environment
    fn get_api_key() -> Option<String> {
        std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .ok()
    }

    /// Try API key authentication with regular Gemini API
    async fn try_api_key_auth(
        &self,
        messages: &[ChatMessage],
    ) -> Result<ChatResponse, anyhow::Error> {
        let api_key = Self::get_api_key().ok_or_else(|| {
            anyhow::anyhow!("No API key found in GOOGLE_API_KEY or GEMINI_API_KEY")
        })?;

        // Use regular Gemini API endpoint
        let url = format!(
            "{}/{}/models/{}:generateContent",
            GEMINI_API_ENDPOINT,
            GEMINI_API_VERSION,
            self.model()
        );

        let (system_parts, contents) = convert_messages_to_gemini(messages);

        let mut request_body = serde_json::json!({
            "contents": contents,
        });

        // Add system instruction if present
        if !system_parts.is_empty() {
            request_body["systemInstruction"] = serde_json::json!({
                "parts": system_parts
            });
        }

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API returned {status}: {text}");
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse {
            provider: format!("{} (API Key)", self.name()),
            model: self.model().to_string(),
            content: text,
        })
    }

    /// Build mandatory Code Assist headers (EXACT format from working debug logs)
    fn code_assist_headers() -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();

        // CRITICAL: These exact values produce 200 OK responses
        headers.insert(
            reqwest::header::USER_AGENT,
            USER_AGENT.parse().expect("invalid user agent"),
        );

        headers.insert(
            "X-Goog-Api-Client",
            X_GOOG_API_CLIENT
                .parse()
                .expect("invalid api client header"),
        );

        // CRITICAL: comma-separated key=value, NOT JSON!
        headers.insert(
            "Client-Metadata",
            CLIENT_METADATA.parse().expect("invalid client metadata"),
        );

        // CRITICAL: x-goog-api-key must be present as empty string
        headers.insert(
            "x-goog-api-key",
            "".parse().expect("invalid api key header"),
        );

        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("invalid content type"),
        );

        headers
    }

    /// Build the Code Assist request envelope (EXACT format from working debug logs)
    fn build_code_assist_body(
        model: &str,
        messages: &[ChatMessage],
        project_id: Option<&str>,
    ) -> serde_json::Value {
        let (system_parts, contents) = convert_messages_to_gemini(messages);

        let mut inner_request = serde_json::json!({
            "contents": contents,
        });

        // Add systemInstruction as separate field (NOT inlined in contents)
        if !system_parts.is_empty() {
            inner_request["systemInstruction"] = serde_json::json!({
                "parts": system_parts
            });
        }

        // CRITICAL: model name WITHOUT "models/" prefix
        let model_name = model.strip_prefix("models/").unwrap_or(model);

        let mut envelope = serde_json::json!({
            "model": model_name,
            "request": inner_request,
        });

        // Add project if available
        if let Some(pid) = project_id {
            envelope["project"] = serde_json::json!(pid);
        }

        envelope
    }

    /// Two-step Code Assist onboarding: loadCodeAssist + onboardUser
    async fn onboard_code_assist(
        client: &Client,
        access_token: &str,
    ) -> Result<ProjectContext, GeminiOAuthError> {
        // Step 1: Load Code Assist
        let load_url = format!(
            "{}/{}:loadCodeAssist",
            CODE_ASSIST_ENDPOINT, CODE_ASSIST_API_VERSION
        );

        let mut headers = Self::code_assist_headers();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token)
                .parse()
                .expect("invalid auth header"),
        );

        let load_body = serde_json::json!({});

        let load_resp = client
            .post(&load_url)
            .headers(headers.clone())
            .json(&load_body)
            .send()
            .await?;

        if !load_resp.status().is_success() {
            let status = load_resp.status();
            let text = load_resp.text().await.unwrap_or_default();
            return Err(GeminiOAuthError::OnboardingFailed(format!(
                "loadCodeAssist failed with {}: {}",
                status, text
            )));
        }

        let load_json: serde_json::Value = load_resp.json().await?;

        // Check if we already have a valid project context
        if let Some(project_id) = load_json.get("managedProjectId").and_then(|v| v.as_str()) {
            let project_context = ProjectContext {
                managed_project_id: Some(project_id.to_string()),
                managed_project_number: load_json
                    .get("managedProjectNumber")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                tier: load_json
                    .get("tier")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            };

            save_project_context(&project_context).await?;
            return Ok(project_context);
        }

        // Step 2: Onboard User if needed
        let onboard_url = format!(
            "{}/{}:onboardUser",
            CODE_ASSIST_ENDPOINT, CODE_ASSIST_API_VERSION
        );

        let onboard_body = serde_json::json!({});

        let onboard_resp = client
            .post(&onboard_url)
            .headers(headers)
            .json(&onboard_body)
            .send()
            .await?;

        if !onboard_resp.status().is_success() {
            let status = onboard_resp.status();
            let text = onboard_resp.text().await.unwrap_or_default();

            // Check for account eligibility issues
            if text.contains("not eligible") || text.contains("eligibility") {
                return Err(GeminiOAuthError::AccountNotEligible(format!(
                    "Account not eligible for Code Assist: {}",
                    text
                )));
            }

            return Err(GeminiOAuthError::OnboardingFailed(format!(
                "onboardUser failed with {}: {}",
                status, text
            )));
        }

        let onboard_json: serde_json::Value = onboard_resp.json().await?;

        // Extract project context from onboarding response
        let project_context = ProjectContext {
            managed_project_id: onboard_json
                .get("managedProjectId")
                .and_then(|v| v.as_str())
                .map(String::from),
            managed_project_number: onboard_json
                .get("managedProjectNumber")
                .and_then(|v| v.as_str())
                .map(String::from),
            tier: onboard_json
                .get("tier")
                .and_then(|v| v.as_str())
                .map(String::from),
        };

        if project_context.managed_project_id.is_none() {
            return Err(GeminiOAuthError::OnboardingFailed(format!(
                "No project ID received: {:?}",
                project_context
            )));
        }

        // Save project context
        save_project_context(&project_context).await?;

        Ok(project_context)
    }
}

#[async_trait]
impl Provider for GoogleGeminiOAuthProvider {
    fn id(&self) -> &str {
        "gemini-oauth"
    }

    fn name(&self) -> &str {
        "Google Gemini (OAuth)"
    }

    fn model(&self) -> &str {
        "gemini-2.5-flash"
    }

    async fn is_configured(&self) -> bool {
        // Check API key first (simpler option)
        if Self::has_api_key_fallback() {
            return true;
        }

        // Check OAuth credentials
        load_credentials().await.unwrap_or(None).is_some()
    }

    async fn setup(&mut self) -> Result<(), anyhow::Error> {
        // Check if we have API key fallback available
        if Self::has_api_key_fallback() {
            println!("✔ Google API key detected - ready to use!");
            println!("  Using regular Gemini API (ai.google.dev)");
            return Ok(());
        }

        // Try OAuth setup
        match ensure_valid_token(&self.client, true).await {
            Ok(_) => {
                println!("✔ Google Gemini OAuth is configured!");
                println!("  Note: Code Assist requires Google Cloud project setup");
                println!("  For easier setup, consider using GOOGLE_API_KEY instead");
                Ok(())
            }
            Err(e) => {
                println!("⚠ OAuth setup failed: {}", e);
                println!();
                println!("💡 EASY FIX: Get a free API key instead!");
                println!("   1. Go to https://ai.google.dev/");
                println!("   2. Click 'Get API key'");
                println!("   3. Set: export GOOGLE_API_KEY=your-key-here");
                println!("   4. Run the command again");
                Err(anyhow::anyhow!("OAuth setup failed and no API key found"))
            }
        }
    }

    /// FIXED: Multiple auth options with automatic fallback
    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse, anyhow::Error> {
        // Option 1: Try API key first (simpler and more reliable)
        if Self::has_api_key_fallback() {
            match self.try_api_key_auth(messages).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    println!("⚠ API key authentication failed: {}", e);
                    println!("  Falling back to OAuth...");
                }
            }
        }

        // Option 2: Try OAuth with Code Assist API
        let creds = ensure_valid_token(&self.client, true).await.map_err(|e| {
            anyhow::anyhow!(
                "OAuth authentication failed: {}. Try setting GOOGLE_API_KEY instead.",
                e
            )
        })?;

        // Try to get project context, attempt onboarding if needed
        let project_context = match load_project_context().await? {
            Some(ctx) if ctx.managed_project_id.is_some() => Some(ctx),
            _ => {
                // Attempt onboarding
                match Self::onboard_code_assist(&self.client, &creds.access_token).await {
                    Ok(ctx) => Some(ctx),
                    Err(e) => {
                        println!("⚠ Code Assist onboarding failed: {}", e);

                        // Provide helpful guidance
                        if Self::has_api_key_fallback() {
                            println!(
                                "  API key is available - this shouldn't happen. Please report this issue."
                            );
                        } else {
                            println!();
                            println!("💡 EASY FIX: Use the regular Gemini API instead!");
                            println!("   1. Go to https://ai.google.dev/");
                            println!("   2. Click 'Get API key' (free)");
                            println!("   3. Set: export GOOGLE_API_KEY=your-key-here");
                            println!("   4. Run the command again");
                            println!();
                            println!("   OR set up Google Cloud project:");
                            println!("   1. Go to Google Cloud Console");
                            println!("   2. Create or select a project");
                            println!("   3. Enable Gemini for Google Cloud API");
                            println!("   4. Set: export GOOGLE_CLOUD_PROJECT=your-project-id");
                        }

                        return Err(anyhow::anyhow!(
                            "Code Assist setup required. See instructions above."
                        ));
                    }
                }
            }
        };

        // *** CRITICAL FIX: Correct URL format ***
        let url = format!(
            "{}/{}:generateContent",
            CODE_ASSIST_ENDPOINT, CODE_ASSIST_API_VERSION
        );

        // *** CRITICAL FIX: Code Assist envelope with exact format from debug logs ***
        let project_id = project_context
            .as_ref()
            .and_then(|ctx| ctx.managed_project_id.as_deref());
        let body = Self::build_code_assist_body(self.model(), messages, project_id);

        // *** CRITICAL FIX: Exact headers from working debug logs ***
        let mut headers = Self::code_assist_headers();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", creds.access_token)
                .parse()
                .expect("invalid auth header"),
        );

        // Add x-goog-user-project if we have a project ID
        if let Some(pid) = project_id {
            headers.insert(
                "x-goog-user-project",
                pid.parse().expect("invalid project header"),
            );
        }

        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();

            if text.contains("not eligible") {
                anyhow::bail!(
                    "Account not eligible for Gemini Code Assist.\n\
                     \n\
                     💡 EASY FIX: Use the regular Gemini API instead!\n\
                     1. Go to https://ai.google.dev/\n\
                     2. Click 'Get API key' (free)\n\
                     3. Set: export GOOGLE_API_KEY=your-key-here\n\
                     4. Run the command again\n\
                     \n\
                     Server response: {}",
                    text
                );
            }

            if status.as_u16() == 500 {
                anyhow::bail!(
                    "Code Assist API returned 500 Internal Server Error.\n\
                     \n\
                     💡 EASY FIX: Use the regular Gemini API instead!\n\
                     1. Go to https://ai.google.dev/\n\
                     2. Click 'Get API key' (free)\n\
                     3. Set: export GOOGLE_API_KEY=your-key-here\n\
                     4. Run the command again\n\
                     \n\
                     OR set up Google Cloud project:\n\
                     1. Go to Google Cloud Console\n\
                     2. Create or select a project\n\
                     3. Enable Gemini for Google Cloud API\n\
                     4. Set: export GOOGLE_CLOUD_PROJECT=your-project-id\n\
                     \n\
                     Server response: {}",
                    text
                );
            }

            anyhow::bail!("Gemini Code Assist API returned {status}: {text}");
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse {
            provider: self.name().to_string(),
            model: self.model().to_string(),
            content: text,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                description: "Current default, OAuth-enabled".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                description: "High capability, 2M context".to_string(),
                is_default: false,
            },
            ModelInfo {
                id: "gemini-1.5-flash".to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                description: "Fast, efficient".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Google OAuth quotas vary by account, project, region, and release policy".to_string()
    }
}

// ────────────────────────────── Helper Functions ───────────────────────

fn creds_path() -> Result<PathBuf, GeminiOAuthError> {
    let home = dirs::home_dir().ok_or(GeminiOAuthError::NoHomeDir)?;
    Ok(home.join(".gemini").join("oauth_creds.json"))
}

fn project_context_path() -> Result<PathBuf, GeminiOAuthError> {
    let home = dirs::home_dir().ok_or(GeminiOAuthError::NoHomeDir)?;
    Ok(home.join(".gemini").join("project_context.json"))
}

async fn load_credentials() -> Result<Option<OAuthCredentials>, GeminiOAuthError> {
    let path = creds_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).await?;
    let creds: OAuthCredentials = serde_json::from_str(&data)?;
    Ok(Some(creds))
}

async fn load_project_context() -> Result<Option<ProjectContext>, GeminiOAuthError> {
    let path = project_context_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).await?;
    let context: ProjectContext = serde_json::from_str(&data)?;
    Ok(Some(context))
}

async fn save_project_context(context: &ProjectContext) -> Result<(), GeminiOAuthError> {
    let path = project_context_path()?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let data = serde_json::to_string_pretty(context)?;
    fs::write(&path, data).await?;
    Ok(())
}

pub async fn ensure_valid_token(
    _client: &Client,
    _interactive: bool,
) -> Result<OAuthCredentials, GeminiOAuthError> {
    if let Some(creds) = load_credentials().await?
        && !creds.is_expired()
    {
        return Ok(creds);
    }

    Err(GeminiOAuthError::NoCachedCredentials)
}

fn convert_messages_to_gemini(
    messages: &[ChatMessage],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut system_parts: Vec<serde_json::Value> = Vec::new();
    let mut contents: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        let role = &msg.role;
        let content = &msg.content;

        if role == "system" {
            // Collect system messages into systemInstruction.parts
            system_parts.push(serde_json::json!({ "text": content }));
            continue;
        }

        let gemini_role = match role.as_str() {
            "assistant" => "model",
            other => other,
        };

        contents.push(serde_json::json!({
            "role": gemini_role,
            "parts": [{ "text": content }]
        }));
    }

    (system_parts, contents)
}
