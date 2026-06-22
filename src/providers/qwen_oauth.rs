//! Qwen OAuth 2.0 Device Authorization Flow (RFC 8628 + PKCE)
//!
//! Implements the complete device-code flow used by the official Qwen CLI:
//!   1. Generate PKCE pair (code_verifier + S256 code_challenge)
//!   2. POST to /oauth2/device/code to obtain device_code + user_code
//!   3. Open the user's browser to the verification URI
//!   4. Poll /oauth2/token until the user authorises (or timeout)
//!   5. Persist credentials to ~/.qwen/oauth_creds.json
//!   6. Transparently refresh expired tokens
//!
//! Compatible with credentials written by `qwen-code` CLI.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

// ────────────────────────────── Constants ──────────────────────────────

const CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const SCOPE: &str = "openid profile email model.completion";
const DEVICE_CODE_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/device/code";
const TOKEN_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/token";
const RESOURCE_URL: &str = "https://portal.qwen.ai/v1";

/// Initial interval between token-polling requests (seconds).
const INITIAL_POLL_INTERVAL_SECS: f64 = 2.0;
/// Maximum interval between polls (seconds).
const MAX_POLL_INTERVAL_SECS: f64 = 10.0;
/// Maximum number of poll attempts before giving up.
const MAX_POLL_ATTEMPTS: u32 = 300;
/// How many milliseconds before actual expiry we treat a token as expired,
/// giving us a comfortable buffer for the refresh round-trip.
const EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1000; // 5 minutes

// ────────────────────────────── Errors ─────────────────────────────────

#[derive(Debug, Error)]
pub enum QwenOAuthError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON (de)serialisation failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Device-code request rejected by server: {0}")]
    DeviceCodeRejected(String),

    #[error("Token request failed: {error} — {error_description}")]
    TokenError {
        error: String,
        error_description: String,
    },

    #[error("Device-code flow timed out after {0} poll attempts")]
    PollTimeout(u32),

    #[error("Token refresh failed: {0}")]
    RefreshFailed(String),

    #[error("Failed to open browser: {0}")]
    BrowserOpen(String),

    #[error("Failed to build credentials directory: {0}")]
    CredsDir(String),
}

// ────────────────────────────── Wire types ─────────────────────────────

/// Response from the device-authorisation endpoint.
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Successful token response.
#[derive(Debug, Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    refresh_token: Option<String>,
    token_type: String,
    expires_in: Option<u64>,
    #[serde(default)]
    resource_url: Option<String>,
}

/// Error token response (during polling or refresh).
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: String,
}

// ────────────────────────────── Persisted credentials ──────────────────

/// Layout of `~/.qwen/oauth_creds.json`, compatible with the official CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub token_type: String,
    /// Epoch millis at which the access token expires.
    pub expiry_date: i64,
    /// Base URL for model completions.
    pub resource_url: String,
}

impl OAuthCredentials {
    /// Returns `true` when the access token is (or is about to be) expired.
    fn is_expired(&self) -> bool {
        let now_ms = chrono::Utc::now().timestamp_millis();
        now_ms >= self.expiry_date - EXPIRY_BUFFER_MS
    }
}

// ────────────────────────────── PKCE helpers ───────────────────────────

