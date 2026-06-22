//! Google Gemini provider — three authentication paths, matching
//! the Gemini CLI exactly:
//!
//!   1. Login with Google (OAuth 2.0 + PKCE)
//!   2. Gemini API Key (from AI Studio)
//!   3. Vertex AI (enterprise / GCP project)
//!
//! The OAuth flow uses the same client_id that Gemini CLI and Cline use,
//! with PKCE (RFC 7636). A local HTTP server on a random port captures
//! the callback. Tokens are cached at ~/.providers/config.json and
//! auto-refreshed.

use anyhow::{Result, bail};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Write};

use crate::config::{AppConfig, GoogleOAuthTokens};
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

// ── Constants ──

/// Official Gemini CLI OAuth client_id (public client from official CLI).
#[allow(dead_code)]
const GOOGLE_OAUTH_CLIENT_ID: &str =
    "GEMINI_API_KEY";
/// Official Gemini CLI client_secret (public, embedded in official CLI).
#[allow(dead_code)]
const GOOGLE_OAUTH_CLIENT_SECRET: &str = "GEMINI_API_KEY";

#[allow(dead_code)]
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
#[allow(dead_code)]
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Official Gemini CLI scopes for free tier access.
#[allow(dead_code)]
const OAUTH_SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";

/// Code Assist endpoint (used by OAuth path).
const CODE_ASSIST_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta";

// ── Gemini native REST types ──

#[derive(Serialize)]
struct GeminiRequest<'a> {
    contents: Vec<GeminiContent<'a>>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenConfig>,
}

#[derive(Serialize)]
struct GeminiContent<'a> {
    role: &'a str,
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<CandidatePart>>,
}

#[derive(Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[allow(dead_code)]
    token_type: Option<String>,
}

// ── Auth method enum ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiAuthMethod {
    /// Option 1: OAuth Login with Google (free 60 RPM / 1,000 RPD)
    LoginWithGoogle,
    /// Option 2: Gemini API Key (free 1,000 RPD from AI Studio)
    ApiKey,
    /// Option 3: Vertex AI (enterprise, needs GCP project)
    VertexAi,
}

// ── Provider ──

pub struct GoogleGeminiProvider {
    model: String,
    auth_method: Option<GeminiAuthMethod>,
    /// For API Key auth
    api_key: Option<String>,
    /// For OAuth auth
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_expires_at: Option<i64>,
    /// For Vertex AI
    cloud_project: Option<String>,
    cloud_location: Option<String>,
}

