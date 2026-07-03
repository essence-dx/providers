pub mod dx_config;
pub mod auth;
pub mod catalog_archive;
pub mod config;
pub mod errors;
pub mod provider;
pub mod provider_metadata;
pub mod provider_metadata_export;
pub mod providers;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

use catalog_archive::{CatalogCommand, run_catalog_command};
use config::AppConfig;
use provider::{ChatMessage, Provider};
use provider_metadata_export::{
    PROVIDER_METADATA_SIDECAR_PATH, ProviderMetadataExportSource, build_provider_metadata_export,
    write_provider_metadata_sidecar,
};

use providers::anthropic::AnthropicProvider;
use providers::antigravity_web::AntigravityWeb;
use providers::chatgpt_web::ChatGptWeb;
use providers::claude_web::ClaudeWeb;
use providers::cloudflare::CloudflareProvider;
use providers::codex_web::CodexWeb;
use providers::gemini_web::GeminiWeb;
use providers::google_gemini::GoogleGeminiProvider;
use providers::google_gemini_oauth::GoogleGeminiOAuthProvider;
use providers::openai_compat::OpenAICompatProvider;
use providers::opencode_zen::OpenCodeZenProvider;
use providers::perplexity::PerplexityProvider;
use providers::qwen_oauth::QwenOAuthProvider;
use providers::replicate::ReplicateProvider;
use providers::rkyv_loader;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// Global provider data loaded once at startup using rkyv+memmap (38 μs)
static PROVIDER_DATA: OnceLock<&'static rkyv_loader::ArchivedProvidersData> = OnceLock::new();

/// Load provider data using ultra-fast rkyv+memmap (38 μs)
fn load_provider_data() -> Result<&'static rkyv_loader::ArchivedProvidersData> {
    match PROVIDER_DATA.get() {
        Some(data) => Ok(*data),
        None => {
            let data = rkyv_loader::load_providers("data/providers.rkyv")
                .map_err(|e| anyhow::anyhow!("Failed to load provider data: {}", e))?;
            let _ = PROVIDER_DATA.set(data);
            Ok(data)
        }
    }
}

#[derive(Parser)]
#[command(name = "providers")]
#[command(version)]
#[command(about = "Universal AI provider CLI - 184 catalog providers, 6,245 models, one tool")]
#[command(
    long_about = "Connect to 184 AI providers with 6,245 models from a single Rust CLI.\n\
                  Supports Google Gemini (OAuth + API Key + Vertex AI), Qwen OAuth,\n\
                  Groq, Cerebras, Mistral, OpenCode Zen, and 180+ more providers.\n\
                  \n\
                  Data loaded via rkyv+memmap in 38 microseconds (200x faster than Node.js)."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send "Hello!" to ALL configured providers
    Hello,
    /// Send a message to a specific provider
    Send {
        /// Provider ID (e.g. groq, gemini, qwen, cerebras, mistral)
        provider: String,
        /// Custom message (default: "Hello! Respond in one sentence.")
        #[arg(short, long, default_value = "Hello! Please respond in one sentence.")]
        message: String,
    },
    /// List ALL providers from database (184 providers) - default view
    List,
    /// List configured providers with API implementations (25 providers)
    Configured,
    /// List ALL providers with filters
    ListAll {
        /// Filter by capability (chat, embedding, image, audio)
        #[arg(short, long)]
        capability: Option<String>,
        /// Filter by source (litellm, models.dev, both)
        #[arg(short, long)]
        source: Option<String>,
        /// Show only providers with models
        #[arg(short, long)]
        with_models: bool,
    },
    /// Set up a specific provider interactively
    Setup {
        /// Provider ID to set up
        provider: String,
    },
    /// Set up ALL providers interactively
    SetupAll,
    /// Show all models supported by each provider
    Models {
        /// Output in JSON format for programmatic use
        #[arg(long)]
        json: bool,
    },
    /// Show models from working providers only
    WorkingModels {
        /// Output in JSON format for programmatic use
        #[arg(long)]
        json: bool,
        /// Show detailed daily limits and capabilities
        #[arg(long)]
        detailed: bool,
    },
    /// Export canonical provider identity, alias, and freemium metadata
    Metadata {
        /// Output in JSON format for programmatic use
        #[arg(long)]
        json: bool,
        /// Write the generated metadata sidecar for downstream DX/Zed readers
        #[arg(long)]
        write_sidecar: bool,
        /// Sidecar output path; defaults to data/provider-metadata.generated.json
        #[arg(long, value_name = "PATH", requires = "write_sidecar")]
        output: Option<PathBuf>,
    },
    /// Validate or refresh the generated provider catalog archive
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    /// Test all providers - auto-configure if needed, then send test message
    Test {
        /// Skip auto-configuration and only test already configured providers
        #[arg(long)]
        skip_setup: bool,
    },
}

