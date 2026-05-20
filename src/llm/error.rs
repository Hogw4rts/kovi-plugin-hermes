use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum LlmError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("api error: {status} - {message}")]
    Api { status: u16, message: String },
    #[error("empty response from model")]
    EmptyResponse,
    #[error("failed to build http client: {0}")]
    ClientBuild(String),
}

impl LlmError {
    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            LlmError::Request(e) => e.is_timeout() || e.is_connect(),
            LlmError::Api { status, .. } => *status == 429 || *status >= 500,
            LlmError::EmptyResponse => false,
            LlmError::ClientBuild(_) => false,
        }
    }
}