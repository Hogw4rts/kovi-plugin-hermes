pub mod error;
pub mod types;

use crate::config::HermesConfig;
use crate::ratelimit::RateLimiter;
use crate::session::{ConversationMessage, SessionStore};
use error::LlmError;
use futures_util::StreamExt;
use std::sync::Arc;
use types::{ApiError, ChatChunkResponse, ChatMessage, ChatRequest, ChatResponse, MessageContent, ModelsResponse, Role};

#[derive(Clone)]
pub struct LlmClient {
    http: reqwest::Client,
    config: Arc<HermesConfig>,
    store: Arc<SessionStore>,
    system_prompt: Arc<String>,
    rate_limiter: RateLimiter,
}

impl LlmClient {
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn new(
        config: Arc<HermesConfig>,
        store: Arc<SessionStore>,
        system_prompt: Arc<String>,
        rate_limiter: RateLimiter,
    ) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.request_timeout_ms))
            .build()
            .map_err(|e| LlmError::ClientBuild(e.to_string()))?;
        Ok(Self {
            http,
            config,
            store,
            system_prompt,
            rate_limiter,
        })
    }

    async fn resolve_model(&self, model_override: Option<&str>) -> String {
        if let Some(m) = model_override {
            return m.to_string();
        }
        let selected = self.store.selected_model().await;
        if selected.is_empty() {
            self.config.model.clone()
        } else {
            selected
        }
    }

    async fn build_messages(
        &self,
        session_id: &str,
        user_message: &str,
        image_urls: &[&str],
    ) -> Vec<ChatMessage> {
        let estimated = 1 + if self.config.local_history_enabled { 8 } else { 0 } + 1;
        let mut messages = Vec::with_capacity(estimated);
        let system_prompt = self.system_prompt.as_str();
        if !system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: Role::System,
                content: MessageContent::Text(system_prompt.to_string()),
            });
        }
        if self.config.local_history_enabled {
            let history = self.store.get_conversation(session_id).await;
            for m in &history {
                messages.push(ChatMessage {
                    role: m.role,
                    content: MessageContent::Text(m.content.clone()),
                });
            }
        }
        let user_content = MessageContent::from_text_and_images(user_message, image_urls);
        if !image_urls.is_empty() {
            match &user_content {
                MessageContent::Multimodal(parts) => {
                    kovi::log::info!(
                        "hermes: sending multimodal message with {} parts ({} image URLs, first URL prefix: {})",
                        parts.len(),
                        image_urls.len(),
                        image_urls.first().map(|u| u.chars().take(50).collect::<String>()).unwrap_or_default()
                    );
                }
                MessageContent::Text(_) => {
                    kovi::log::warn!("hermes: image_urls provided but content is Text-only!");
                }
            }
        }
        messages.push(ChatMessage {
            role: Role::User,
            content: user_content,
        });
        messages
    }

    async fn persist_turn(&self, session_id: &str, user_message: &str, assistant_reply: &str) {
        if self.config.local_history_enabled {
            self.store
                .append_conversation(
                    session_id,
                    &[
                        ConversationMessage {
                            role: Role::User,
                            content: user_message.to_string(),
                        },
                        ConversationMessage {
                            role: Role::Assistant,
                            content: assistant_reply.to_string(),
                        },
                    ],
                    self.config.local_history_max_messages,
                )
                .await;
        }
    }

    pub async fn complete(
        &self,
        session_id: &str,
        user_message: &str,
        model_override: Option<&str>,
        image_urls: &[&str],
    ) -> Result<String, LlmError> {
        let model = self.resolve_model(model_override).await;
        let messages = self.build_messages(session_id, user_message, image_urls).await;

        let body = ChatRequest {
            model,
            messages,
            stream: false,
        };

        let url = format!(
            "{}/chat/completions",
            self.config.api_base_url.trim_end_matches('/')
        );
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.config.retry_delay_ms))
                    .await;
            }

            self.rate_limiter.acquire().await;
            let result = self.send_request(&url, &body, session_id).await;
            match result {
                Ok(reply) => {
                    self.persist_turn(session_id, user_message, &reply).await;
                    return Ok(reply);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::EmptyResponse))
    }

    pub async fn complete_stream(
        &self,
        session_id: &str,
        user_message: &str,
        model_override: Option<&str>,
        image_urls: &[&str],
    ) -> Result<tokio::sync::mpsc::Receiver<StreamEvent>, LlmError> {
        let model = self.resolve_model(model_override).await;
        let messages = self.build_messages(session_id, user_message, image_urls).await;

        let body = ChatRequest {
            model,
            messages,
            stream: true,
        };

        let url = format!(
            "{}/chat/completions",
            self.config.api_base_url.trim_end_matches('/')
        );

        self.rate_limiter.acquire().await;

        let mut req = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key.as_str()))
            .header("Content-Type", "application/json")
            .json(&body);

        if !session_id.is_empty() {
            req = req.header("X-Hermes-Session-Id", session_id);
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();

        if !resp.status().is_success() {
            let payload: ApiError = resp.json().await.unwrap_or_else(|e| {
                kovi::log::warn!("hermes: failed to parse API error body: {e}");
                ApiError { error: None }
            });
            let message = payload
                .error
                .and_then(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(LlmError::Api { status, message });
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        let user_msg = user_message.to_string();
        let session_id_owned = session_id.to_string();
        let store = self.store.clone();
        let config = self.config.clone();

kovi::spawn(async move {
            let mut full_reply = String::new();
            let mut stream = resp.bytes_stream();

            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            let mut consumed: usize = 0;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => buf.extend_from_slice(&bytes),
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                        return;
                    }
                }

                while let Some(pos) = buf[consumed..].iter().position(|&b| b == b'\n') {
                    let line_start = consumed;
                    let line_end = consumed + pos;
                    consumed = line_end + 1;

                    let line = match std::str::from_utf8(&buf[line_start..line_end]) {
                        Ok(s) => s.trim_end_matches('\r'),
                        Err(_) => continue,
                    };

                    match parse_sse_line(line) {
                        Some(SseFrame::Done) => {
                            if config.local_history_enabled && !full_reply.is_empty() {
                                store
                                    .append_conversation(
                                        &session_id_owned,
                                        &[
                                            ConversationMessage {
                                                role: Role::User,
                                                content: user_msg.clone(),
                                            },
                                            ConversationMessage {
                                                role: Role::Assistant,
                                                content: full_reply,
                                            },
                                        ],
                                        config.local_history_max_messages,
                                    )
                                    .await;
                            }
                            let _ = tx.send(StreamEvent::Done).await;
                            return;
                        }
                        Some(SseFrame::Data(data)) => {
                            if let Some(delta) = extract_delta(&data) {
                                full_reply.push_str(&delta);
                                let _ = tx.send(StreamEvent::Delta(delta)).await;
                            }
                        }
                        None => {}
                    }
                }

                if consumed > 8192 {
                    buf.drain(..consumed);
                    consumed = 0;
                }
            }

            if full_reply.is_empty() {
                let _ = tx
                    .send(StreamEvent::Error(
                        "connection closed before completion".to_string(),
                    ))
                    .await;
            } else {
                if config.local_history_enabled {
                    store
                        .append_conversation(
                            &session_id_owned,
                            &[
                                ConversationMessage {
                                    role: Role::User,
                                    content: user_msg,
                                },
                                ConversationMessage {
                                    role: Role::Assistant,
                                    content: full_reply,
                                },
                            ],
                            config.local_history_max_messages,
                        )
                        .await;
                }
                let _ = tx
                    .send(StreamEvent::Error(
                        "stream ended unexpectedly".to_string(),
                    ))
                    .await;
            }
        });

        Ok(rx)
    }

    async fn send_request(
        &self,
        url: &str,
        body: &ChatRequest,
        session_id: &str,
    ) -> Result<String, LlmError> {
        let mut req = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key.as_str()))
            .header("Content-Type", "application/json")
            .json(body);

        if !session_id.is_empty() {
            req = req.header("X-Hermes-Session-Id", session_id);
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();

        if !resp.status().is_success() {
            let payload: ApiError = resp.json().await.unwrap_or_else(|e| {
                kovi::log::warn!("hermes: failed to parse API error body: {e}");
                ApiError { error: None }
            });
            let message = payload
                .error
                .and_then(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(LlmError::Api { status, message });
        }

        let payload: ChatResponse = resp.json().await.map_err(LlmError::Request)?;

        let text = payload
            .choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .filter(|t| !t.trim().is_empty());

        if let Some(t) = text {
            Ok(t)
        } else {
            Err(LlmError::EmptyResponse)
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        self.rate_limiter.acquire().await;

        let url = format!(
            "{}/models",
            self.config.api_base_url.trim_end_matches('/')
        );

        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key.as_str()))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(LlmError::Api {
                status,
                message: format!("HTTP {status}"),
            });
        }

        let payload: ModelsResponse = resp.json().await.map_err(LlmError::Request)?;

        let mut models: Vec<String> = payload
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| m.id.filter(|id| !id.trim().is_empty()))
            .collect();
        models.sort_unstable();
        models.dedup();
        Ok(models)
    }
}

