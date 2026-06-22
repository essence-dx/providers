// Secure token storage using OS keychain
use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SERVICE_NAME: &str = "providers-cli";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub token: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, String>,
}

/// Store a token securely in the OS keychain
pub fn store_token(provider: &str, token: &StoredToken) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider).context("Failed to create keyring entry")?;

    let token_json = serde_json::to_string(token).context("Failed to serialize token")?;

    entry
        .set_password(&token_json)
        .context("Failed to store token in keychain")?;

    Ok(())
}

/// Retrieve a token from the OS keychain
pub fn retrieve_token(provider: &str) -> Result<StoredToken> {
    let entry = Entry::new(SERVICE_NAME, provider).context("Failed to create keyring entry")?;

    let token_json = entry
        .get_password()
        .context("Failed to retrieve token from keychain")?;

    let token: StoredToken =
        serde_json::from_str(&token_json).context("Failed to deserialize token")?;

    Ok(token)
}

/// Delete a token from the OS keychain
#[allow(dead_code)]
pub fn delete_token(provider: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider).context("Failed to create keyring entry")?;

    entry
        .delete_credential()
        .context("Failed to delete token from keychain")?;

    Ok(())
}

/// Check if a token is expired
pub fn is_token_expired(token: &StoredToken) -> bool {
    if let Some(expires_at) = token.expires_at {
        chrono::Utc::now() >= expires_at
    } else {
        false // No expiry means token doesn't expire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_storage() {
        if std::env::var_os("PROVIDERS_RUN_KEYCHAIN_TESTS").is_none() {
            eprintln!("Skipping OS keychain test; set PROVIDERS_RUN_KEYCHAIN_TESTS=1 to run it.");
            return;
        }

        let token = StoredToken {
            token: "test_token_123".to_string(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(24)),
            metadata: HashMap::new(),
        };

        let provider = format!("test_provider_{}", uuid::Uuid::new_v4());

        // Store token
        store_token(&provider, &token).unwrap();

        // Retrieve token
        let retrieved = retrieve_token(&provider).unwrap();
        assert_eq!(retrieved.token, "test_token_123");

        // Delete token
        delete_token(&provider).unwrap();
    }
}