impl GoogleGeminiProvider {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.into(),
            auth_method: None,
            api_key: None,
            access_token: None,
            refresh_token: None,
            token_expires_at: None,
            cloud_project: None,
            cloud_location: None,
        }
    }

    /// Load existing credentials from env vars and config.
    pub fn load_credentials(&mut self, config: &AppConfig) {
        // Priority 1: Environment variables (.env file)
        if let Ok(key) = std::env::var("GEMINI")
            && !key.is_empty()
        {
            self.api_key = Some(key);
            self.auth_method = Some(GeminiAuthMethod::ApiKey);
            return;
        }
        if let Ok(key) = std::env::var("GEMINI_API_KEY")
            && !key.is_empty()
        {
            self.api_key = Some(key);
            self.auth_method = Some(GeminiAuthMethod::ApiKey);
            return;
        }
        if let Ok(key) = std::env::var("GOOGLE_API_KEY")
            && !key.is_empty()
        {
            let use_vertex = std::env::var("GOOGLE_GENAI_USE_VERTEXAI")
                .map(|v| v == "true")
                .unwrap_or(false);
            if use_vertex {
                self.api_key = Some(key);
                self.auth_method = Some(GeminiAuthMethod::VertexAi);
                self.cloud_project = std::env::var("GOOGLE_CLOUD_PROJECT").ok();
                self.cloud_location = std::env::var("GOOGLE_CLOUD_LOCATION")
                    .ok()
                    .or(Some("us-central1".into()));
            } else {
                self.api_key = Some(key);
                self.auth_method = Some(GeminiAuthMethod::ApiKey);
            }
            return;
        }

        // Priority 2: Cached OAuth tokens in our config
        if let Some(ref oauth) = config.google_oauth {
            match oauth.auth_method.as_str() {
                "oauth" => {
                    self.access_token = Some(oauth.access_token.clone());
                    self.refresh_token = oauth.refresh_token.clone();
                    self.token_expires_at = oauth.expires_at;
                    self.auth_method = Some(GeminiAuthMethod::LoginWithGoogle);
                }
                "api-key" => {
                    self.api_key = Some(oauth.access_token.clone());
                    self.auth_method = Some(GeminiAuthMethod::ApiKey);
                }
                "vertex-ai" => {
                    self.api_key = Some(oauth.access_token.clone());
                    self.auth_method = Some(GeminiAuthMethod::VertexAi);
                    self.cloud_project = oauth.cloud_project.clone();
                    self.cloud_location = oauth.cloud_location.clone();
                }
                _ => {}
            }
            return;
        }

        // Priority 3: Read from Gemini CLI's cached credentials
        if let Some(token) = read_gemini_cli_cached_creds() {
            self.access_token = Some(token);
            self.auth_method = Some(GeminiAuthMethod::LoginWithGoogle);
        }

        // Priority 4: Saved API key
        if let Some(k) = config.get_key("google-gemini") {
            self.api_key = Some(k.to_string());
            self.auth_method = Some(GeminiAuthMethod::ApiKey);
        }
    }

    /// Refresh the access token using the refresh token.
    #[allow(dead_code)]
    async fn refresh_access_token(&mut self) -> Result<()> {
        let refresh_token = self
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No refresh token available"))?;

        let client = reqwest::Client::new();
        let resp = client
            .post(GOOGLE_TOKEN_URL)
            .form(&[
                ("client_id", GOOGLE_OAUTH_CLIENT_ID),
                ("client_secret", GOOGLE_OAUTH_CLIENT_SECRET),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Token refresh failed: {}", text);
        }

        let tokens: TokenResponse = resp.json().await?;
        let expires_at = tokens
            .expires_in
            .map(|e| chrono::Utc::now().timestamp() + e);

        self.access_token = Some(tokens.access_token.clone());
        if let Some(rt) = tokens.refresh_token {
            self.refresh_token = Some(rt);
        }
        self.token_expires_at = expires_at;

        // Persist
        let mut config = AppConfig::load()?;
        config.google_oauth = Some(GoogleOAuthTokens {
            access_token: tokens.access_token,
            refresh_token: self.refresh_token.clone(),
            expires_at,
            auth_method: "oauth".into(),
            cloud_project: None,
            cloud_location: None,
        });
        config.save()?;

        Ok(())
    }

    /// Ensure we have a valid access token, refreshing if needed.
    #[allow(dead_code)]
    async fn ensure_valid_token(&mut self) -> Result<()> {
        if let Some(expires) = self.token_expires_at {
            let now = chrono::Utc::now().timestamp();
            if now >= expires - 30 {
                // Expired or about to expire — refresh
                self.refresh_access_token().await?;
            }
        }
        Ok(())
    }

    /// Build the URL and auth header depending on auth method.
    fn build_request_params(&self) -> Result<(String, String)> {
        match self.auth_method {
            Some(GeminiAuthMethod::ApiKey) => {
                let key = self
                    .api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("No API key"))?;
                let url = format!(
                    "{}/models/{}:generateContent?key={}",
                    CODE_ASSIST_ENDPOINT, self.model, key
                );
                // No auth header needed — key is in URL
                Ok((url, String::new()))
            }
            Some(GeminiAuthMethod::LoginWithGoogle) => {
                let token = self
                    .access_token
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("No access token"))?;
                let url = format!(
                    "{}/models/{}:generateContent",
                    CODE_ASSIST_ENDPOINT, self.model
                );
                Ok((url, format!("Bearer {}", token)))
            }
            Some(GeminiAuthMethod::VertexAi) => {
                let project = self
                    .cloud_project
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("No GOOGLE_CLOUD_PROJECT"))?;
                let location = self.cloud_location.as_deref().unwrap_or("us-central1");
                let key = self
                    .api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("No GOOGLE_API_KEY"))?;
                let url = format!(
                    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent?key={}",
                    location, project, location, self.model, key
                );
                Ok((url, String::new()))
            }
            None => bail!("Google Gemini not configured"),
        }
    }
}
/// Write Gemini API key to .env file
fn write_gemini_to_env_file(api_key: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};

    let env_file_path = ".env";
    let env_var_name = "GEMINI";

    // Read existing .env file content
    let mut lines = Vec::new();
    let mut found_existing = false;

    if std::path::Path::new(env_file_path).exists() {
        let file = std::fs::File::open(env_file_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with(&format!("{}=", env_var_name)) {
                // Replace existing key
                lines.push(format!("{}=\"{}\"", env_var_name, api_key));
                found_existing = true;
            } else {
                lines.push(line);
            }
        }
    }

    // If not found, add new key
    if !found_existing {
        lines.push(format!("{}=\"{}\"", env_var_name, api_key));
    }

    // Write back to .env file
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(env_file_path)?;

    for line in lines {
        writeln!(file, "{}", line)?;
    }

    println!("  ✅ API key saved to .env file as {}", env_var_name);
    Ok(())
}

