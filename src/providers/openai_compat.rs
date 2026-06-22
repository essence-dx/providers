//! One implementation that covers Groq, Cerebras, Mistral, OpenRouter,
//! Cohere, NVIDIA NIM, SambaNova, HuggingFace, GitHub Models, Fireworks,
//! DeepSeek, and Together AI. The only differences are `base_url`,
//! `api_key`, and `model`.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::provider::{ChatMessage, ChatResponse, ModelInfo, Provider};

// ── OpenAI-compatible request / response types ──

#[derive(Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
}

/// Configuration for a single OpenAI-compatible provider.
pub struct OpenAICompatProvider {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub default_model: String,
    pub api_key: Option<String>,
    pub signup_url: String,
    pub extra_headers: Vec<(String, String)>,
}

impl OpenAICompatProvider {
    pub fn new(
        id: &str,
        display_name: &str,
        base_url: &str,
        model: &str,
        signup_url: &str,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            base_url: base_url.into(),
            default_model: model.into(),
            api_key: None,
            signup_url: signup_url.into(),
            extra_headers: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }

    /// Attempt to load the API key from (1) .env file, (2) env var, then (3) config file.
    pub fn load_key(&mut self, config: &AppConfig) {
        // Priority 1: Check .env file with simple names (GROQ, GITHUB_MODELS, etc.)
        let simple_env_name = self.id.to_uppercase().replace('-', "_");
        if let Ok(v) = std::env::var(&simple_env_name)
            && !v.is_empty()
        {
            self.api_key = Some(v);
            return;
        }

        // Priority 2: Standard env var format
        let env_name = format!("{}_API_KEY", self.id.to_uppercase().replace('-', "_"));
        if let Ok(v) = std::env::var(&env_name)
            && !v.is_empty()
        {
            self.api_key = Some(v);
            return;
        }

        // Priority 3: Config file
        if let Some(k) = config.get_key(&self.id) {
            self.api_key = Some(k.to_string());
        }
    }
}

/// Write an API key to the .env file
fn write_to_env_file(provider_id: &str, api_key: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};

    let env_file_path = ".env";

    // Map provider IDs to .env variable names
    let env_var_name = match provider_id {
        "groq" => "GROQ".to_string(),
        "github-models" => "GITHUB_MODELS".to_string(),
        "cerebras" => "CEREBRAS".to_string(),
        "mistral" => "MISTRAL".to_string(),
        "openrouter" => "OPENROUTER".to_string(),
        "cohere" => "COHERE".to_string(),
        "nvidia" => "NVIDIA".to_string(),
        "sambanova" => "SAMBANOVA".to_string(),
        "huggingface" => "HUGGINGFACE".to_string(),
        "fireworks" => "FIREWORKS".to_string(),
        "deepseek" => "DEEPSEEK".to_string(),
        "together" => "TOGETHER".to_string(),
        "cloudflare" => "CLOUDFLARE".to_string(),
        _ => provider_id.to_uppercase().replace('-', "_"),
    };

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

/// Prompt the user for an API key interactively.
fn prompt_api_key(name: &str, url: &str) -> Result<String> {
    use std::io::{self, Write};

    println!();
    println!("  ┌─────────────────────────────────────────────────────┐");
    println!("  │  {} — API Key Setup", name);
    println!("  │");
    println!("  │  1. Go to: {}", url);
    println!("  │  2. Sign up (free, no credit card needed)");
    println!("  │  3. Copy your API key and paste below");
    println!("  └─────────────────────────────────────────────────────┘");
    println!();

    // Automatically open browser
    println!("  🌐 Opening {} in your browser...", url);
    if let Err(e) = open::that(url) {
        println!("  ⚠ Could not open browser: {}", e);
        println!("  Please manually go to: {}", url);
    }

    println!();
    print!("    API key: ");
    io::stdout().flush()?;

    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    let key = key.trim().to_string();
    if key.is_empty() {
        bail!("No API key provided");
    }
    Ok(key)
}

