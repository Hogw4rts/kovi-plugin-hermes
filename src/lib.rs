#![deny(clippy::correctness)]
#![warn(clippy::suspicious, clippy::style, clippy::complexity, clippy::perf)]
#![allow(clippy::too_many_lines)]

mod cmd;
mod config;
mod guard;
mod llm;
mod message;
mod queue;
mod ratelimit;
mod reply;
mod routing;
mod secret;
mod session;
mod trigger;

use config::HermesConfig;
use guard::NotificationGuard;
use kovi::log::info;
use kovi::MsgEvent;
use kovi::PluginBuilder as plugin;
use llm::LlmClient;
use llm::StreamEvent;
use message::{build_context_label, build_user_prompt, clean_outbound_text, extract_image_urls};
use queue::SessionQueue;
use ratelimit::RateLimiter;
use reply::{is_reply_to_bot_message, reply_text};
use routing::{GroupId, MsgType, MessageRoute, UserId, build_base_session_key};
use session::SessionStore;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) static DATA_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

struct CachedConfig {
    inner: Arc<HermesConfig>,
    normalized_keywords: Arc<Vec<String>>,
    system_prompt: Arc<String>,
}

impl CachedConfig {
    fn config(&self) -> &HermesConfig {
        &self.inner
    }
}

#[kovi::plugin]
async fn main() {
    let bot = plugin::get_runtime_bot();
    let data_path = DATA_PATH.get_or_init(|| bot.get_data_path()).clone();
    let data_path = Arc::new(data_path);

    let config = config::load_config(&data_path);
    if config.api_base_url.is_empty() || config.api_key.is_empty() {
        kovi::log::error!(
            "hermes: api_base_url and api_key must be configured in hermes.json"
        );
        return;
    }
    info!("hermes: plugin loaded, model={}", config.model);

    let normalized_keywords = Arc::new(config::normalize_keywords(&config.keyword_triggers));
    let system_prompt = Arc::new(config.build_system_prompt());
    let use_stream = config.stream_response;
    let rate_limit_rpm = config.api_rate_limit_rpm;
    let config = Arc::new(config);

    let cached = Arc::new(CachedConfig {
        inner: config.clone(),
        normalized_keywords,
        system_prompt,
    });

    let store = Arc::new(SessionStore::new(&data_path).await);
    let rate_limiter = if rate_limit_rpm > 0 {
        RateLimiter::new(rate_limit_rpm)
    } else {
        RateLimiter::unlimited()
    };
    let llm = Arc::new(LlmClient::new(
        config.clone(),
        store.clone(),
        cached.system_prompt.clone(),
        rate_limiter,
    ));
    let queue = Arc::new(SessionQueue::new());
    let notif_guard = Arc::new(NotificationGuard::new());

    plugin::on_msg({
        let bot = bot.clone();
        let cached = cached.clone();
        let store = store.clone();
        let llm = llm.clone();
        let queue = queue.clone();
        let notif_guard = notif_guard.clone();

        move |event| {
            let bot = bot.clone();
            let cached = cached.clone();
            let store = store.clone();
            let llm = llm.clone();
            let queue = queue.clone();
            let notif_guard = notif_guard.clone();
            let self_id = event.self_id;

            async move {
                handle_message(&bot, &event, &cached, &store, &llm, &queue, &notif_guard, self_id, use_stream).await;
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn handle_message(
    bot: &kovi::RuntimeBot,
    event: &MsgEvent,
    cached: &CachedConfig,
    store: &SessionStore,
    llm: &LlmClient,
    queue: &SessionQueue,
    notif_guard: &NotificationGuard,
    self_id: i64,
    use_stream: bool,
) {
    let config = cached.config();
    let msg_type = if event.group_id.is_some() {
        MsgType::Group
    } else {
        MsgType::Private
    };

    let user_id = UserId(event.sender.user_id);
    let group_id = GroupId(event.group_id.unwrap_or(0));

    if user_id.0 == self_id {
        return;
    }

    let is_admin = bot.get_all_admin()
        .is_ok_and(|admins| admins.contains(&user_id.0));

    let route = MessageRoute {
        msg_type,
        user_id,
        group_id,
        message_id: event.message_id,
        sender_name: event
            .sender
            .nickname
            .clone()
            .unwrap_or_default(),
    };

    let text = match event.borrow_text() {
        Some(t) => t.trim().to_string(),
        None => String::new(),
    };

    let image_urls = if config.image_recognition {
        extract_image_urls(&event.message)
    } else {
        Vec::new()
    };

    if text.is_empty() && image_urls.is_empty() {
        return;
    }

    if config.admin_only_chat && !is_admin {
        if config.notify_non_admin_blocked && notif_guard.should_notify(user_id).await {
            reply_text(bot, &route, config, &config.non_admin_blocked_message).await;
        }
        return;
    }

    let keyword_hit = trigger::contains_keyword(&text, &cached.normalized_keywords);
    let is_cmd = trigger::is_command(&text);

    if msg_type == MsgType::Group {
        let is_reply_to_bot = is_reply_to_bot_message(bot, event, self_id).await;

        let result = trigger::decide_group_trigger(
            config,
            event,
            self_id,
            keyword_hit,
            is_cmd,
            is_reply_to_bot,
        );

        match result {
            trigger::TriggerResult::Triggered(reason) => {
                info!(
                    "hermes: accepted group trigger from user {user_id} in group {group_id} via {reason:?}"
                );
            }
            trigger::TriggerResult::NotTriggered => {
                return;
            }
        }
    }

    let base_key = build_base_session_key(&route, config);

    match cmd::handle_command(event, &text, config, llm, store, &base_key, is_admin).await {
        cmd::CommandResult::Handled => return,
        cmd::CommandResult::NotACommand => {}
        cmd::CommandResult::Failed(e) => {
            kovi::log::warn!("hermes: command failed: {e}");
            return;
        }
    }

    let session_id = store.session_id(&base_key).await;
    let context_label = build_context_label(
        msg_type == MsgType::Group,
        group_id.0,
        user_id.0,
        &route.sender_name,
        is_admin,
    );
    let user_prompt = build_user_prompt(&text, &context_label);

    handle_chat(bot, llm, store, config, queue, &route, &base_key, &session_id, &user_prompt, &image_urls, use_stream).await;
}

#[allow(clippy::too_many_arguments)]
async fn handle_chat(
    bot: &kovi::RuntimeBot,
    llm: &LlmClient,
    store: &SessionStore,
    config: &HermesConfig,
    queue: &SessionQueue,
    route: &MessageRoute,
    base_key: &str,
    session_id: &str,
    user_prompt: &str,
    image_urls: &[String],
    use_stream: bool,
) {
    let debounce_ms = config.queue_debounce_ms;
    let llm_clone = llm.clone();
    let store_clone = store.clone();
    let config_clone = config.clone();
    let bot_clone = bot.clone();
    let route_clone = route.clone();
    let session_id_owned = session_id.to_string();
    let user_prompt_owned = user_prompt.to_string();
    let image_urls_owned = image_urls.to_vec();

    queue.enqueue(base_key, move || {
        let llm = llm_clone;
        let store = store_clone;
        let config = config_clone;
        let bot = bot_clone;
        let route = route_clone;
        let session_id = session_id_owned;
        let user_prompt = user_prompt_owned;
        let image_urls = image_urls_owned;

        async move {
            if debounce_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(debounce_ms)).await;
            }

            if use_stream {
                handle_stream_reply(&llm, &config, &bot, &route, &session_id, &user_prompt, &image_urls).await;
            } else {
                handle_normal_reply(&llm, &config, &bot, &route, &session_id, &user_prompt, &image_urls).await;
            }

            drop(store);
        }
    }).await;
}

async fn handle_normal_reply(
    llm: &LlmClient,
    config: &HermesConfig,
    bot: &kovi::RuntimeBot,
    route: &MessageRoute,
    session_id: &str,
    user_prompt: &str,
    image_urls: &[String],
) {
    match llm.complete(session_id, user_prompt, None, image_urls).await {
        Ok(reply) => {
            let cleaned = clean_outbound_text(&reply, config.format_markdown);
            let outbound = if cleaned.is_empty() {
                "\u{8fd9}\u{8f6e}\u{6ca1}\u{6709}\u{8fd4}\u{56de}\u{53ef}\u{53d1}\u{9001}\u{7684}\u{6587}\u{672c}\u{3002}".to_string()
            } else {
                cleaned
            };
            reply_text(bot, route, config, &outbound).await;
        }
        Err(e) => {
            kovi::log::warn!("hermes: message handling failed for {session_id}: {e}");
            reply_text(
                bot,
                route,
                config,
                &format!("\u{8c03}\u{7528}\u{5931}\u{8d25}: {e}"),
            )
            .await;
        }
    }
}

async fn handle_stream_reply(
    llm: &LlmClient,
    config: &HermesConfig,
    bot: &kovi::RuntimeBot,
    route: &MessageRoute,
    session_id: &str,
    user_prompt: &str,
    image_urls: &[String],
) {
    let mut rx = match llm.complete_stream(session_id, user_prompt, None, image_urls).await {
        Ok(rx) => rx,
        Err(e) => {
            kovi::log::warn!("hermes: stream request failed for {session_id}: {e}");
            reply_text(
                bot,
                route,
                config,
                &format!("\u{8c03}\u{7528}\u{5931}\u{8d25}: {e}"),
            )
            .await;
            return;
        }
    };

    let mut buffer = String::new();
    let mut sent_any = false;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Delta(delta) => {
                buffer.push_str(&delta);
                let should_flush = buffer.len() >= config.max_message_length
                    || buffer.ends_with("\n\n");

                if should_flush {
                    let cleaned = clean_outbound_text(&buffer, config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(bot, route, config, &cleaned).await;
                        sent_any = true;
                    }
                    buffer.clear();
                }
            }
            StreamEvent::Done => {
                if !buffer.is_empty() {
                    let cleaned = clean_outbound_text(&buffer, config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(bot, route, config, &cleaned).await;
                        sent_any = true;
                    }
                }
                if !sent_any {
                    reply_text(
                        bot,
                        route,
                        config,
                        "\u{8fd9}\u{8f6e}\u{6ca1}\u{6709}\u{8fd4}\u{56de}\u{53ef}\u{53d1}\u{9001}\u{7684}\u{6587}\u{672c}\u{3002}",
                    )
                    .await;
                }
                break;
            }
            StreamEvent::Error(e) => {
                kovi::log::warn!("hermes: stream error for {session_id}: {e}");
                if !buffer.is_empty() {
                    let cleaned = clean_outbound_text(&buffer, config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(bot, route, config, &cleaned).await;
                        sent_any = true;
                    }
                }
                if !sent_any {
                    reply_text(
                        bot,
                        route,
                        config,
                        &format!("\u{8c03}\u{7528}\u{5931}\u{8d25}: {e}"),
                    )
                    .await;
                }
                break;
            }
        }
    }
}