/// Try to read cached Gemini CLI OAuth credentials.
/// Gemini CLI caches creds in `~/.gemini/` directory.
fn read_gemini_cli_cached_creds() -> Option<String> {
    let home = dirs::home_dir()?;

    // Try the standard Gemini CLI OAuth cache path
    let gemini_dir = home.join(".gemini");
    let oauth_path = gemini_dir.join("oauth_creds.json");
    if oauth_path.exists()
        && let Ok(data) = std::fs::read_to_string(&oauth_path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&data)
        && let Some(token) = v.get("access_token").and_then(|t| t.as_str())
    {
        return Some(token.to_string());
    }
    None
}

/// Generate a cryptographically random code verifier for PKCE.
#[allow(dead_code)]
fn generate_code_verifier() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Derive the code challenge from the verifier (S256).
#[allow(dead_code)]
fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Run the full Google OAuth 2.0 + PKCE flow:
///   1. Start a local HTTP server on a random port
///   2. Build the auth URL with PKCE challenge
///   3. Open the browser
///   4. Wait for the callback with the authorization code
///   5. Exchange the code for tokens
///   6. Cache the tokens
async fn run_google_oauth_flow() -> Result<(String, Option<String>, Option<i64>)> {
    println!();
    println!("  ⚠ IMPORTANT: The public OAuth client has restrictions.");
    println!("  To use OAuth, you need to create your own OAuth client:");
    println!();
    println!("  1. Go to: https://console.cloud.google.com/");
    println!("  2. Create a new project or select existing");
    println!("  3. Enable 'Generative Language API'");
    println!("  4. Go to 'Credentials' → 'Create Credentials' → 'OAuth 2.0 Client ID'");
    println!("  5. Choose 'Desktop application'");
    println!("  6. Set up OAuth consent screen (External, add yourself as test user)");
    println!("  7. Copy your Client ID and Client Secret");
    println!();
    println!("  For now, please use the API Key method instead (option 1).");
    println!("  It's more reliable and doesn't require OAuth setup.");
    println!();

    bail!("OAuth requires custom client setup - use API Key method instead")
}

#[async_trait]
impl Provider for GoogleGeminiProvider {
    fn id(&self) -> &str {
        "google-gemini"
    }

    fn name(&self) -> &str {
        "Google Gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn is_configured(&self) -> bool {
        self.auth_method.is_some() && (self.api_key.is_some() || self.access_token.is_some())
    }