#[async_trait]
impl Provider for OpenAICompatProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.display_name
    }

    fn model(&self) -> &str {
        &self.default_model
    }

    async fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    async fn setup(&mut self) -> Result<()> {
        if self.api_key.is_some() {
            return Ok(());
        }
        let key = prompt_api_key(&self.display_name, &self.signup_url)?;

        // Write to .env file instead of config
        write_to_env_file(&self.id, &key)?;

        self.api_key = Some(key);
        Ok(())
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse> {
        let api_key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("{} not configured", self.display_name))?;

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let body = CompletionRequest {
            model: &self.default_model,
            messages,
            max_tokens: Some(1024),
        };

        let client = reqwest::Client::new();
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .bearer_auth(api_key)
            .json(&body);

        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("{} HTTP {}: {}", self.display_name, status.as_u16(), text);
        }

        let data: CompletionResponse = resp.json().await?;
        let content = data
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_else(|| "(empty)".into());

        Ok(ChatResponse {
            provider: self.display_name.clone(),
            model: self.default_model.clone(),
            content,
        })
    }

    fn get_models(&self) -> Vec<ModelInfo> {
        match self.id.as_str() {
            "groq" => vec![
                ModelInfo {
                    id: "llama-3.3-70b-versatile".to_string(),
                    name: "Llama 3.3 70B Versatile".to_string(),
                    description: "Current default, fastest inference".to_string(),
                    is_default: self.default_model == "llama-3.3-70b-versatile",
                },
                ModelInfo {
                    id: "llama-3.1-70b-versatile".to_string(),
                    name: "Llama 3.1 70B Versatile".to_string(),
                    description: "Stable, reliable".to_string(),
                    is_default: self.default_model == "llama-3.1-70b-versatile",
                },
                ModelInfo {
                    id: "llama-3.1-8b-instant".to_string(),
                    name: "Llama 3.1 8B Instant".to_string(),
                    description: "Ultra-fast, lightweight".to_string(),
                    is_default: self.default_model == "llama-3.1-8b-instant",
                },
                ModelInfo {
                    id: "mixtral-8x7b-32768".to_string(),
                    name: "Mixtral 8x7B".to_string(),
                    description: "Mixture of experts".to_string(),
                    is_default: self.default_model == "mixtral-8x7b-32768",
                },
            ],
            "cerebras" => vec![
                ModelInfo {
                    id: "llama3.1-8b".to_string(),
                    name: "Llama 3.1 8B".to_string(),
                    description: "Current default, 2,200 tokens/s".to_string(),
                    is_default: self.default_model == "llama3.1-8b",
                },
                ModelInfo {
                    id: "llama3.1-70b".to_string(),
                    name: "Llama 3.1 70B".to_string(),
                    description: "High capability".to_string(),
                    is_default: self.default_model == "llama3.1-70b",
                },
            ],
            "mistral" => vec![
                ModelInfo {
                    id: "mistral-small-latest".to_string(),
                    name: "Mistral Small Latest".to_string(),
                    description: "Current default, excellent reasoning".to_string(),
                    is_default: self.default_model == "mistral-small-latest",
                },
                ModelInfo {
                    id: "mistral-medium-latest".to_string(),
                    name: "Mistral Medium Latest".to_string(),
                    description: "Balanced performance".to_string(),
                    is_default: self.default_model == "mistral-medium-latest",
                },
                ModelInfo {
                    id: "mistral-large-latest".to_string(),
                    name: "Mistral Large Latest".to_string(),
                    description: "Highest capability".to_string(),
                    is_default: self.default_model == "mistral-large-latest",
                },
            ],
            "openrouter" => vec![
                ModelInfo {
                    id: "mistralai/mistral-small-3.1-24b-instruct:free".to_string(),
                    name: "Mistral Small 3.1 24B (Free)".to_string(),
                    description: "Current default, free tier".to_string(),
                    is_default: self.default_model
                        == "mistralai/mistral-small-3.1-24b-instruct:free",
                },
                ModelInfo {
                    id: "deepseek/deepseek-r1:free".to_string(),
                    name: "DeepSeek R1 (Free)".to_string(),
                    description: "Reasoning model, free tier".to_string(),
                    is_default: self.default_model == "deepseek/deepseek-r1:free",
                },
                ModelInfo {
                    id: "meta-llama/llama-3.2-3b-instruct:free".to_string(),
                    name: "Llama 3.2 3B (Free)".to_string(),
                    description: "Lightweight, free tier".to_string(),
                    is_default: self.default_model == "meta-llama/llama-3.2-3b-instruct:free",
                },
            ],
            "cohere" => vec![
                ModelInfo {
                    id: "command-a-03-2025".to_string(),
                    name: "Command A 03-2025".to_string(),
                    description: "Current default, strongest across domains".to_string(),
                    is_default: self.default_model == "command-a-03-2025",
                },
                ModelInfo {
                    id: "command-r-plus-08-2024".to_string(),
                    name: "Command R+ 08-2024".to_string(),
                    description: "Advanced enterprise tasks".to_string(),
                    is_default: self.default_model == "command-r-plus-08-2024",
                },
                ModelInfo {
                    id: "command-r-08-2024".to_string(),
                    name: "Command R 08-2024".to_string(),
                    description: "Balanced performance".to_string(),
                    is_default: self.default_model == "command-r-08-2024",
                },
            ],
            "nvidia" => vec![
                ModelInfo {
                    id: "meta/llama-3.3-70b-instruct".to_string(),
                    name: "Meta Llama 3.3 70B Instruct".to_string(),
                    description: "Current default".to_string(),
                    is_default: self.default_model == "meta/llama-3.3-70b-instruct",
                },
                ModelInfo {
                    id: "meta/llama-3.1-70b-instruct".to_string(),
                    name: "Meta Llama 3.1 70B Instruct".to_string(),
                    description: "Stable version".to_string(),
                    is_default: self.default_model == "meta/llama-3.1-70b-instruct",
                },
            ],
            "sambanova" => vec![
                ModelInfo {
                    id: "Meta-Llama-3.3-70B-Instruct".to_string(),
                    name: "Meta Llama 3.3 70B Instruct".to_string(),
                    description: "Current default".to_string(),
                    is_default: self.default_model == "Meta-Llama-3.3-70B-Instruct",
                },
                ModelInfo {
                    id: "Meta-Llama-3.1-70B-Instruct".to_string(),
                    name: "Meta Llama 3.1 70B Instruct".to_string(),
                    description: "Stable version".to_string(),
                    is_default: self.default_model == "Meta-Llama-3.1-70B-Instruct",
                },
                ModelInfo {
                    id: "Meta-Llama-3.1-8B-Instruct".to_string(),
                    name: "Meta Llama 3.1 8B Instruct".to_string(),
                    description: "Efficient".to_string(),
                    is_default: self.default_model == "Meta-Llama-3.1-8B-Instruct",
                },
            ],
            "huggingface" => vec![
                ModelInfo {
                    id: "meta-llama/Llama-3.2-3B-Instruct".to_string(),
                    name: "Llama 3.2 3B Instruct".to_string(),
                    description: "Current default, efficient".to_string(),
                    is_default: self.default_model == "meta-llama/Llama-3.2-3B-Instruct",
                },
                ModelInfo {
                    id: "mistralai/Mistral-7B-Instruct-v0.3".to_string(),
                    name: "Mistral 7B Instruct v0.3".to_string(),
                    description: "Balanced performance".to_string(),
                    is_default: self.default_model == "mistralai/Mistral-7B-Instruct-v0.3",
                },
            ],
            "github-models" => vec![
                ModelInfo {
                    id: "gpt-4o-mini".to_string(),
                    name: "GPT-4o Mini".to_string(),
                    description: "Current default, OpenAI's efficient model".to_string(),
                    is_default: self.default_model == "gpt-4o-mini",
                },
                ModelInfo {
                    id: "gpt-4o".to_string(),
                    name: "GPT-4o".to_string(),
                    description: "High capability when available".to_string(),
                    is_default: self.default_model == "gpt-4o",
                },
                ModelInfo {
                    id: "claude-3-haiku".to_string(),
                    name: "Claude 3 Haiku".to_string(),
                    description: "Anthropic's fast model".to_string(),
                    is_default: self.default_model == "claude-3-haiku",
                },
                ModelInfo {
                    id: "llama-3.1-70b-instruct".to_string(),
                    name: "Llama 3.1 70B Instruct".to_string(),
                    description: "Meta's model".to_string(),
                    is_default: self.default_model == "llama-3.1-70b-instruct",
                },
            ],
            "fireworks" => vec![ModelInfo {
                id: "accounts/fireworks/models/llama-v3p3-70b-instruct".to_string(),
                name: "Llama 3.3 70B Instruct".to_string(),
                description: "Current default".to_string(),
                is_default: self.default_model
                    == "accounts/fireworks/models/llama-v3p3-70b-instruct",
            }],
            "deepseek" => vec![
                ModelInfo {
                    id: "deepseek-chat".to_string(),
                    name: "DeepSeek Chat".to_string(),
                    description: "Current default".to_string(),
                    is_default: self.default_model == "deepseek-chat",
                },
                ModelInfo {
                    id: "deepseek-coder".to_string(),
                    name: "DeepSeek Coder".to_string(),
                    description: "Code generation specialized".to_string(),
                    is_default: self.default_model == "deepseek-coder",
                },
            ],
            "together" => vec![ModelInfo {
                id: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
                name: "Llama 3.3 70B Instruct Turbo".to_string(),
                description: "Current default".to_string(),
                is_default: self.default_model == "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            }],
            _ => vec![ModelInfo {
                id: self.default_model.clone(),
                name: format!("{} Default", self.display_name),
                description: "Default model for this provider".to_string(),
                is_default: true,
            }],
        }
    }

    fn get_free_tier_info(&self) -> String {
        match self.id.as_str() {
            "groq" => "Free tier; exact limits are model and account specific".to_string(),
            "cerebras" => "Free trial tier; limits vary by account".to_string(),
            "mistral" => "Free or evaluation tier; check account admin limits".to_string(),
            "openrouter" => {
                "Free model routes; quota varies by account and spend history".to_string()
            }
            "cohere" => "Free evaluation tier; quota varies by model and account".to_string(),
            "nvidia" => "Free developer and prototyping tier".to_string(),
            "sambanova" => "Free tier when eligible".to_string(),
            "huggingface" => "Free inference API".to_string(),
            "github-models" => "No-cost prototyping access when eligible".to_string(),
            "fireworks" => "Trial or prepaid access; verify current account status".to_string(),
            "deepseek" => "Promotional credits vary by account and region".to_string(),
            "together" => "Paid or promotional access; verify current account status".to_string(),
            _ => "Free tier available".to_string(),
        }
    }
}
