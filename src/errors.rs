use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum ProviderError {
    #[error("Provider '{provider}' is not configured — run `providers setup {provider}`")]
    NotConfigured { provider: String },

    #[error("HTTP {status} from {provider}: {body}")]
    HttpError {
        provider: String,
        status: u16,
        body: String,
    },

    #[error("OAuth flow failed for {provider}: {reason}")]
    OAuthFailed { provider: String, reason: String },

    #[error("OAuth timed out waiting for authorization")]
    OAuthTimeout,

    #[error("Empty response from {provider}")]
    EmptyResponse { provider: String },

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
