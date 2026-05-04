use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Multimodal(Vec<ContentPart>),
}

impl MessageContent {
    #[allow(dead_code)]
    pub fn text(s: impl Into<String>) -> Self {
        MessageContent::Text(s.into())
    }

    pub fn from_text_and_images(text: &str, image_urls: &[String]) -> Self {
        if image_urls.is_empty() {
            return MessageContent::Text(text.to_string());
        }
        let mut parts: Vec<ContentPart> = Vec::with_capacity(1 + image_urls.len());
        parts.push(ContentPart::Text {
            text: text.to_string(),
        });
        for url in image_urls {
            parts.push(ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: url.clone(),
                },
            });
        }
        MessageContent::Multimodal(parts)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrl,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
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
