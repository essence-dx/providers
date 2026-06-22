use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessKind {
    BrowserSession,
    CatalogOnly,
    FreeCredits,
    FreeTier,
    LocalRuntime,
    Paid,
    PremiumAccount,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    BrowserSession,
    CloudAccount,
    LocalRuntime,
    OAuth,
    PublicKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExposureStatus {
    CatalogOnly,
    Implemented,
    PendingBackend,
    VerifiedWorking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub canonical_id: String,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub database_ids: Vec<String>,
    pub runtime_id: Option<String>,
    pub exposure_status: ExposureStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreemiumMetadata {
    pub access: AccessKind,
    pub auth: Vec<AuthKind>,
    pub env_vars: Vec<String>,
    pub note: String,
    pub free_model_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub canonical_id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub database_ids: &'static [&'static str],
    pub runtime_id: Option<&'static str>,
    pub exposure_status: ExposureStatus,
    pub access: AccessKind,
    pub auth: &'static [AuthKind],
    pub env_vars: &'static [&'static str],
    pub freemium_note: &'static str,
    pub free_model_ids: &'static [&'static str],
}

impl ProviderMetadata {
    pub fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            canonical_id: self.canonical_id.to_string(),
            display_name: self.display_name.to_string(),
            aliases: self.aliases.iter().map(|value| value.to_string()).collect(),
            database_ids: self
                .database_ids
                .iter()
                .map(|value| value.to_string())
                .collect(),
            runtime_id: self.runtime_id.map(str::to_string),
            exposure_status: self.exposure_status,
        }
    }

    pub fn freemium(&self) -> FreemiumMetadata {
        FreemiumMetadata {
            access: self.access,
            auth: self.auth.to_vec(),
            env_vars: self
                .env_vars
                .iter()
                .map(|value| value.to_string())
                .collect(),
            note: self.freemium_note.to_string(),
            free_model_ids: self
                .free_model_ids
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }
}

const API_KEY: &[AuthKind] = &[AuthKind::ApiKey];
const BROWSER_SESSION: &[AuthKind] = &[AuthKind::BrowserSession];
const CLOUD_ACCOUNT_API_KEY: &[AuthKind] = &[AuthKind::CloudAccount, AuthKind::ApiKey];
const LOCAL_RUNTIME: &[AuthKind] = &[AuthKind::LocalRuntime];
const OAUTH: &[AuthKind] = &[AuthKind::OAuth];
const PUBLIC_OR_API_KEY: &[AuthKind] = &[AuthKind::PublicKey, AuthKind::ApiKey];