/// Build the full roster of providers (including web-based OAuth scrapers).
fn build_providers(config: &AppConfig) -> Vec<Box<dyn Provider>> {
    let mut all: Vec<Box<dyn Provider>> = Vec::new();

    // === WEB-BASED PROVIDERS (Cookie Extraction / OAuth) ===

    // ChatGPT Web (Free) - Extract session from Chrome
    let chatgpt_web = ChatGptWeb::new();
    all.push(Box::new(chatgpt_web));

    // OpenAI Codex (ChatGPT Plus/Pro) - OAuth via ChatGPT
    let codex_web = CodexWeb::new();
    all.push(Box::new(codex_web));

    // Google Gemini Web (Free) - Extract session from Chrome
    let gemini_web = GeminiWeb::new();
    all.push(Box::new(gemini_web));

    // Google Antigravity (Free) - OAuth via Google account
    let antigravity_web = AntigravityWeb::new();
    all.push(Box::new(antigravity_web));

    // Claude Web (Free) - Extract session from Chrome
    let claude_web = ClaudeWeb::new();
    all.push(Box::new(claude_web));

    // === API KEY PROVIDERS ===

    // Google Gemini (API key, OAuth, and Vertex-compatible flows)
    let mut gemini = GoogleGeminiProvider::new("gemini-3.1-flash-lite-preview");
    gemini.load_credentials(config);
    all.push(Box::new(gemini));

    // Google Gemini OAuth (secure OAuth 2.0 flow)
    let gemini_oauth = GoogleGeminiOAuthProvider::new(config.clone());
    all.push(Box::new(gemini_oauth));

    // Qwen OAuth and DashScope-linked metadata
    let mut qwen = QwenOAuthProvider::new();
    qwen.load_key(config);
    all.push(Box::new(qwen));

    // Groq
    let mut groq = OpenAICompatProvider::new(
        "groq",
        "Groq",
        "https://api.groq.com/openai/v1",
        "llama-3.3-70b-versatile",
        "https://console.groq.com/keys",
    );
    groq.load_key(config);
    all.push(Box::new(groq));

    // Cerebras
    let mut cerebras = OpenAICompatProvider::new(
        "cerebras",
        "Cerebras",
        "https://api.cerebras.ai/v1",
        "llama3.1-8b",
        "https://cloud.cerebras.ai/",
    );
    cerebras.load_key(config);
    all.push(Box::new(cerebras));

    // Mistral and Codestral-compatible models
    let mut mistral = OpenAICompatProvider::new(
        "mistral",
        "Mistral",
        "https://api.mistral.ai/v1",
        "mistral-small-latest",
        "https://console.mistral.ai/api-keys/",
    );
    mistral.load_key(config);
    all.push(Box::new(mistral));

    // OpenRouter
    let mut openrouter = OpenAICompatProvider::new(
        "openrouter",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "mistralai/mistral-small-3.1-24b-instruct:free",
        "https://openrouter.ai/keys",
    );
    openrouter.load_key(config);
    all.push(Box::new(openrouter));

    // OpenCode Zen (public free models, paid models with API key)
    let mut opencode_zen = OpenCodeZenProvider::new();
    opencode_zen.load_key(config);
    all.push(Box::new(opencode_zen));

    // Cohere
    let mut cohere = OpenAICompatProvider::new(
        "cohere",
        "Cohere",
        "https://api.cohere.ai/compatibility/v1",
        "command-a-03-2025",
        "https://dashboard.cohere.com/api-keys",
    );
    cohere.load_key(config);
    all.push(Box::new(cohere));

    // NVIDIA NIM
    let mut nvidia = OpenAICompatProvider::new(
        "nvidia",
        "NVIDIA NIM",
        "https://integrate.api.nvidia.com/v1",
        "meta/llama-3.3-70b-instruct",
        "https://build.nvidia.com/",
    );
    nvidia.load_key(config);
    all.push(Box::new(nvidia));

    // SambaNova
    let mut sambanova = OpenAICompatProvider::new(
        "sambanova",
        "SambaNova",
        "https://api.sambanova.ai/v1",
        "Meta-Llama-3.3-70B-Instruct",
        "https://cloud.sambanova.ai/apis",
    );
    sambanova.load_key(config);
    all.push(Box::new(sambanova));

    // HuggingFace Inference
    let mut hf = OpenAICompatProvider::new(
        "huggingface",
        "HuggingFace",
        "https://router.huggingface.co/v1",
        "meta-llama/Llama-3.2-3B-Instruct",
        "https://huggingface.co/settings/tokens",
    );
    hf.load_key(config);
    all.push(Box::new(hf));

    // GitHub Models
    let mut github = OpenAICompatProvider::new(
        "github-models",
        "GitHub Models",
        "https://models.inference.ai.azure.com",
        "gpt-4o-mini",
        "https://github.com/marketplace/models",
    );
    github.load_key(config);
    all.push(Box::new(github));

    // Fireworks AI
    let mut fireworks = OpenAICompatProvider::new(
        "fireworks",
        "Fireworks AI",
        "https://api.fireworks.ai/inference/v1",
        "accounts/fireworks/models/llama-v3p3-70b-instruct",
        "https://fireworks.ai/account/api-keys",
    );
    fireworks.load_key(config);
    all.push(Box::new(fireworks));

    // DeepSeek
    let mut deepseek = OpenAICompatProvider::new(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "deepseek-chat",
        "https://platform.deepseek.com/api_keys",
    );
    deepseek.load_key(config);
    all.push(Box::new(deepseek));

    // Together AI
    let mut together = OpenAICompatProvider::new(
        "together",
        "Together AI",
        "https://api.together.xyz/v1",
        "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        "https://api.together.ai/settings/api-keys",
    );
    together.load_key(config);
    all.push(Box::new(together));

    // Cloudflare Workers AI
    let mut cf = CloudflareProvider::new();
    cf.load_key(config);
    all.push(Box::new(cf));

    // Anthropic Claude
    let mut anthropic = AnthropicProvider::new();
    anthropic.load_key(config);
    all.push(Box::new(anthropic));

    // Perplexity AI (Agent API launched 2026)
    let mut perplexity = PerplexityProvider::new();
    perplexity.load_key(config);
    all.push(Box::new(perplexity));

    // Replicate (popular AI model hosting)
    let mut replicate = ReplicateProvider::new();
    replicate.load_key(config);
    all.push(Box::new(replicate));

    all
}