pub enum StreamEvent {
    Delta(String),
    Done,
    Error(String),
}

enum SseFrame {
    Data(String),
    Done,
}

fn parse_sse_line(line: &str) -> Option<SseFrame> {
    let data = line.strip_prefix("data: ")?;
    if data == "[DONE]" {
        Some(SseFrame::Done)
    } else {
        Some(SseFrame::Data(data.to_string()))
    }
}

fn extract_delta(data: &str) -> Option<String> {
    let chunk_resp = serde_json::from_str::<ChatChunkResponse>(data).ok()?;
    let delta = chunk_resp
        .choices
        .first()?
        .delta
        .as_ref()?
        .content
        .as_ref()?;
    if delta.is_empty() {
        None
    } else {
        Some(delta.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_line_data() {
        let frame = parse_sse_line("data: {\"choices\":[]}");
        assert!(matches!(frame, Some(SseFrame::Data(s)) if s == "{\"choices\":[]}"));
    }

    #[test]
    fn test_parse_sse_line_done() {
        let frame = parse_sse_line("data: [DONE]");
        assert!(matches!(frame, Some(SseFrame::Done)));
    }

    #[test]
    fn test_parse_sse_line_ignores_non_data() {
        assert!(parse_sse_line(": comment").is_none());
        assert!(parse_sse_line("event: ping").is_none());
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line(" ").is_none());
    }

    #[test]
    fn test_parse_sse_line_ignores_data_without_space() {
        assert!(parse_sse_line("data:without-space").is_none());
    }

    #[test]
    fn test_extract_delta_with_content() {
        let data = r#"{"choices":[{"delta":{"content":"hello"}}]}"#;
        assert_eq!(extract_delta(data), Some("hello".to_string()));
    }

    #[test]
    fn test_extract_delta_empty_content() {
        let data = r#"{"choices":[{"delta":{"content":""}}]}"#;
        assert_eq!(extract_delta(data), None);
    }

    #[test]
    fn test_extract_delta_no_content() {
        let data = r#"{"choices":[{"delta":{}}]}"#;
        assert_eq!(extract_delta(data), None);
    }

    #[test]
    fn test_extract_delta_invalid_json() {
        assert_eq!(extract_delta("not json"), None);
    }
}