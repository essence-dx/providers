use anyhow::Result;
use chrono::{DateTime, NaiveDateTime};
use clap::Subcommand;
use std::{collections::BTreeSet, path::PathBuf};

use crate::provider_metadata::metadata_for_provider_id;
use crate::providers::{opencode_zen, rkyv_loader};

const ALLOWED_PROVIDER_SOURCES: &[&str] = &["litellm", "models.dev", "both", "litellm+models.dev"];

#[derive(Subcommand)]
pub enum CatalogCommand {
    /// Validate the checked-in rkyv provider catalog
    Validate {
        /// Catalog path; defaults to data/providers.rkyv
        #[arg(long, value_name = "PATH", default_value = "data/providers.rkyv")]
        path: PathBuf,
    },
    /// Backfill metadata-declared free models into catalog provider rows
    RefreshFreemiumModels {
        /// Catalog path; defaults to data/providers.rkyv
        #[arg(long, value_name = "PATH", default_value = "data/providers.rkyv")]
        path: PathBuf,
    },
    /// Remove duplicate provider-scoped model rows and refresh declared counts
    Normalize {
        /// Catalog path; defaults to data/providers.rkyv
        #[arg(long, value_name = "PATH", default_value = "data/providers.rkyv")]
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CatalogRefreshSummary {
    providers_updated: usize,
    models_added: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CatalogNormalizeSummary {
    model_rows_removed: usize,
}

pub fn run_catalog_command(command: &CatalogCommand) -> Result<()> {
    match command {
        CatalogCommand::Validate { path } => {
            let catalog = rkyv_loader::read_provider_catalog(path)?;
            validate_provider_catalog(&catalog)?;

            println!();
            println!("  Provider Catalog Archive");
            println!("  Path: {}", path.display());
            println!("  Providers: {}", catalog.providers.len());
            println!("  Models: {}", catalog.total_models);
            println!("  Status: valid");
            println!();
        }
        CatalogCommand::RefreshFreemiumModels { path } => {
            let mut catalog = rkyv_loader::read_provider_catalog(path)?;
            let summary = refresh_freemium_catalog_models(&mut catalog)?;
            validate_provider_catalog(&catalog)?;
            rkyv_loader::write_provider_catalog(path, &catalog)?;

            println!();
            println!("  Provider Catalog Archive");
            println!("  Path: {}", path.display());
            println!("  Providers updated: {}", summary.providers_updated);
            println!("  Models added: {}", summary.models_added);
            println!("  Total providers: {}", catalog.total_providers);
            println!("  Total models: {}", catalog.total_models);
            println!();
        }
        CatalogCommand::Normalize { path } => {
            let mut catalog = rkyv_loader::read_provider_catalog(path)?;
            let summary = normalize_provider_catalog_models(&mut catalog);
            validate_provider_catalog(&catalog)?;
            rkyv_loader::write_provider_catalog(path, &catalog)?;

            println!();
            println!("  Provider Catalog Archive");
            println!("  Path: {}", path.display());
            println!(
                "  Duplicate model rows removed: {}",
                summary.model_rows_removed
            );
            println!("  Total providers: {}", catalog.total_providers);
            println!("  Total models: {}", catalog.total_models);
            println!();
        }
    }

    Ok(())
}

fn validate_provider_catalog(catalog: &rkyv_loader::ProvidersData) -> Result<()> {
    validate_catalog_archive_metadata(catalog)?;

    if catalog.total_providers != catalog.providers.len() {
        anyhow::bail!(
            "catalog declares {} providers but contains {}",
            catalog.total_providers,
            catalog.providers.len()
        );
    }

    let actual_models = catalog
        .providers
        .iter()
        .map(|provider| provider.models.len())
        .sum::<usize>();
    if catalog.total_models != actual_models {
        anyhow::bail!(
            "catalog declares {} models but contains {}",
            catalog.total_models,
            actual_models
        );
    }

    let mut provider_ids = BTreeSet::new();
    for provider in &catalog.providers {
        let provider_id = provider.id.trim();
        if provider_id.is_empty() {
            anyhow::bail!("catalog contains a provider with a blank id");
        }
        let provider_key = normalize_catalog_identifier(provider_id);
        if !provider_ids.insert(provider_key.clone()) {
            anyhow::bail!("catalog contains duplicate provider id `{provider_key}`");
        }
        validate_provider_source(provider_id, &provider.source)?;
        if provider.model_count != provider.models.len() {
            anyhow::bail!(
                "catalog provider `{}` declares {} models but contains {}",
                provider.id,
                provider.model_count,
                provider.models.len()
            );
        }
        let mut model_route_ids = BTreeSet::new();
        for model in &provider.models {
            let model_id = model.id.trim();
            if model_id.is_empty() {
                anyhow::bail!(
                    "catalog provider `{}` contains a blank model id",
                    provider.id
                );
            }
            let route_id = provider_scoped_model_id(provider_id, model_id);
            if !model_route_ids.insert(route_id.clone()) {
                anyhow::bail!(
                    "catalog contains duplicate model id `{}` for provider `{}`",
                    route_id,
                    provider_id
                );
            }
        }
    }

    validate_opencode_zen_catalog_coverage(catalog)
}

fn validate_catalog_archive_metadata(catalog: &rkyv_loader::ProvidersData) -> Result<()> {
    let version = catalog.version.trim();
    if version.is_empty() {
        anyhow::bail!("catalog contains a blank version");
    }
    if !is_semver_like_version(version) {
        anyhow::bail!("catalog contains invalid version `{version}`");
    }

    let generated_at = catalog.generated_at.trim();
    if generated_at.is_empty() || !is_parseable_catalog_timestamp(generated_at) {
        anyhow::bail!("catalog contains invalid generated_at `{generated_at}`");
    }

    Ok(())
}

fn validate_provider_source(provider_id: &str, source: &str) -> Result<()> {
    let source = source.trim();
    if ALLOWED_PROVIDER_SOURCES.contains(&source) {
        Ok(())
    } else {
        anyhow::bail!("catalog provider `{provider_id}` has unknown source `{source}`");
    }
}

fn is_semver_like_version(version: &str) -> bool {
    let version = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_parseable_catalog_timestamp(timestamp: &str) -> bool {
    DateTime::parse_from_rfc3339(timestamp).is_ok()
        || NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
}

fn validate_opencode_zen_catalog_coverage(catalog: &rkyv_loader::ProvidersData) -> Result<()> {
    let metadata = metadata_for_provider_id("opencode-zen")
        .ok_or_else(|| anyhow::anyhow!("OpenCode Zen metadata is missing"))?;

    for database_id in metadata.database_ids {
        let provider = catalog
            .providers
            .iter()
            .find(|provider| provider.id == *database_id)
            .ok_or_else(|| anyhow::anyhow!("catalog provider `{database_id}` is missing"))?;

        for model_id in metadata.free_model_ids {
            if !provider.models.iter().any(|model| model.id == *model_id) {
                anyhow::bail!(
                    "catalog provider `{}` is missing OpenCode Zen free model `{}`",
                    database_id,
                    model_id
                );
            }
        }
    }

    Ok(())
}

fn refresh_freemium_catalog_models(
    catalog: &mut rkyv_loader::ProvidersData,
) -> Result<CatalogRefreshSummary> {
    let metadata = metadata_for_provider_id("opencode-zen")
        .ok_or_else(|| anyhow::anyhow!("OpenCode Zen metadata is missing"))?;
    let model_names = opencode_zen::public_free_models();
    let mut providers_updated = 0usize;
    let mut models_added = 0usize;

    for database_id in metadata.database_ids {
        let provider = catalog
            .providers
            .iter_mut()
            .find(|provider| provider.id == *database_id)
            .ok_or_else(|| anyhow::anyhow!("catalog provider `{database_id}` is missing"))?;
        let mut provider_changed = false;

        for model_id in metadata.free_model_ids {
            if provider.models.iter().any(|model| model.id == *model_id) {
                continue;
            }

            provider.models.push(rkyv_loader::Model {
                id: model_id.to_string(),
                name: model_names
                    .iter()
                    .find(|(id, _)| id == model_id)
                    .map(|(_, name)| *name)
                    .unwrap_or(model_id)
                    .to_string(),
                mode: "chat".to_string(),
                max_tokens: 0,
                input_cost: 0.0,
                output_cost: 0.0,
            });
            provider_changed = true;
            models_added += 1;
        }

        if provider_changed {
            provider.model_count = provider.models.len();
            providers_updated += 1;
        }
    }

    catalog.total_models = catalog
        .providers
        .iter()
        .map(|provider| provider.models.len())
        .sum();

    Ok(CatalogRefreshSummary {
        providers_updated,
        models_added,
    })
}

fn normalize_provider_catalog_models(
    catalog: &mut rkyv_loader::ProvidersData,
) -> CatalogNormalizeSummary {
    let mut model_rows_removed = 0usize;

    for provider in &mut catalog.providers {
        let provider_id = provider.id.trim().to_string();
        let mut model_route_ids = BTreeSet::new();
        let before = provider.models.len();
        provider.models.retain(|model| {
            let model_id = model.id.trim();
            if model_id.is_empty() {
                return true;
            }
            model_route_ids.insert(provider_scoped_model_id(&provider_id, model_id))
        });
        model_rows_removed += before.saturating_sub(provider.models.len());
        provider.model_count = provider.models.len();
    }

    catalog.total_providers = catalog.providers.len();
    catalog.total_models = catalog
        .providers
        .iter()
        .map(|provider| provider.models.len())
        .sum();

    CatalogNormalizeSummary { model_rows_removed }
}

fn provider_scoped_model_id(provider_id: &str, model_id: &str) -> String {
    if model_id
        .split_once('/')
        .is_some_and(|(prefix, _)| prefix == provider_id)
    {
        model_id.to_string()
    } else {
        format!("{provider_id}/{model_id}")
    }
}

fn normalize_catalog_identifier(id: &str) -> String {
    let mut normalized = String::new();
    let mut previous_dash = false;

    for ch in id.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freemium_catalog_refresh_adds_opencode_zen_models() {
        let mut catalog = rkyv_loader::ProvidersData {
            version: "v1.0.0".to_string(),
            generated_at: "2026-04-02T05:04:13.937112".to_string(),
            total_providers: 2,
            total_models: 1,
            providers: vec![
                catalog_provider("opencode", &["big-pickle"]),
                catalog_provider("opencode-go", &[]),
            ],
        };

        let summary = refresh_freemium_catalog_models(&mut catalog).expect("refresh catalog");
        validate_provider_catalog(&catalog).expect("validate refreshed catalog");
        let metadata = metadata_for_provider_id("opencode-zen").expect("OpenCode Zen metadata");

        assert_eq!(summary.providers_updated, 2);
        assert_eq!(summary.models_added, 11);
        assert_eq!(catalog.total_models, 12);

        for database_id in metadata.database_ids {
            let provider = catalog
                .providers
                .iter()
                .find(|provider| provider.id == *database_id)
                .expect("catalog provider");
            for model_id in metadata.free_model_ids {
                assert!(provider.models.iter().any(|model| model.id == *model_id));
            }
        }
    }

    #[test]
    fn provider_catalog_rejects_duplicate_provider_ids() {
        let catalog = catalog_with_valid_opencode_coverage(vec![
            catalog_provider("duplicate-provider", &["model-a"]),
            catalog_provider("duplicate-provider", &["model-b"]),
        ]);

        let error = validate_provider_catalog(&catalog)
            .expect_err("duplicate provider IDs should fail catalog validation");

        assert!(
            error
                .to_string()
                .contains("duplicate provider id `duplicate-provider`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_duplicate_model_ids() {
        let catalog = catalog_with_valid_opencode_coverage(vec![catalog_provider(
            "duplicate-models",
            &["same-model", "same-model"],
        )]);

        let error = validate_provider_catalog(&catalog)
            .expect_err("duplicate model IDs should fail catalog validation");

        assert!(
            error.to_string().contains(
                "duplicate model id `duplicate-models/same-model` for provider `duplicate-models`"
            ),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_duplicate_provider_scoped_model_ids() {
        let catalog = catalog_with_valid_opencode_coverage(vec![catalog_provider(
            "openai",
            &["sora-2", "openai/sora-2"],
        )]);

        let error = validate_provider_catalog(&catalog)
            .expect_err("duplicate provider-scoped model IDs should fail catalog validation");

        assert!(
            error
                .to_string()
                .contains("duplicate model id `openai/sora-2` for provider `openai`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_blank_version() {
        let mut catalog = catalog_with_valid_opencode_coverage(Vec::new());
        catalog.version = "  ".to_string();

        let error = validate_provider_catalog(&catalog)
            .expect_err("blank catalog version should fail validation");

        assert!(
            error.to_string().contains("blank version"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_invalid_version() {
        let mut catalog = catalog_with_valid_opencode_coverage(Vec::new());
        catalog.version = "release-candidate".to_string();

        let error = validate_provider_catalog(&catalog)
            .expect_err("non-semver catalog version should fail validation");

        assert!(
            error.to_string().contains("invalid version"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_invalid_generated_at() {
        let mut catalog = catalog_with_valid_opencode_coverage(Vec::new());
        catalog.generated_at = "not-a-timestamp".to_string();

        let error = validate_provider_catalog(&catalog)
            .expect_err("invalid generated_at should fail validation");

        assert!(
            error.to_string().contains("invalid generated_at"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_unknown_provider_source() {
        let mut provider = catalog_provider("unknown-source", &["model-a"]);
        provider.source = "spreadsheet".to_string();
        let catalog = catalog_with_valid_opencode_coverage(vec![provider]);

        let error = validate_provider_catalog(&catalog)
            .expect_err("unknown provider source should fail validation");

        assert!(
            error.to_string().contains("unknown source"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_rejects_normalized_duplicate_provider_ids() {
        let mut first = catalog_provider("open_ai", &["model-a"]);
        first.source = "litellm".to_string();
        let mut second = catalog_provider("open-ai", &["model-b"]);
        second.source = "models.dev".to_string();
        let catalog = catalog_with_valid_opencode_coverage(vec![first, second]);

        let error = validate_provider_catalog(&catalog)
            .expect_err("normalized duplicate provider IDs should fail validation");

        assert!(
            error
                .to_string()
                .contains("duplicate provider id `open-ai`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn provider_catalog_normalize_removes_duplicate_provider_scoped_model_rows() {
        let mut catalog = catalog_with_valid_opencode_coverage(vec![catalog_provider(
            "openai",
            &["sora-2", "openai/sora-2"],
        )]);

        let summary = normalize_provider_catalog_models(&mut catalog);

        assert_eq!(summary.model_rows_removed, 1);
        validate_provider_catalog(&catalog).expect("normalized catalog should validate");
        let openai = catalog
            .providers
            .iter()
            .find(|provider| provider.id == "openai")
            .expect("OpenAI fixture provider");
        assert_eq!(openai.model_count, 1);
        assert_eq!(openai.models[0].id, "sora-2");
    }

    fn catalog_provider(id: &str, model_ids: &[&str]) -> rkyv_loader::Provider {
        rkyv_loader::Provider {
            id: id.to_string(),
            name: id.to_string(),
            source: "litellm".to_string(),
            model_count: model_ids.len(),
            supports_chat: true,
            supports_embedding: false,
            supports_image: false,
            supports_audio: false,
            api_url: String::new(),
            docs_url: String::new(),
            models: model_ids
                .iter()
                .map(|model_id| rkyv_loader::Model {
                    id: (*model_id).to_string(),
                    name: (*model_id).to_string(),
                    mode: "chat".to_string(),
                    max_tokens: 0,
                    input_cost: 0.0,
                    output_cost: 0.0,
                })
                .collect(),
        }
    }

    fn catalog_with_valid_opencode_coverage(
        mut providers: Vec<rkyv_loader::Provider>,
    ) -> rkyv_loader::ProvidersData {
        let metadata = metadata_for_provider_id("opencode-zen").expect("OpenCode Zen metadata");
        for database_id in metadata.database_ids {
            providers.push(catalog_provider(database_id, metadata.free_model_ids));
        }

        let total_models = providers
            .iter()
            .map(|provider| provider.models.len())
            .sum::<usize>();

        rkyv_loader::ProvidersData {
            version: "v1.0.0".to_string(),
            generated_at: "2026-04-02T05:04:13.937112".to_string(),
            total_providers: providers.len(),
            total_models,
            providers,
        }
    }
}