fn working_provider_ids() -> &'static [&'static str] {
    &[
        "google-gemini",
        "qwen",
        "groq",
        "cerebras",
        "mistral",
        "opencode-zen",
        "cohere",
        "sambanova",
        "github-models",
    ]
}

fn provider_metadata_export_source() -> ProviderMetadataExportSource {
    ProviderMetadataExportSource {
        repo: provider_metadata_repo_root(),
        commit: provider_metadata_commit(),
        generated_at: chrono::Utc::now(),
    }
}

fn provider_metadata_repo_root() -> String {
    git_stdout(&["rev-parse", "--show-toplevel"]).unwrap_or_else(|| {
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    })
}

fn provider_metadata_commit() -> Option<String> {
    let explicit = std::env::var("DX_PROVIDERS_COMMIT").ok();
    resolve_provider_metadata_commit(explicit.as_deref(), || git_stdout(&["rev-parse", "HEAD"]))
}

fn resolve_provider_metadata_commit(
    explicit: Option<&str>,
    git_head: impl FnOnce() -> Option<String>,
) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|commit| !commit.is_empty())
        .map(ToString::to_string)
        .or_else(git_head)
}

fn git_stdout(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn command_requires_provider_config(command: &Commands) -> bool {
    !matches!(
        command,
        Commands::Metadata { .. } | Commands::Catalog { .. }
    )
}

fn run_metadata_command(json: bool, write_sidecar: bool, output: Option<&Path>) -> Result<()> {
    let source = provider_metadata_export_source();
    let export = build_provider_metadata_export(source.clone());

    if write_sidecar {
        let sidecar_path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(PROVIDER_METADATA_SIDECAR_PATH));
        let write = write_provider_metadata_sidecar(&sidecar_path, source)?;

        if !json {
            println!();
            println!("  Provider Metadata Sidecar");
            println!("  Path: {}", write.path.display());
            println!("  Providers: {}", write.provider_count);
            println!("  Aliases: {}", write.alias_count);
            println!("  Hash: {}", write.content_sha256);
            println!();
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&export)?);
    } else if !write_sidecar {
        println!();
        println!("  Provider Metadata Export");
        println!("  Schema: {}", export.schema);
        println!("  Providers: {}", export.providers.len());
        println!("  Aliases: {}", export.alias_index.len());
        println!("  Sidecar: {}", PROVIDER_METADATA_SIDECAR_PATH);
        println!("  Use --json for machine-readable output.");
        println!("  Use --write-sidecar to refresh the generated sidecar.");
        println!();
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let dx = dx_config::ProvidersDxConfig::load();
    let _ = std::fs::create_dir_all(&dx.sr_dir);
    let _ = std::fs::create_dir_all(&dx.receipts_dir);

    let cli = Cli::parse();
    let command = cli.command;

    if let Commands::Metadata {
        json,
        write_sidecar,
        output,
    } = &command
    {
        let ret = run_metadata_command(*json, *write_sidecar, output.as_deref());
        dx.write_sr("providers", &[("tool", "providers"), ("action", "metadata"), ("status", "ok")])?;
        dx.write_global_sr("providers", &[("tool", "providers"), ("action", "metadata"), ("status", "ok")])?;
        if let Some(status) = dx.read_status("providers") {
            eprintln!("[providers] metadata sr cache verified: {} entries", status.len());
        }
        return ret;
    }
    if let Commands::Catalog { command } = &command {
        let ret = run_catalog_command(command);
        dx.write_sr("providers", &[("tool", "providers"), ("action", "catalog"), ("status", "ok")])?;
        dx.write_global_sr("providers", &[("tool", "providers"), ("action", "catalog"), ("status", "ok")])?;
        if let Some(status) = dx.read_status("providers") {
            eprintln!("[providers] catalog sr cache verified: {} entries", status.len());
        }
        return ret;
    }

    debug_assert!(command_requires_provider_config(&command));

    // Load .env file at startup
    dotenv::dotenv().ok();

    // Load provider data using rkyv+memmap (38 μs - ultra-fast!)
    let provider_data = load_provider_data()?;

    let config = AppConfig::load()?;

    match command {
        Commands::List => {
            println!();
            println!("  +-----------------------------------------------------------------------+");
            println!("  |  All Providers Database                                              |");
            println!("  +-----------------------------------------------------------------------+");
            println!();

            println!(
                "  {:<4} {:<25} {:<35} {:<12} {:<8} {:<8} Models",
                "#", "ID", "Name", "Source", "Features", "API URL"
            );
            println!("  {}", "-".repeat(120));

            for (i, p) in provider_data.providers.iter().enumerate() {
                let mut features = String::new();
                if p.supports_chat {
                    features.push('C');
                }
                if p.supports_embedding {
                    features.push('E');
                }
                if p.supports_image {
                    features.push('I');
                }
                if p.supports_audio {
                    features.push('A');
                }
                if features.is_empty() {
                    features = "-".to_string();
                }

                // Truncate API URL if too long
                let api_url = p.api_url.as_str();
                let api_display = if api_url.len() > 35 {
                    format!("{}...", &api_url[..32])
                } else {
                    api_url.to_string()
                };

                println!(
                    "  {:<4} {:<25} {:<35} {:<12} {:<8} {:<38} {}",
                    i + 1,
                    p.id.as_str(),
                    p.name.as_str(),
                    p.source.as_str(),
                    features,
                    api_display,
                    p.model_count,
                );
            }

            println!();
            println!("  {}", "=".repeat(120));
            println!("  📊 Total: {} providers", provider_data.total_providers);
            println!();
            println!("  Legend:");
            println!("    Features: C=Chat, E=Embedding, I=Image, A=Audio");
            println!("    Source: litellm, models.dev, or litellm+models.dev (merged)");
            println!();
            println!("  Commands:");
            println!(
                "    {} - Show configured providers with API keys",
                "providers configured".bold()
            );
            println!(
                "    {} - Filter providers (--capability, --source, --with-models)",
                "providers list-all".bold()
            );
            println!();
        }

        Commands::Configured => {
            println!();
            println!("  +-----------------------------------------------------------------------+");
            println!("  |  Configured Providers - API Implementation Status                    |");
            println!("  +-----------------------------------------------------------------------+");
            println!();
            println!(
                "  📊 Database: {} total providers, {} models",
                provider_data.total_providers, provider_data.total_models
            );
            println!("  🔧 Showing: 25 providers with API implementations");
            println!();

            let providers = build_providers(&config);
            println!(
                "  {:<4} {:<14} {:<25} {:<38} Status",
                "#", "ID", "Provider", "Model"
            );
            println!("  {}", "-".repeat(90));

            for (i, p) in providers.iter().enumerate() {
                let ok = p.is_configured().await;
                let status = if ok {
                    "Ready".green().to_string()
                } else {
                    "Not configured".yellow().to_string()
                };
                println!(
                    "  {:<4} {:<14} {:<25} {:<38} {}",
                    i + 1,
                    p.id(),
                    p.name(),
                    p.model(),
                    status,
                );
            }
            println!();
            println!(
                "  Run: {} or {}",
                "providers setup <id>".bold(),
                "providers setup-all".bold()
            );
            println!();
        }

        Commands::ListAll {
            capability,
            source,
            with_models,
        } => {
            println!();
            println!("  +-----------------------------------------------------------------------+");
            println!("  |  All Providers from Database (184 providers)                         |");
            println!("  +-----------------------------------------------------------------------+");
            println!();
            println!(
                "  📊 Database v{} - Generated: {}",
                provider_data.version.as_str(),
                provider_data.generated_at.as_str()
            );
            println!(
                "  📦 Total: {} providers, {} models",
                provider_data.total_providers, provider_data.total_models
            );
            println!();

            // Filter providers
            let mut filtered_providers: Vec<_> = provider_data.providers.iter().collect();

            if let Some(cap) = &capability {
                filtered_providers.retain(|p| match cap.to_lowercase().as_str() {
                    "chat" => p.supports_chat,
                    "embedding" => p.supports_embedding,
                    "image" => p.supports_image,
                    "audio" => p.supports_audio,
                    _ => true,
                });
            }

            if let Some(src) = &source {
                filtered_providers.retain(|p| match src.to_lowercase().as_str() {
                    "litellm" => p.source.as_str() == "litellm",
                    "models.dev" => p.source.as_str() == "models.dev",
                    "both" => p.source.as_str() == "both",
                    _ => true,
                });
            }

            if with_models {
                filtered_providers.retain(|p| p.model_count > 0);
            }

            println!(
                "  {:<4} {:<20} {:<30} {:<8} {:<10} Models",
                "#", "ID", "Name", "Source", "Features"
            );
            println!("  {}", "-".repeat(90));

            for (i, p) in filtered_providers.iter().enumerate() {
                let mut features = String::new();
                if p.supports_chat {
                    features.push('C');
                }
                if p.supports_embedding {
                    features.push('E');
                }
                if p.supports_image {
                    features.push('I');
                }
                if p.supports_audio {
                    features.push('A');
                }
                if features.is_empty() {
                    features = "-".to_string();
                }

                println!(
                    "  {:<4} {:<20} {:<30} {:<8} {:<10} {}",
                    i + 1,
                    p.id.as_str(),
                    p.name.as_str(),
                    p.source.as_str(),
                    features,
                    p.model_count,
                );
            }

            println!();
            println!("  {}", "=".repeat(75));
            println!("  📈 Showing {} providers", filtered_providers.len());
            println!();
            println!("  Legend: C=Chat, E=Embedding, I=Image, A=Audio");
            println!();
            println!("  Filters:");
            println!("    --capability <chat|embedding|image|audio>");
            println!("    --source <litellm|models.dev|both>");
            println!("    --with-models (only show providers with models)");
            println!();
        }

        Commands::Setup { provider } => {
            let mut providers = build_providers(&config);
            let found = providers.iter_mut().find(|p| {
                p.id() == provider || p.name().to_lowercase().contains(&provider.to_lowercase())
            });
            match found {
                Some(p) => {
                    p.setup().await?;
                    println!("  {} configured!", p.name());
                }
                None => {
                    eprintln!("  '{}' not found. Run 'providers list'.", provider);
                }
            }
        }

        Commands::SetupAll => {
            let mut providers = build_providers(&config);
            for p in providers.iter_mut() {
                if p.is_configured().await {
                    println!("  {} already configured", p.name());
                } else {
                    println!("\n  Setting up {}...", p.name().bold());
                    match p.setup().await {
                        Ok(()) => println!("  {} ready!", p.name()),
                        Err(e) => {
                            println!("  {} skipped: {}", p.name(), e)
                        }
                    }
                }
            }
        }

        Commands::Hello => {
            println!();
            println!("  +-----------------------------------------------------------------------+");
            println!("  |  Sending 'Hello!' to all configured providers...                    |");
            println!("  +-----------------------------------------------------------------------+");
            println!();

            let providers = build_providers(&config);
            let messages = vec![ChatMessage {
                role: "user".into(),
                content: "Hello! Please respond in one sentence.".into(),
            }];

            let mut ok = 0usize;
            let mut fail = 0usize;
            let mut skip = 0usize;

            for p in &providers {
                if !p.is_configured().await {
                    println!("  {} - skipped (not configured)", p.name().dimmed());
                    skip += 1;
                    continue;
                }

                print!("  {} ({})... ", p.name().bold(), p.model());

                match p.chat(&messages).await {
                    Ok(resp) => {
                        ok += 1;
                        println!("{}", "OK".green());
                        println!("    {}", resp.content.trim().dimmed());
                        println!();
                    }
                    Err(e) => {
                        fail += 1;
                        println!("{}", "ERROR".red());
                        println!("    {}", e.to_string().red());
                        println!();
                    }
                }
            }

            println!("  {}", "-".repeat(60));
            println!(
                "  {} succeeded, {} failed, {} skipped",
                ok.to_string().green(),
                fail.to_string().red(),
                skip.to_string().yellow(),
            );
        }

        Commands::Send { provider, message } => {
            let providers = build_providers(&config);
            let found = providers.iter().find(|p| {
                p.id() == provider || p.name().to_lowercase().contains(&provider.to_lowercase())
            });
            match found {
                Some(p) => {
                    if !p.is_configured().await {
                        eprintln!(
                            "  {} not configured. Run: providers setup {}",
                            p.name(),
                            p.id()
                        );
                        return Ok(());
                    }
                    println!("  {} ({})...", p.name().bold(), p.model());
                    match p
                        .chat(&[ChatMessage {
                            role: "user".into(),
                            content: message,
                        }])
                        .await
                    {
                        Ok(resp) => println!("\n  {}", resp.content.trim()),
                        Err(e) => eprintln!("  Error: {}", e),
                    }
                }
                None => {
                    eprintln!("  '{}' not found. Run 'providers list'.", provider);
                }
            }
        }

        Commands::WorkingModels { json, detailed } => {
            let providers = build_providers(&config);
            let working_providers: Vec<_> = providers
                .iter()
                .filter(|p| working_provider_ids().contains(&p.id()))
                .collect();

            if json {
                // JSON output for programmatic use
                let mut provider_list = Vec::new();

                for provider in &working_providers {
                    let provider_info = provider.get_provider_info().await;
                    let mut provider_json = serde_json::json!({
                        "id": provider_info.id,
                        "name": provider_info.name,
                        "status": provider_info.status,
                        "models": provider_info.models,
                        "free_tier": provider_info.free_tier,
                        "identity": provider_info.identity,
                        "freemium": provider_info.freemium
                    });

                    if detailed {
                        // Add detailed daily limits and capabilities
                        let daily_limits = match provider.id() {
                            "cerebras" => serde_json::json!({
                                "daily_requests": "account-specific free trial quota",
                                "daily_tokens": "1,000,000",
                                "rate_limit": "account-specific",
                                "inference_speed": "2,200 tokens/s"
                            }),
                            "groq" => serde_json::json!({
                                "daily_requests": "model-specific free quota",
                                "rate_limit": "30 RPM",
                                "inference_speed": "Fastest available"
                            }),
                            "qwen" => serde_json::json!({
                                "daily_requests": "account and region specific",
                                "rate_limit": "account-specific",
                                "specialization": "Coding tasks"
                            }),
                            "google-gemini" => serde_json::json!({
                                "daily_requests": "auth, model, and region specific",
                                "rate_limit": "account-specific"
                            }),
                            "mistral" => serde_json::json!({
                                "daily_requests": "admin limits determine current quota",
                                "monthly_tokens": "account-specific",
                                "specialization": "Reasoning tasks"
                            }),
                            "cohere" => serde_json::json!({
                                "monthly_requests": "1,000 evaluation calls when eligible",
                                "rate_limit": "20 RPM for common chat models"
                            }),
                            "sambanova" => serde_json::json!({
                                "daily_requests": "free-tier account quota when eligible",
                                "model_size": "70B parameters"
                            }),
                            "github-models" => serde_json::json!({
                                "daily_requests": "GitHub account and plan specific",
                                "models": "no-cost prototyping access when eligible"
                            }),
                            "opencode-zen" => serde_json::json!({
                                "access": "public free models",
                                "paid_models": "OPENCODE_API_KEY required"
                            }),
                            _ => serde_json::json!({}),
                        };
                        provider_json["daily_limits"] = daily_limits;
                    }

                    provider_list.push(provider_json);
                }

                let models_data = serde_json::json!({
                    "working_providers": provider_list,
                    "generated_at": provider_data.generated_at.as_str(),
                    "database_version": provider_data.version.as_str(),
                    "total_providers_in_database": provider_data.total_providers,
                    "total_models_in_database": provider_data.total_models,
                    "total_working_providers": provider_list.len(),
                    "total_models": provider_list.iter()
                        .map(|p| p["models"].as_array().unwrap_or(&vec![]).len())
                        .sum::<usize>(),
                    "estimated_daily_capacity": "provider-specific free and freemium quotas"
                });

                println!("{}", serde_json::to_string_pretty(&models_data)?);
            } else {
                // Human-readable output
                println!();
                println!(
                    "  +-----------------------------------------------------------------------+"
                );
                println!(
                    "  |  Working AI Providers & Models (March 12, 2026)                     |"
                );
                println!(
                    "  +-----------------------------------------------------------------------+"
                );
                println!();
                println!(
                    "  📊 Database: {} providers, {} models available",
                    provider_data.total_providers, provider_data.total_models
                );
                println!();

                let mut total_models = 0;
                for provider in &working_providers {
                    let provider_info = provider.get_provider_info().await;

                    println!(
                        "  {} - {}",
                        provider_info.name.bold().green(),
                        "✅ Working".green()
                    );

                    for model in &provider_info.models {
                        let marker = if model.is_default { "●" } else { "○" };
                        println!("    {} {} ({})", marker, model.name, model.description);
                        total_models += 1;
                    }

                    // Add daily limits info
                    let daily_info = match provider.id() {
                        "cerebras" => "Free trial token quota when eligible - 2,200 tokens/s class",
                        "groq" => "Model-specific free quota - 30 RPM class - fast inference",
                        "qwen" => {
                            "Qwen and DashScope quotas vary by account, region, and release policy"
                        }
                        "google-gemini" => {
                            "Gemini quotas vary by auth method, model, project, and region"
                        }
                        "mistral" => "Free/evaluation tier is controlled by account admin limits",
                        "opencode-zen" => {
                            "Public free Zen models; paid Zen models need OPENCODE_API_KEY"
                        }
                        "cohere" => "Free evaluation calls when eligible - enterprise-grade models",
                        "sambanova" => "Free-tier quota when eligible - 70B model access",
                        "github-models" => {
                            "No-cost prototyping quota depends on GitHub account and plan"
                        }
                        _ => "Free tier available",
                    };

                    if detailed {
                        println!("    📊 {}", daily_info);
                    }

                    println!("    Free Tier: {}", provider_info.free_tier);
                    println!();
                }

                println!("  {}", "=".repeat(75));
                println!(
                    "  📈 {} working providers with {} total models",
                    working_providers.len().to_string().bold(),
                    total_models.to_string().bold()
                );
                println!("  🚀 Capacity: provider-specific free and freemium quotas");
                println!("  💡 Use --detailed flag for daily limits breakdown");
                println!("  📋 Use --json flag for machine-readable output");
                println!();
            }
        }

        Commands::Models { json } => {
            let providers = build_providers(&config);

            if json {
                // JSON output for programmatic use
                let mut provider_list = Vec::new();

                for provider in &providers {
                    let provider_info = provider.get_provider_info().await;
                    provider_list.push(serde_json::json!({
                        "id": provider_info.id,
                        "name": provider_info.name,
                        "status": provider_info.status,
                        "models": provider_info.models,
                        "free_tier": provider_info.free_tier,
                        "identity": provider_info.identity,
                        "freemium": provider_info.freemium
                    }));
                }

                let models_data = serde_json::json!({
                    "providers": provider_list,
                    "generated_at": provider_data.generated_at.as_str(),
                    "database_version": provider_data.version.as_str(),
                    "total_providers_in_database": provider_data.total_providers,
                    "total_models_in_database": provider_data.total_models,
                    "configured_providers": provider_list.len(),
                    "total_models": provider_list.iter()
                        .map(|p| p["models"].as_array().unwrap_or(&vec![]).len())
                        .sum::<usize>()
                });

                println!("{}", serde_json::to_string_pretty(&models_data)?);
            } else {
                // Human-readable output
                println!();
                println!(
                    "  +-----------------------------------------------------------------------+"
                );
                println!(
                    "  |  AI Models Available by Provider (March 12, 2026)                   |"
                );
                println!(
                    "  +-----------------------------------------------------------------------+"
                );
                println!();
                println!(
                    "  📊 Database: {} providers, {} models (v{})",
                    provider_data.total_providers,
                    provider_data.total_models,
                    provider_data.version.as_str()
                );
                println!();

                for provider in &providers {
                    let provider_info = provider.get_provider_info().await;
                    let status_color = if provider_info.status == "ready" {
                        provider_info.name.bold().green()
                    } else {
                        provider_info.name.bold().yellow()
                    };

                    let status_text = if provider_info.status == "ready" {
                        "Ready".green()
                    } else {
                        "Not configured".yellow()
                    };

                    println!("  {} - {}", status_color, status_text);

                    for model in &provider_info.models {
                        let marker = if model.is_default { "●" } else { "○" };
                        println!("    {} {} ({})", marker, model.name, model.description);
                    }

                    println!("    Free Tier: {}", provider_info.free_tier);
                    println!();
                }

                println!("  {}", "=".repeat(75));
                println!(
                    "  {} Use 'providers send <provider> \"switch to <model>\"' to change models",
                    "Tip:".bold()
                );
                println!(
                    "  {} ● = default model, ○ = available model",
                    "Legend:".bold()
                );
                println!(
                    "  {} Use --json flag for machine-readable output",
                    "Tip:".bold()
                );
                println!();
            }
        }

        Commands::Metadata {
            json,
            write_sidecar,
            output,
        } => {
            return run_metadata_command(json, write_sidecar, output.as_deref());
        }

        Commands::Catalog { command } => {
            return run_catalog_command(&command);
        }

        Commands::Test { skip_setup } => {
            println!();
            println!("  +-----------------------------------------------------------------------+");
            println!("  |  Testing All Providers - Auto-Configure & Test                      |");
            println!("  +-----------------------------------------------------------------------+");
            println!();

            let mut providers = build_providers(&config);
            let messages = vec![ChatMessage {
                role: "user".into(),
                content: "Hello! Please respond with just 'OK' in one word.".into(),
            }];

            let mut ok = 0usize;
            let mut fail = 0usize;
            let mut skip = 0usize;
            let mut configured = 0usize;

            for p in providers.iter_mut() {
                // Check if provider is configured
                if !p.is_configured().await {
                    if skip_setup {
                        println!(
                            "  {} - {} (not configured)",
                            p.name().dimmed(),
                            "SKIPPED".yellow()
                        );
                        skip += 1;
                        continue;
                    }

                    // Auto-configure provider
                    println!(
                        "  {} - {} (not configured)",
                        p.name().bold(),
                        "CONFIGURING".yellow()
                    );
                    println!("    Setting up {}...", p.name());

                    match p.setup().await {
                        Ok(()) => {
                            println!("    {} configured successfully!", p.name().green());
                            configured += 1;
                        }
                        Err(e) => {
                            println!("    {} setup failed: {}", p.name().red(), e);
                            println!("    {} - {}", p.name().dimmed(), "SKIPPED".yellow());
                            skip += 1;
                            continue;
                        }
                    }
                }

                // Test provider
                print!("  {} ({})... ", p.name().bold(), p.model());

                match p.chat(&messages).await {
                    Ok(resp) => {
                        ok += 1;
                        println!("{}", "✅ OK".green());
                        println!("    Response: {}", resp.content.trim().dimmed());
                        println!();
                    }
                    Err(e) => {
                        fail += 1;
                        println!("{}", "❌ ERROR".red());
                        println!("    Error: {}", e.to_string().red());
                        println!();
                    }
                }
            }

            println!("  {}", "=".repeat(75));
            println!("  📊 Test Results:");
            println!(
                "    {} providers tested successfully",
                ok.to_string().green().bold()
            );
            println!("    {} providers failed", fail.to_string().red().bold());
            println!("    {} providers skipped", skip.to_string().yellow().bold());
            if configured > 0 {
                println!(
                    "    {} providers newly configured",
                    configured.to_string().cyan().bold()
                );
            }
            println!();
            println!("  💡 Tip: Use 'providers working-models --detailed' to see daily limits");
            println!();
        }
    }

    dx.write_sr("providers", &[("tool", "providers"), ("action", "run"), ("status", "ok")])?;
    dx.write_global_sr("providers", &[("tool", "providers"), ("action", "run"), ("status", "ok")])?;
    if let Some(status) = dx.read_status("providers") {
        eprintln!("[providers] run sr cache verified: {} entries", status.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_metadata::metadata_for_provider_id;

    #[test]
    fn every_runtime_provider_has_metadata() {
        let config = AppConfig::default();
        let providers = build_providers(&config);

        for provider in providers {
            assert!(
                metadata_for_provider_id(provider.id()).is_some(),
                "missing metadata for provider {}",
                provider.id()
            );
        }
    }

    #[test]
    fn working_provider_roster_includes_opencode_zen() {
        assert!(working_provider_ids().contains(&"opencode-zen"));
    }

    #[test]
    fn metadata_command_does_not_require_provider_config() {
        let metadata = Commands::Metadata {
            json: false,
            write_sidecar: false,
            output: None,
        };
        let catalog = Commands::Catalog {
            command: CatalogCommand::Validate {
                path: PathBuf::from("data/providers.rkyv"),
            },
        };
        assert!(!command_requires_provider_config(&metadata));
        assert!(!command_requires_provider_config(&catalog));
        assert!(command_requires_provider_config(&Commands::Configured));
    }

    #[test]
    fn metadata_commit_resolver_prefers_explicit_commit() {
        let commit = resolve_provider_metadata_commit(Some("abc123"), || {
            panic!("git fallback must not run when explicit commit is available")
        });

        assert_eq!(commit.as_deref(), Some("abc123"));
    }

    #[test]
    fn metadata_commit_resolver_falls_back_to_git_head() {
        let commit = resolve_provider_metadata_commit(None, || Some("def456".to_string()));

        assert_eq!(commit.as_deref(), Some("def456"));
    }
}