/// Generate a cryptographically random PKCE code-verifier (base64url, 43 chars).
fn generate_code_verifier() -> String {
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Derive the S256 code-challenge from a code-verifier.
fn derive_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

// ────────────────────────────── File helpers ───────────────────────────

/// Returns `~/.qwen/oauth_creds.json`.
fn creds_path() -> Result<PathBuf, QwenOAuthError> {
    let home = dirs::home_dir()
        .ok_or_else(|| QwenOAuthError::CredsDir("could not determine home directory".into()))?;
    Ok(home.join(".qwen").join("oauth_creds.json"))
}

async fn load_credentials() -> Result<Option<OAuthCredentials>, QwenOAuthError> {
    let path = creds_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = tokio::fs::read_to_string(&path).await?;
    let creds: OAuthCredentials = serde_json::from_str(&data)?;
    Ok(Some(creds))
}

async fn save_credentials(creds: &OAuthCredentials) -> Result<(), QwenOAuthError> {
    let path = creds_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(creds)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

// ────────────────────────────── Core OAuth logic ───────────────────────

/// Step 1 + 2: request a device code from the authorisation server.
async fn request_device_code(
    client: &reqwest::Client,
    code_challenge: &str,
) -> Result<DeviceCodeResponse, QwenOAuthError> {
    let params = [
        ("client_id", CLIENT_ID),
        ("scope", SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
    ];

    let resp = client
        .post(DEVICE_CODE_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(QwenOAuthError::DeviceCodeRejected(body));
    }

    let device: DeviceCodeResponse = resp.json().await?;
    Ok(device)
}

/// Step 3: attempt to open the verification URL in the user's default browser.
fn open_browser(url: &str) -> Result<(), QwenOAuthError> {
    open::that(url).map_err(|e| QwenOAuthError::BrowserOpen(e.to_string()))
}

/// Step 4: poll the token endpoint until the user authorises (or timeout).
async fn poll_for_token(
    client: &reqwest::Client,
    device_code: &str,
    code_verifier: &str,
    initial_interval: u64,
) -> Result<TokenSuccessResponse, QwenOAuthError> {
    let mut interval_secs = if initial_interval == 0 {
        INITIAL_POLL_INTERVAL_SECS
    } else {
        initial_interval as f64
    };

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", CLIENT_ID),
        ("device_code", device_code),
        ("code_verifier", code_verifier),
    ];

    for _attempt in 1..=MAX_POLL_ATTEMPTS {
        tokio::time::sleep(Duration::from_secs_f64(interval_secs)).await;

        let resp = client
            .post(TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await?;

        // Try to detect success vs structured error from the body.
        let status = resp.status();
        let body = resp.text().await?;

        if status.is_success() {
            let token: TokenSuccessResponse = serde_json::from_str(&body)?;
            return Ok(token);
        }

        // Try to parse a structured OAuth error.
        let err: TokenErrorResponse = match serde_json::from_str(&body) {
            Ok(e) => e,
            Err(_) => {
                // Unparseable body — propagate as generic token error.
                return Err(QwenOAuthError::TokenError {
                    error: status.to_string(),
                    error_description: body,
                });
            }
        };

        match err.error.as_str() {
            "authorization_pending" => {
                // User hasn't authorised yet — keep polling.
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            "slow_down" => {
                // Back off: multiply interval by 1.5, cap at MAX.
                interval_secs = (interval_secs * 1.5).min(MAX_POLL_INTERVAL_SECS);
            }
            "expired_token" => {
                return Err(QwenOAuthError::TokenError {
                    error: err.error,
                    error_description:
                        "The device code has expired. Please restart the login flow.".into(),
                });
            }
            "access_denied" => {
                return Err(QwenOAuthError::TokenError {
                    error: err.error,
                    error_description: if err.error_description.is_empty() {
                        "The user denied the authorisation request.".to_string()
                    } else {
                        err.error_description
                    },
                });
            }
            _ => {
                // Any other error is fatal.
                return Err(QwenOAuthError::TokenError {
                    error: err.error,
                    error_description: err.error_description,
                });
            }
        }
    }

    Err(QwenOAuthError::PollTimeout(MAX_POLL_ATTEMPTS))
}

/// Step 6: refresh an expired access token using the refresh token.
async fn refresh_access_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<TokenSuccessResponse, QwenOAuthError> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(QwenOAuthError::RefreshFailed(body));
    }

    let token: TokenSuccessResponse = resp.json().await?;
    Ok(token)
}

/// Convert a raw token response into persistable credentials.
fn token_response_to_credentials(
    resp: &TokenSuccessResponse,
    previous_refresh_token: Option<&str>,
) -> OAuthCredentials {
    let expires_in_ms = resp.expires_in.unwrap_or(3600) as i64 * 1000;
    let expiry_date = chrono::Utc::now().timestamp_millis() + expires_in_ms;
    OAuthCredentials {
        access_token: resp.access_token.clone(),
        refresh_token: resp
            .refresh_token
            .clone()
            .or_else(|| previous_refresh_token.map(String::from)),
        token_type: resp.token_type.clone(),
        expiry_date,
        resource_url: resp
            .resource_url
            .clone()
            .unwrap_or_else(|| RESOURCE_URL.to_string()),
    }
}

// ────────────────────────────── Public high-level API ──────────────────

/// Run the full interactive device-code login flow.
///
/// Prints instructions to stdout, opens the browser, polls until authorised,
/// persists credentials, and returns them.
async fn login(client: &reqwest::Client) -> Result<OAuthCredentials, QwenOAuthError> {
    // ── PKCE ──
    let code_verifier = generate_code_verifier();
    let code_challenge = derive_code_challenge(&code_verifier);

    // ── Device code ──
    let device = request_device_code(client, &code_challenge).await?;

    // ── User instructions ──
    println!();
    println!("┌──────────────────────────────────────────────────────┐");
    println!("│  Qwen OAuth — Device Authorisation                  │");
    println!("├──────────────────────────────────────────────────────┤");
    println!("│  Open this URL in your browser:                     │");
    println!("│  {:<51}│", device.verification_uri);
    println!("│                                                      │");
    println!("│  And enter code: {:<35}│", device.user_code);
    println!("│                                                      │");
    println!("│  Waiting for authorisation…                          │");
    println!("└──────────────────────────────────────────────────────┘");
    println!();

    // Try to open the complete URI (includes user_code) directly.
    let uri = device
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device.verification_uri);

    println!("  🌐 Opening {} in your browser...", uri);
    if let Err(e) = open_browser(uri) {
        println!("  ⚠ Could not open browser automatically: {}", e);
        println!("  Please open the URL above manually.");
    }

    // ── Poll ──
    let token_resp =
        poll_for_token(client, &device.device_code, &code_verifier, device.interval).await?;

    let creds = token_response_to_credentials(&token_resp, None);

    // ── Persist ──
    save_credentials(&creds).await?;
    println!();
    println!("  ✅ Logged in successfully. Credentials saved to ~/.qwen/oauth_creds.json");

    Ok(creds)
}

/// Obtain a valid access token, refreshing or re-logging-in as needed.
///
/// 1. Load cached credentials.
/// 2. If valid → return.
/// 3. If expired + refresh_token → refresh, persist, return.
/// 4. Otherwise → run the full login flow.
async fn ensure_valid_token(client: &reqwest::Client) -> Result<OAuthCredentials, QwenOAuthError> {
    if let Some(creds) = load_credentials().await? {
        if !creds.is_expired() {
            return Ok(creds);
        }

        // Attempt refresh.
        if let Some(ref rt) = creds.refresh_token {
            match refresh_access_token(client, rt).await {
                Ok(token_resp) => {
                    let new_creds = token_response_to_credentials(&token_resp, Some(rt));
                    save_credentials(&new_creds).await?;
                    println!("  ✅ Qwen token refreshed.");
                    return Ok(new_creds);
                }
                Err(e) => {
                    println!("  ⚠ Token refresh failed: {}", e);
                    println!("  Falling back to full login flow…");
                }
            }
        }
    }

    // Full interactive login.
    login(client).await
}

// ────────────────────────────── Provider implementation ────────────────

/// The Qwen OAuth provider, integrating with the rest of the CLI via the
/// `Provider` trait.
pub struct QwenOAuthProvider {
    client: reqwest::Client,
}

impl QwenOAuthProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client"); // safe: only fails on TLS init errors
        Self { client }
    }

    pub fn load_key(&mut self, _config: &AppConfig) {
        // OAuth doesn't use API keys - credentials are managed via device flow
    }

    /// Obtain a valid bearer token, running the interactive flow if necessary.
    async fn get_token(&self) -> Result<OAuthCredentials, QwenOAuthError> {
        ensure_valid_token(&self.client).await
    }
}