static PROVIDER_METADATA: &[ProviderMetadata] = &[
    ProviderMetadata {
        canonical_id: "chatgpt-web",
        display_name: "ChatGPT Web",
        aliases: &["chatgpt"],
        database_ids: &[],
        runtime_id: Some("chatgpt-web"),
        exposure_status: ExposureStatus::PendingBackend,
        access: AccessKind::BrowserSession,
        auth: BROWSER_SESSION,
        env_vars: &[],
        freemium_note: "Free ChatGPT web session; backend execution is pending.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "codex-web",
        display_name: "OpenAI Codex Web",
        aliases: &["openai-codex"],
        database_ids: &[],
        runtime_id: Some("codex-web"),
        exposure_status: ExposureStatus::PendingBackend,
        access: AccessKind::PremiumAccount,
        auth: BROWSER_SESSION,
        env_vars: &[],
        freemium_note: "ChatGPT account based Codex access; backend execution is pending.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "gemini-web",
        display_name: "Gemini Web",
        aliases: &[],
        database_ids: &[],
        runtime_id: Some("gemini-web"),
        exposure_status: ExposureStatus::PendingBackend,
        access: AccessKind::BrowserSession,
        auth: BROWSER_SESSION,
        env_vars: &[],
        freemium_note: "Free Gemini web session; backend execution is pending.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "antigravity-web",
        display_name: "Google Antigravity",
        aliases: &["antigravity"],
        database_ids: &[],
        runtime_id: Some("antigravity-web"),
        exposure_status: ExposureStatus::PendingBackend,
        access: AccessKind::FreeTier,
        auth: BROWSER_SESSION,
        env_vars: &[],
        freemium_note: "Free preview access; backend execution is pending.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "claude-web",
        display_name: "Claude Web",
        aliases: &[],
        database_ids: &[],
        runtime_id: Some("claude-web"),
        exposure_status: ExposureStatus::PendingBackend,
        access: AccessKind::BrowserSession,
        auth: BROWSER_SESSION,
        env_vars: &[],
        freemium_note: "Claude web session access; backend execution is pending.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "google-gemini",
        display_name: "Google AI Studio",
        aliases: &["gemini", "google", "google-ai-studio"],
        database_ids: &["gemini"],
        runtime_id: Some("google-gemini"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["GOOGLE_API_KEY"],
        freemium_note: "Gemini free quotas vary by model and region.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "gemini-oauth",
        display_name: "Gemini OAuth",
        aliases: &["gemini-cli-oauth"],
        database_ids: &[],
        runtime_id: Some("gemini-oauth"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: OAUTH,
        env_vars: &[],
        freemium_note: "Google OAuth flow with free Code Assist style quotas.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "qwen",
        display_name: "Qwen OAuth",
        aliases: &["dashscope-oauth", "qwen-code"],
        database_ids: &[],
        runtime_id: Some("qwen"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: OAUTH,
        env_vars: &[],
        freemium_note: "Qwen OAuth and Qwen Code account quotas vary by region and release policy.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "groq",
        display_name: "Groq",
        aliases: &[],
        database_ids: &["groq"],
        runtime_id: Some("groq"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["GROQ_API_KEY"],
        freemium_note: "Free developer quota, commonly around 30 RPM with daily request limits.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "cerebras",
        display_name: "Cerebras",
        aliases: &[],
        database_ids: &["cerebras"],
        runtime_id: Some("cerebras"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["CEREBRAS_API_KEY"],
        freemium_note: "Free developer tier, commonly around 30 RPM and 1M tokens per day.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "mistral",
        display_name: "Mistral La Plateforme",
        aliases: &["codestral"],
        database_ids: &["mistral", "codestral", "text-completion-codestral"],
        runtime_id: Some("mistral"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["MISTRAL_API_KEY"],
        freemium_note: "Experiment and evaluation access varies by account; Codestral uses the same key.",
        free_model_ids: &["codestral-latest"],
    },
    ProviderMetadata {
        canonical_id: "openrouter",
        display_name: "OpenRouter",
        aliases: &[],
        database_ids: &["openrouter"],
        runtime_id: Some("openrouter"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["OPENROUTER_API_KEY"],
        freemium_note: "Free model routes and daily quotas vary by account and spend history.",
        free_model_ids: &[
            "mistralai/mistral-small-3.1-24b-instruct:free",
            "deepseek/deepseek-r1:free",
            "meta-llama/llama-3.2-3b-instruct:free",
        ],
    },
    ProviderMetadata {
        canonical_id: "opencode-zen",
        display_name: "OpenCode Zen",
        aliases: &["zen", "opencode-free", "opencode-zen-free"],
        database_ids: &["opencode", "opencode-go"],
        runtime_id: Some("opencode-zen"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: PUBLIC_OR_API_KEY,
        env_vars: &["OPENCODE_API_KEY", "OPENCODE_ZEN_API_KEY"],
        freemium_note: "Public free-model access works without a private key; paid Zen models require an OpenCode API key.",
        free_model_ids: &[
            "big-pickle",
            "deepseek-v4-flash-free",
            "mimo-v2.5-free",
            "minimax-m3-free",
            "nemotron-3-super-free",
            "nemotron-3-ultra-free",
        ],
    },
    ProviderMetadata {
        canonical_id: "cohere",
        display_name: "Cohere",
        aliases: &[],
        database_ids: &["cohere"],
        runtime_id: Some("cohere"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["COHERE_API_KEY"],
        freemium_note: "Free developer request quota varies by model and account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "nvidia",
        display_name: "NVIDIA NIM",
        aliases: &["nvidia-nim", "nvidia_nim"],
        database_ids: &["nvidia_nim"],
        runtime_id: Some("nvidia"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["NVIDIA_API_KEY"],
        freemium_note: "NVIDIA NIM has no-credit-card developer quotas for many hosted models.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "sambanova",
        display_name: "SambaNova",
        aliases: &[],
        database_ids: &["sambanova"],
        runtime_id: Some("sambanova"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["SAMBANOVA_API_KEY"],
        freemium_note: "Persistent small developer quota suitable for light usage.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "huggingface",
        display_name: "Hugging Face",
        aliases: &["hugging-face"],
        database_ids: &["huggingface"],
        runtime_id: Some("huggingface"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["HUGGINGFACE_API_KEY", "HF_TOKEN"],
        freemium_note: "Free inference access depends on model, provider route, and account limits.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "github-models",
        display_name: "GitHub Models",
        aliases: &["github", "github_models"],
        database_ids: &["github-models"],
        runtime_id: Some("github-models"),
        exposure_status: ExposureStatus::VerifiedWorking,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["GITHUB_TOKEN"],
        freemium_note: "Quota depends on GitHub and Copilot plan eligibility.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "fireworks",
        display_name: "Fireworks AI",
        aliases: &["fireworks-ai", "fireworks_ai"],
        database_ids: &["fireworks_ai"],
        runtime_id: Some("fireworks"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["FIREWORKS_API_KEY"],
        freemium_note: "Free trial availability varies by account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "deepseek",
        display_name: "DeepSeek",
        aliases: &[],
        database_ids: &["deepseek"],
        runtime_id: Some("deepseek"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeCredits,
        auth: API_KEY,
        env_vars: &["DEEPSEEK_API_KEY"],
        freemium_note: "Free credit promotions vary by account and region.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "together",
        display_name: "Together AI",
        aliases: &["together-ai", "together_ai", "togetherai"],
        database_ids: &["together_ai", "togetherai"],
        runtime_id: Some("together"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeCredits,
        auth: API_KEY,
        env_vars: &["TOGETHER_API_KEY"],
        freemium_note: "Free credit availability varies by account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "cloudflare",
        display_name: "Cloudflare Workers AI",
        aliases: &["cloudflare-workers-ai", "workers-ai"],
        database_ids: &["cloudflare", "cloudflare-workers-ai"],
        runtime_id: Some("cloudflare"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: CLOUD_ACCOUNT_API_KEY,
        env_vars: &["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID"],
        freemium_note: "Workers AI free quota is measured in neurons with account-level limits.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "anthropic",
        display_name: "Anthropic",
        aliases: &["claude"],
        database_ids: &["anthropic"],
        runtime_id: Some("anthropic"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeCredits,
        auth: API_KEY,
        env_vars: &["ANTHROPIC_API_KEY"],
        freemium_note: "Credit availability and verification requirements vary by account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "perplexity",
        display_name: "Perplexity",
        aliases: &[],
        database_ids: &["perplexity"],
        runtime_id: Some("perplexity"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["PERPLEXITY_API_KEY"],
        freemium_note: "Free or trial availability varies by account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "replicate",
        display_name: "Replicate",
        aliases: &[],
        database_ids: &["replicate"],
        runtime_id: Some("replicate"),
        exposure_status: ExposureStatus::Implemented,
        access: AccessKind::FreeCredits,
        auth: API_KEY,
        env_vars: &["REPLICATE_API_TOKEN", "REPLICATE_API_KEY"],
        freemium_note: "Trial credit availability varies by account.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "ovhcloud",
        display_name: "OVHcloud AI Endpoints",
        aliases: &["ovh-ai-endpoints"],
        database_ids: &["ovhcloud"],
        runtime_id: None,
        exposure_status: ExposureStatus::CatalogOnly,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["OVH_AI_ENDPOINTS_ACCESS_TOKEN"],
        freemium_note: "Catalog-only in this CLI slice; OVH advertises limited free IP/key quotas.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "zai",
        display_name: "ZAI",
        aliases: &["z-ai", "zhipu", "glm-provider"],
        database_ids: &["zai", "zai-coding-plan", "vertex_ai-zai_models"],
        runtime_id: None,
        exposure_status: ExposureStatus::CatalogOnly,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["ZAI_API_KEY"],
        freemium_note: "Catalog-only in this CLI slice; free Flash model access requires provider verification.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "scaleway",
        display_name: "Scaleway",
        aliases: &[],
        database_ids: &["scaleway"],
        runtime_id: None,
        exposure_status: ExposureStatus::CatalogOnly,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["SCALEWAY_API_KEY"],
        freemium_note: "Catalog-only in this CLI slice; free-token program requires provider verification.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "dashscope",
        display_name: "Alibaba DashScope",
        aliases: &["alibaba-dashscope", "alibaba"],
        database_ids: &[
            "dashscope",
            "alibaba-cn",
            "alibaba-coding-plan",
            "alibaba-coding-plan-cn",
        ],
        runtime_id: Some("qwen"),
        exposure_status: ExposureStatus::CatalogOnly,
        access: AccessKind::FreeTier,
        auth: API_KEY,
        env_vars: &["DASHSCOPE_API_KEY"],
        freemium_note: "DashScope free-token grants vary by model, region, and account age.",
        free_model_ids: &[],
    },
    ProviderMetadata {
        canonical_id: "gemini-cli",
        display_name: "Gemini CLI",
        aliases: &["google-gemini-cli"],
        database_ids: &[],
        runtime_id: None,
        exposure_status: ExposureStatus::CatalogOnly,
        access: AccessKind::FreeTier,
        auth: LOCAL_RUNTIME,
        env_vars: &[],
        freemium_note: "Local Gemini CLI integration can use personal Google account quotas.",
        free_model_ids: &[],
    },
];

#[allow(dead_code)]
pub(crate) fn all_provider_metadata() -> &'static [ProviderMetadata] {
    PROVIDER_METADATA
}

pub fn metadata_for_provider_id(id: &str) -> Option<&'static ProviderMetadata> {
    let normalized = normalize_provider_id(id);
    PROVIDER_METADATA.iter().find(|metadata| {
        normalize_provider_id(metadata.canonical_id) == normalized
            || metadata
                .runtime_id
                .map(|runtime_id| normalize_provider_id(runtime_id) == normalized)
                .unwrap_or(false)
            || metadata
                .aliases
                .iter()
                .any(|alias| normalize_provider_id(alias) == normalized)
            || metadata
                .database_ids
                .iter()
                .any(|database_id| normalize_provider_id(database_id) == normalized)
    })
}

pub(crate) fn normalize_provider_id(id: &str) -> String {
    id.trim().to_ascii_lowercase().replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_zen_public_models_are_declared() {
        let metadata = metadata_for_provider_id("opencode-zen").expect("OpenCode Zen metadata");

        assert_eq!(metadata.canonical_id, "opencode-zen");
        assert_eq!(metadata.access, AccessKind::FreeTier);
        assert!(metadata.auth.contains(&AuthKind::PublicKey));
        assert!(metadata.free_model_ids.contains(&"deepseek-v4-flash-free"));
        assert!(metadata.free_model_ids.contains(&"big-pickle"));
    }

    #[test]
    fn requested_freemium_provider_aliases_resolve() {
        let expected = [
            ("nvidia_nim", "nvidia"),
            ("groq", "groq"),
            ("cerebras", "cerebras"),
            ("google-ai-studio", "google-gemini"),
            ("github-models", "github-models"),
            ("mistral", "mistral"),
            ("codestral", "mistral"),
            ("cloudflare-workers-ai", "cloudflare"),
            ("openrouter", "openrouter"),
            ("sambanova", "sambanova"),
            ("ovhcloud", "ovhcloud"),
            ("zai", "zai"),
            ("scaleway", "scaleway"),
            ("alibaba-dashscope", "dashscope"),
            ("gemini-cli", "gemini-cli"),
            ("opencode-free", "opencode-zen"),
        ];

        for (alias, canonical_id) in expected {
            let metadata = metadata_for_provider_id(alias).expect(alias);
            assert_eq!(metadata.canonical_id, canonical_id, "{alias}");
        }
    }

    #[test]
    fn provider_metadata_identifiers_are_unique() {
        let mut seen = std::collections::BTreeMap::new();

        for metadata in all_provider_metadata() {
            for id in std::iter::once(metadata.canonical_id)
                .chain(metadata.aliases.iter().copied())
                .chain(metadata.database_ids.iter().copied())
            {
                let normalized = normalize_provider_id(id);
                if let Some(existing) = seen.insert(normalized.clone(), metadata.canonical_id) {
                    assert_eq!(
                        existing, metadata.canonical_id,
                        "identifier {normalized} maps to both {existing} and {}",
                        metadata.canonical_id
                    );
                }
            }
        }
    }
}
