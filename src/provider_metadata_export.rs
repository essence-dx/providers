use crate::provider_metadata::{
    FreemiumMetadata, ProviderIdentity, all_provider_metadata, normalize_provider_id,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROVIDER_METADATA_SCHEMA: &str = "dx.providers.metadata.v1";
pub const PROVIDER_METADATA_SCHEMA_VERSION: u16 = 1;
pub const PROVIDER_METADATA_SIDECAR_PATH: &str = "data/provider-metadata.generated.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderMetadataExport {
    pub schema: String,
    pub schema_version: u16,
    pub source: ProviderMetadataExportSource,
    pub summary: ProviderMetadataExportSummary,
    pub redaction: ProviderMetadataExportRedaction,
    pub providers: Vec<ProviderMetadataExportRow>,
    pub alias_index: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderMetadataExportSource {
    pub repo: String,
    pub commit: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderMetadataExportSummary {
    pub provider_count: usize,
    pub alias_count: usize,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderMetadataExportRedaction {
    pub secrets_included: bool,
    pub statement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderMetadataExportRow {
    pub identity: ProviderIdentity,
    pub freemium: FreemiumMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadataSidecarWrite {
    pub path: PathBuf,
    pub provider_count: usize,
    pub alias_count: usize,
    pub content_sha256: String,
}

pub fn build_provider_metadata_export(
    source: ProviderMetadataExportSource,
) -> ProviderMetadataExport {
    let mut providers = Vec::new();
    let mut alias_index = BTreeMap::new();

    for metadata in all_provider_metadata() {
        let canonical_id = metadata.canonical_id.to_string();
        index_provider_identifier(&mut alias_index, metadata.canonical_id, &canonical_id);
        if let Some(runtime_id) = metadata.runtime_id {
            index_provider_identifier(&mut alias_index, runtime_id, &canonical_id);
        }
        for alias in metadata.aliases {
            index_provider_identifier(&mut alias_index, alias, &canonical_id);
        }
        for database_id in metadata.database_ids {
            index_provider_identifier(&mut alias_index, database_id, &canonical_id);
        }

        providers.push(ProviderMetadataExportRow {
            identity: metadata.identity(),
            freemium: metadata.freemium(),
        });
    }

    let summary = ProviderMetadataExportSummary {
        provider_count: providers.len(),
        alias_count: alias_index.len(),
        content_sha256: metadata_payload_sha256(
            PROVIDER_METADATA_SCHEMA,
            PROVIDER_METADATA_SCHEMA_VERSION,
            &providers,
            &alias_index,
        ),
    };
    let redaction = ProviderMetadataExportRedaction {
        secrets_included: false,
        statement: "This export contains provider identifiers, model identifiers, and environment variable names only; it never contains credential values, tokens, cookies, or API keys.".to_string(),
    };

    ProviderMetadataExport {
        schema: PROVIDER_METADATA_SCHEMA.to_string(),
        schema_version: PROVIDER_METADATA_SCHEMA_VERSION,
        source,
        summary,
        redaction,
        providers,
        alias_index,
    }
}

pub fn write_provider_metadata_sidecar(
    path: impl AsRef<Path>,
    source: ProviderMetadataExportSource,
) -> Result<ProviderMetadataSidecarWrite> {
    let path = path.as_ref();
    let export = build_provider_metadata_export(source);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create provider metadata sidecar directory {}",
                parent.display()
            )
        })?;
    }

    let payload = format!("{}\n", serde_json::to_string_pretty(&export)?);
    fs::write(path, payload).with_context(|| {
        format!(
            "failed to write provider metadata sidecar {}",
            path.display()
        )
    })?;

    Ok(ProviderMetadataSidecarWrite {
        path: path.to_path_buf(),
        provider_count: export.summary.provider_count,
        alias_count: export.summary.alias_count,
        content_sha256: export.summary.content_sha256,
    })
}

fn index_provider_identifier(
    alias_index: &mut BTreeMap<String, String>,
    id: &str,
    canonical_id: &str,
) {
    alias_index
        .entry(normalize_provider_id(id))
        .or_insert_with(|| canonical_id.to_string());
}

fn metadata_payload_sha256(
    schema: &str,
    schema_version: u16,
    providers: &[ProviderMetadataExportRow],
    alias_index: &BTreeMap<String, String>,
) -> String {
    #[derive(Serialize)]
    struct DigestPayload<'a> {
        schema: &'a str,
        schema_version: u16,
        providers: &'a [ProviderMetadataExportRow],
        alias_index: &'a BTreeMap<String, String>,
    }

    let bytes = serde_json::to_vec(&DigestPayload {
        schema,
        schema_version,
        providers,
        alias_index,
    })
    .expect("provider metadata digest payload should serialize");
    let digest = sha2::Sha256::digest(bytes);
    let mut hash = String::with_capacity("sha256:".len() + 64);
    hash.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hash, "{byte:02x}").expect("writing to String cannot fail");
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_metadata::metadata_for_provider_id;
    use chrono::{TimeZone, Utc};
    use std::fs;

    fn fixed_source() -> ProviderMetadataExportSource {
        ProviderMetadataExportSource {
            repo: r"G:\Dx\providers".to_string(),
            commit: Some("ec18e7e".to_string()),
            generated_at: Utc
                .with_ymd_and_hms(2026, 6, 5, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
        }
    }

    #[test]
    fn metadata_export_contains_opencode_zen_and_alias_index() {
        let export = build_provider_metadata_export(fixed_source());

        assert_eq!(export.schema, "dx.providers.metadata.v1");
        assert_eq!(export.schema_version, 1);
        assert_eq!(
            export.alias_index.get("opencode-go").map(String::as_str),
            Some("opencode-zen")
        );

        let opencode = export
            .providers
            .iter()
            .find(|provider| provider.identity.canonical_id == "opencode-zen")
            .expect("OpenCode Zen export row");
        assert!(
            opencode
                .freemium
                .env_vars
                .contains(&"OPENCODE_API_KEY".to_string())
        );
        assert!(
            opencode
                .freemium
                .free_model_ids
                .contains(&"deepseek-v4-flash-free".to_string())
        );
    }

    #[test]
    fn metadata_export_records_sidecar_summary_and_redaction_contract() {
        let export = build_provider_metadata_export(fixed_source());

        assert_eq!(
            PROVIDER_METADATA_SIDECAR_PATH,
            "data/provider-metadata.generated.json"
        );
        assert_eq!(export.summary.provider_count, export.providers.len());
        assert_eq!(export.summary.alias_count, export.alias_index.len());
        assert!(export.summary.content_sha256.starts_with("sha256:"));
        assert_eq!(export.summary.content_sha256.len(), "sha256:".len() + 64);
        assert_eq!(
            export.summary.content_sha256,
            metadata_payload_sha256(
                &export.schema,
                export.schema_version,
                &export.providers,
                &export.alias_index
            )
        );
        assert!(!export.redaction.secrets_included);
        assert!(
            export
                .redaction
                .statement
                .contains("environment variable names only")
        );
    }

    #[test]
    fn metadata_export_alias_index_contains_requested_freemium_provider_aliases() {
        let export = build_provider_metadata_export(fixed_source());
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
            assert_eq!(
                export
                    .alias_index
                    .get(&normalize_provider_id(alias))
                    .map(String::as_str),
                Some(canonical_id),
                "{alias} should map to {canonical_id}"
            );
        }
    }

    #[test]
    fn metadata_export_alias_index_matches_provider_metadata_resolution() {
        let export = build_provider_metadata_export(fixed_source());

        assert_eq!(
            export.alias_index.get("qwen").map(String::as_str),
            Some("qwen"),
            "canonical qwen must not be overwritten by DashScope's runtime alias"
        );

        for (alias, canonical_id) in &export.alias_index {
            let metadata = metadata_for_provider_id(alias).unwrap_or_else(|| {
                panic!("alias_index key `{alias}` must resolve through provider metadata")
            });
            assert_eq!(
                metadata.canonical_id, canonical_id,
                "alias_index key `{alias}` must match metadata_for_provider_id"
            );
        }
    }

    #[test]
    fn generated_sidecar_matches_current_export_payload() {
        let bytes = fs::read_to_string(PROVIDER_METADATA_SIDECAR_PATH)
            .expect("read generated provider metadata sidecar");
        let parsed: ProviderMetadataExport =
            serde_json::from_str(&bytes).expect("parse generated metadata sidecar");
        let rebuilt = build_provider_metadata_export(parsed.source.clone());

        assert_eq!(parsed, rebuilt);
    }

    #[test]
    fn write_provider_metadata_sidecar_writes_readable_export_file() {
        let root = std::env::temp_dir().join(format!(
            "providers-metadata-sidecar-{}",
            uuid::Uuid::new_v4()
        ));
        let sidecar_path = root.join(PROVIDER_METADATA_SIDECAR_PATH);

        let written = write_provider_metadata_sidecar(&sidecar_path, fixed_source())
            .expect("write metadata sidecar");
        let bytes = fs::read_to_string(&sidecar_path).expect("read metadata sidecar");
        let parsed: ProviderMetadataExport =
            serde_json::from_str(&bytes).expect("parse metadata sidecar");

        assert_eq!(written.path, sidecar_path);
        assert_eq!(written.provider_count, parsed.providers.len());
        assert_eq!(written.alias_count, parsed.alias_index.len());
        assert_eq!(written.content_sha256, parsed.summary.content_sha256);
        assert_eq!(parsed.schema, PROVIDER_METADATA_SCHEMA);
        assert!(!parsed.redaction.secrets_included);
        assert_eq!(parsed, build_provider_metadata_export(fixed_source()));

        fs::remove_dir_all(root).expect("remove temp sidecar directory");
    }

    #[test]
    fn metadata_export_json_does_not_include_secret_storage_keys() {
        let export = build_provider_metadata_export(fixed_source());
        let value = serde_json::to_value(&export).expect("serialize metadata export");
        let forbidden_keys = [
            "api_keys",
            "access_token",
            "refresh_token",
            "cookie",
            "cookies",
            "keychain",
        ];

        assert_json_keys_do_not_contain(&value, &forbidden_keys);
    }

    fn assert_json_keys_do_not_contain(value: &serde_json::Value, forbidden_keys: &[&str]) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, value) in object {
                    let key = key.to_lowercase();
                    assert!(
                        !forbidden_keys.iter().any(|forbidden| key == *forbidden),
                        "metadata export must not include secret storage key `{key}`"
                    );
                    assert_json_keys_do_not_contain(value, forbidden_keys);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    assert_json_keys_do_not_contain(item, forbidden_keys);
                }
            }
            _ => {}
        }
    }
}