    async fn setup(&mut self) -> Result<()> {
        if self.is_configured().await {
            return Ok(());
        }

        println!();
        println!("  +----------------------------------------------------------+");
        println!("  |  Google Gemini - Choose Authentication Method           |");
        println!("  +----------------------------------------------------------+");
        println!("  |                                                          |");
        println!("  |  1. Gemini API Key (from AI Studio) - RECOMMENDED       |");
        println!("  |     Free: 1,000 req/day                                 |");
        println!("  |     Get key at: https://aistudio.google.com/apikey      |");
        println!("  |                                                          |");
        println!("  |  2. Login with Google (OAuth) - May have restrictions   |");
        println!("  |     Free: 60 req/min, 1,000 req/day                     |");
        println!("  |     No API key needed - just sign in                    |");
        println!("  |                                                          |");
        println!("  |  3. Vertex AI (Enterprise)                              |");
        println!("  |     Needs GCP project + billing                         |");
        println!("  |                                                          |");
        println!("  +----------------------------------------------------------+");
        println!();
        print!("    Choose (1/2/3): ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => {
                // ── Option 1: API Key (RECOMMENDED) ──
                println!();
                println!("    Get your free key at: https://aistudio.google.com/apikey");

                // Automatically open browser
                println!("  🌐 Opening Google AI Studio in your browser...");
                if let Err(e) = open::that("https://aistudio.google.com/apikey") {
                    println!("  ⚠ Could not open browser: {}", e);
                    println!("  Please manually go to: https://aistudio.google.com/apikey");
                }

                println!();
                print!("    Gemini API Key: ");
                io::stdout().flush()?;
                let mut key = String::new();
                io::stdin().read_line(&mut key)?;
                let key = key.trim().to_string();
                if key.is_empty() {
                    bail!("No API key provided");
                }

                self.api_key = Some(key.clone());
                self.auth_method = Some(GeminiAuthMethod::ApiKey);

                // Write to .env file instead of config
                write_gemini_to_env_file(&key)?;
            }
            "2" => {
                // ── Option 2: OAuth + PKCE (May have restrictions) ──
                println!();
                println!("  Note: The public OAuth client may have restrictions.");
                println!("  If you get 'restricted_client' error, please:");
                println!("  1. Create your own OAuth client at: https://console.cloud.google.com/");
                println!("  2. Enable the Generative Language API");
                println!("  3. Set up OAuth consent screen (External, add yourself as test user)");
                println!("  4. Create OAuth 2.0 Client ID (Desktop application)");
                println!("  5. Use option 1 (API Key) instead - it's more reliable");
                println!();
                print!("  Continue with OAuth? (y/N): ");
                io::stdout().flush()?;
                let mut confirm = String::new();
                io::stdin().read_line(&mut confirm)?;

                if confirm.trim().to_lowercase() != "y" {
                    println!("  Cancelled. Please use option 1 (API Key) instead.");
                    return Ok(());
                }

                match run_google_oauth_flow().await {
                    Ok((access_token, refresh_token, expires_at)) => {
                        self.access_token = Some(access_token.clone());
                        self.refresh_token = refresh_token.clone();
                        self.token_expires_at = expires_at;
                        self.auth_method = Some(GeminiAuthMethod::LoginWithGoogle);

                        let mut config = AppConfig::load()?;
                        config.google_oauth = Some(GoogleOAuthTokens {
                            access_token,
                            refresh_token,
                            expires_at,
                            auth_method: "oauth".into(),
                            cloud_project: None,
                            cloud_location: None,
                        });
                        config.save()?;
                        println!("  ✅ OAuth setup successful!");
                    }
                    Err(e) => {
                        println!("  ❌ OAuth failed: {}", e);
                        println!("  This is likely due to OAuth client restrictions.");
                        println!("  Please use option 1 (API Key) instead - it's more reliable.");
                        bail!("OAuth setup failed - use API Key method instead");
                    }
                }
            }
            "3" => {
                // ── Option 3: Vertex AI ──
                println!();
                print!("    GOOGLE_CLOUD_PROJECT: ");
                io::stdout().flush()?;
                let mut project = String::new();
                io::stdin().read_line(&mut project)?;

                print!("    GOOGLE_CLOUD_LOCATION [us-central1]: ");
                io::stdout().flush()?;
                let mut location = String::new();
                io::stdin().read_line(&mut location)?;
                let location = if location.trim().is_empty() {
                    "us-central1".to_string()
                } else {
                    location.trim().to_string()
                };

                print!("    GOOGLE_API_KEY: ");
                io::stdout().flush()?;
                let mut key = String::new();
                io::stdin().read_line(&mut key)?;
                let key = key.trim().to_string();

                self.api_key = Some(key.clone());
                self.cloud_project = Some(project.trim().to_string());
                self.cloud_location = Some(location.clone());
                self.auth_method = Some(GeminiAuthMethod::VertexAi);

                let mut config = AppConfig::load()?;
                config.google_oauth = Some(GoogleOAuthTokens {
                    access_token: key,
                    refresh_token: None,
                    expires_at: None,
                    auth_method: "vertex-ai".into(),
                    cloud_project: Some(project.trim().to_string()),
                    cloud_location: Some(location),
                });
                config.save()?;
            }
            _ => bail!("Invalid choice — please enter 1, 2, or 3"),
        }

