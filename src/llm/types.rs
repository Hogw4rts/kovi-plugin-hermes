use serde::{Deserialize, Serialize};

pub use crate::routing::Role;

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
#[non_exhaustive]
pub enum MessageContent {
    Text(String),
    Multimodal(Vec<ContentPart>),
}

impl MessageContent {
    /// Convenience constructor for text-only content.
    #[allow(dead_code)]
    pub fn text(s: impl Into<String>) -> Self {
        MessageContent::Text(s.into())
    }

    pub fn from_text_and_images(text: &str, image_urls: &[&str]) -> Self {
        if image_urls.is_empty() {
            return MessageContent::Text(text.to_string());
        }
        let mut parts: Vec<ContentPart> = Vec::with_capacity(1 + image_urls.len());
        if !text.is_empty() {
            parts.push(ContentPart::Text {
                text: text.to_string(),
            });
        }
        parts.extend(image_urls.iter().map(|url| ContentPart::ImageUrl {
            image_url: ImageUrl {
                url: url.to_string(),
                detail: None,
            },
        }));
        MessageContent::Multimodal(parts)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrl,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Low,
    High,
    #[default]
    Auto,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Option<Vec<Choice>>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Option<ChoiceMessage>,
}

#[derive(Debug, Deserialize)]
pub struct ChoiceMessage {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChunkResponse {
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    pub delta: Option<ChunkDelta>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkDelta {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ModelsResponse {
    pub data: Option<Vec<ModelItem>>,
}

#[derive(Debug, Deserialize)]
pub struct ModelItem {
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub error: Option<ApiErrorDetail>,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorDetail {
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_text_only() {
        let content = MessageContent::from_text_and_images("hello", &[]);
        assert!(matches!(content, MessageContent::Text(s) if s == "hello"));
    }

    #[test]
    fn test_message_content_multimodal() {
        let content = MessageContent::from_text_and_images("look", &["https://x.com/img.png"]);
        let MessageContent::Multimodal(parts) = content else {
            panic!("expected Multimodal");
        };
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], ContentPart::Text { text } if text == "look"));
        assert!(matches!(&parts[1], ContentPart::ImageUrl { image_url } if image_url.url == "https://x.com/img.png"));
    }

    #[test]
    fn test_message_content_image_only_skips_empty_text() {
        let content = MessageContent::from_text_and_images("", &["https://x.com/img.png"]);
        let MessageContent::Multimodal(parts) = content else {
            panic!("expected Multimodal");
        };
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ContentPart::ImageUrl { .. }));
    }

    #[test]
    fn test_message_content_text_convenience() {
        let content = MessageContent::text("hello");
        assert!(matches!(content, MessageContent::Text(s) if s == "hello"));
    }

    #[test]
    fn test_image_url_serialization() {
        let iu = ImageUrl {
            url: "https://x.com/a.png".to_string(),
            detail: None,
        };
        let json = serde_json::to_string(&iu).unwrap();
        assert!(json.contains("\"url\":\"https://x.com/a.png\""));
        assert!(!json.contains("detail"));
    }

    #[test]
    fn test_image_url_with_detail_serialization() {
        let iu = ImageUrl {
            url: "https://x.com/a.png".to_string(),
            detail: Some(ImageDetail::High),
        };
        let json = serde_json::to_string(&iu).unwrap();
        assert!(json.contains("\"detail\":\"high\""));
    }

    #[test]
    fn test_chat_request_serialization_text_only() {
        let req = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::Text("hi".to_string()),
            }],
            stream: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"content\":\"hi\""));
        assert!(!json.contains("stream"));
    }

    #[test]
    fn test_chat_request_serialization_multimodal() {
        let req = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::from_text_and_images(
                    "describe",
                    &["https://x.com/a.png"],
                ),
            }],
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"type\":\"image_url\""));
        assert!(json.contains("\"stream\":true"));
    }
}