#[async_trait]
impl Provider for QwenOAuthProvider {
    fn id(&self) -> &str {
        "qwen"
    }

    fn name(&self) -> &str {
        "Qwen (OAuth)"
    }

    fn model(&self) -> &str {
        "qwen3-coder-plus"
    }

    async fn is_configured(&self) -> bool {
        // Check if we have cached credentials
        load_credentials().await.ok().flatten().is_some()
    }

    async fn setup(&mut self) -> Result<()> {
        login(&self.client).await?;
        Ok(())
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let creds = self.get_token().await?;

        // Build correct API URL - resource_url should be like "portal.qwen.ai"
        let url = format!(
            "https://{}/v1/chat/completions",
            creds
                .resource_url
                .trim_end_matches('/')
                .trim_start_matches("https://")
                .trim_start_matches("http://")
        );

        let body = serde_json::json!({
            "model": "qwen3-coder-plus",
            "messages": messages,
            "max_tokens": 1024,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&creds.access_token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Qwen API returned {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse {
            provider: "Qwen (OAuth)".into(),
            model: "qwen3-coder-plus".into(),
            content,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "qwen3-coder-plus".to_string(),
                name: "Qwen 3 Coder Plus".to_string(),
                description: "Coding specialized, 32K context".to_string(),
                is_default: true,
            },
            ModelInfo {
                id: "qwen2.5-72b-instruct".to_string(),
                name: "Qwen 2.5 72B Instruct".to_string(),
                description: "General purpose, 128K context".to_string(),
                is_default: false,
            },
            ModelInfo {
                id: "qwen2.5-coder-32b-instruct".to_string(),
                name: "Qwen 2.5 Coder 32B".to_string(),
                description: "Code generation".to_string(),
                is_default: false,
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Qwen and DashScope quotas vary by account, region, and release policy".to_string()
    }
}
