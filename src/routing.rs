use crate::config::HermesConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    Private,
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub i64);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct MessageRoute {
    pub msg_type: MsgType,
    pub user_id: UserId,
    pub group_id: GroupId,
    pub message_id: i32,
    pub sender_name: String,
}

pub fn build_base_session_key(route: &MessageRoute, config: &HermesConfig) -> String {
    match route.msg_type {
        MsgType::Group => {
            if config.group_sessions_per_user {
                format!("qq:group:{}:user:{}", route.group_id, route.user_id)
            } else {
                format!("qq:group:{}", route.group_id)
            }
        }
        MsgType::Private => format!("qq:user:{}", route.user_id),
    }
}

#[derive(Debug, Clone)]
pub struct UserInput {
    pub text: String,
    pub image_urls: Vec<String>,
}

impl UserInput {
    pub fn image_urls_as_str(&self) -> Vec<&str> {
        self.image_urls.iter().map(String::as_str).collect()
    }
}