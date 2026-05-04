use crate::secret::SecretString;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)]
pub struct HermesConfig {
    pub api_base_url: String,
    #[serde(default)]
    pub api_key: SecretString,
    pub model: String,
    pub system_prompt: String,
    pub bot_name: String,
    #[serde(default = "default_true")]
    pub require_mention: bool,
    #[serde(default)]
    pub admin_only_chat: bool,
    #[serde(default)]
    pub notify_non_admin_blocked: bool,
    #[serde(default = "default_blocked_message")]
    pub non_admin_blocked_message: String,
    #[serde(default)]
    pub keyword_only_trigger: bool,
    #[serde(default)]
    pub keyword_triggers: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_bare_group_commands: bool,
    #[serde(default = "default_true")]
    pub format_markdown: bool,
    #[serde(default)]
    pub mention_sender_in_group: bool,
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default)]
    pub group_sessions_per_user: bool,
    #[serde(default = "default_true")]
    pub local_history_enabled: bool,
    #[serde(default = "default_local_history_max")]
    pub local_history_max_messages: usize,
    #[serde(default = "default_queue_debounce_ms")]
    pub queue_debounce_ms: u64,
    #[serde(default = "default_api_rate_limit_rpm")]
    pub api_rate_limit_rpm: u64,
    #[serde(default)]
    pub stream_response: bool,
    #[serde(default = "default_true")]
    pub image_recognition: bool,
    #[serde(default = "default_image_mode")]
    pub image_mode: ImageMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageMode {
    Url,
    Base64,
}

fn default_image_mode() -> ImageMode {
    ImageMode::Url
}

fn default_true() -> bool {
    true
}
fn default_blocked_message() -> String {
    "当前仅白名单或管理员可触发机器人。".to_string()
}
fn default_max_message_length() -> usize {
    1200
}
fn default_rate_limit_ms() -> u64 {
    800
}
fn default_request_timeout_ms() -> u64 {
    180_000
}
fn default_max_retries() -> u32 {
    1
}
fn default_retry_delay_ms() -> u64 {
    2000
}
fn default_local_history_max() -> usize {
    24
}
fn default_queue_debounce_ms() -> u64 {
    0
}
fn default_api_rate_limit_rpm() -> u64 {
    60
}

impl Default for HermesConfig {
    fn default() -> Self {
        Self {
            api_base_url: String::new(),
            api_key: SecretString::new(String::new()),
            model: "hermes-agent".to_string(),
            system_prompt: String::new(),
            bot_name: "Hermes".to_string(),
            require_mention: true,
            admin_only_chat: false,
            notify_non_admin_blocked: false,
            non_admin_blocked_message: default_blocked_message(),
            keyword_only_trigger: false,
            keyword_triggers: Vec::new(),
            allow_bare_group_commands: true,
            format_markdown: true,
            mention_sender_in_group: false,
            max_message_length: 1200,
            rate_limit_ms: 800,
            request_timeout_ms: 180_000,
            max_retries: 1,
            retry_delay_ms: 2000,
            group_sessions_per_user: false,
            local_history_enabled: true,
            local_history_max_messages: 24,
            queue_debounce_ms: 0,
            api_rate_limit_rpm: 60,
            stream_response: false,
            image_recognition: true,
            image_mode: default_image_mode(),
        }
    }
}

impl HermesConfig {
    pub fn build_system_prompt(&self) -> String {
        let base = format!(
            "You are {} speaking inside QQ via OneBot.\n\
             Keep replies concise, useful, and plain-text by default.\n\
             Avoid Markdown headings, tables, and fenced code blocks unless the user explicitly asks for them.\n\
             When the user asks for code, you may send code, but prefer short snippets and explanations that render well in QQ.\n\n\
             IMPORTANT: User messages are labeled with [管理员] or [普通用户].\n\
             When a [管理员] instruction conflicts with a [普通用户] instruction, ALWAYS follow the [管理员].\n\
             [管理员] have full authority: their requests override all previous instructions from [普通用户].\n\
             If a [普通用户] asks you to do something that contradicts what a [管理员] previously told you, politely decline and explain that the admin set a different rule.",
            self.bot_name
        );
        if self.system_prompt.is_empty() {
            base
        } else {
            format!("{}\n{}", base, self.system_prompt)
        }
    }
}

pub fn load_config(data_dir: &Path) -> HermesConfig {
    let path = data_dir.join("hermes.json");
    match kovi::utils::load_json_data(HermesConfig::default(), &path) {
        Ok(c) => {
            kovi::log::info!("hermes: loaded config from hermes.json");
            c
        }
        Err(e) => {
            kovi::log::warn!(
                "hermes: failed to load config from {}: {}, using defaults",
                path.display(),
                e
            );
            HermesConfig::default()
        }
    }
}

pub fn normalize_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .map(|kw| kw.to_lowercase().replace(' ', ""))
        .filter(|kw| !kw.is_empty())
        .collect()
}