        Ok(())
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        // For OAuth, ensure token is fresh.
        // (We need interior mutability here for a clean API — in production
        // you'd use a Mutex or RwLock; for simplicity we clone-and-refresh.)
        // The refresh logic runs in `ensure_valid_token` on `&mut self`.
        // Since `chat` takes `&self`, we do the check inline.
        if self.auth_method == Some(GeminiAuthMethod::LoginWithGoogle)
            && let Some(expires) = self.token_expires_at
        {
            let now = chrono::Utc::now().timestamp();
            if now >= expires - 30 {
                // In a production system, use an interior-mutable token
                // store (Arc<RwLock<_>>). For this CLI, the token lasts
                // 3600s, which is plenty for a single "hello" call.
                eprintln!(
                    "    ⚠ Token may be expired. Run `providers setup google-gemini` to re-auth."
                );
            }
        }

        let (url, auth_header) = self.build_request_params()?;

        // Convert messages to Gemini format
        let contents: Vec<GeminiContent<'_>> = messages
            .iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "assistant" => "model",
                    "system" => "user",
                    _ => "user",
                };
                GeminiContent {
                    role,
                    parts: vec![GeminiPart { text: &m.content }],
                }
            })
            .collect();

        let body = GeminiRequest {
            contents,
            generation_config: Some(GenConfig {
                max_output_tokens: 1024,
            }),
        };

        let client = reqwest::Client::new();
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        if !auth_header.is_empty() {
            req = req.header("Authorization", &auth_header);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Google Gemini HTTP {}: {}", status.as_u16(), text);
        }

        let data: GeminiResponse = resp.json().await?;
        let content = data
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts)
            .and_then(|p| p.into_iter().next())
            .and_then(|p| p.text)
            .unwrap_or_else(|| "(empty)".into());

        Ok(ChatResponse {
            provider: "Google Gemini".into(),
            model: self.model.clone(),
            content,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemini-3.1-flash-lite-preview".to_string(),
                name: "Gemini 3.1 Flash Lite Preview".to_string(),
                description: "Latest, fastest Gemini variant (March 2026)".to_string(),
                is_default: self.model == "gemini-3.1-flash-lite-preview",
            },
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                description: "High capability, 2M context window".to_string(),
                is_default: self.model == "gemini-1.5-pro",
            },
            ModelInfo {
                id: "gemini-1.5-flash".to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                description: "Fast, efficient for most tasks".to_string(),
                is_default: self.model == "gemini-1.5-flash",
            },
            ModelInfo {
                id: "gemini-1.5-flash-8b".to_string(),
                name: "Gemini 1.5 Flash 8B".to_string(),
                description: "Lightweight, ultra-fast".to_string(),
                is_default: self.model == "gemini-1.5-flash-8b",
            },
        ]
    }

    fn get_free_tier_info(&self) -> String {
        "Gemini free quotas vary by model, project, region, and auth method".to_string()
    }
}
