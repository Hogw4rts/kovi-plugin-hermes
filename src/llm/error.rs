use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("api error: {status} - {message}")]
    Api { status: u16, message: String },
    #[error("empty response from model")]
    EmptyResponse,
}