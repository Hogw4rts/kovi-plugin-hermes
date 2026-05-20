#![deny(clippy::correctness)]
#![warn(clippy::suspicious, clippy::style, clippy::complexity, clippy::perf)]
#![allow(clippy::too_many_lines)]

mod cmd;
mod config;
mod guard;
mod llm;
mod message;
mod onebot_api;
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
use message::{build_context_label, build_user_prompt, clean_outbound_text, extract_image_urls, extract_reply_image_urls};
use queue::SessionQueue;
use ratelimit::RateLimiter;
use reply::{is_reply_to_bot_message, reply_text};
use routing::{GroupId, MsgType, MessageRoute, UserInput, UserId, build_base_session_key};
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

    if let Err(errors) = config.validate() {
        for e in &errors {
            kovi::log::error!("hermes: config error: {e}");
        }
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
    let llm = match LlmClient::new(
        config.clone(),
        store.clone(),
        cached.system_prompt.clone(),
        rate_limiter,
    ) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            kovi::log::error!("hermes: failed to initialize LLM client: {e}");
            return;
        }
    };
    let queue = Arc::new(SessionQueue::new());
    let notif_guard = Arc::new(NotificationGuard::new());

    if config.onebot_api_enabled && !config.onebot_api_key.is_empty() {
        let ob_state = onebot_api::OnebotState::new(
            bot.clone(),
            config.onebot_api_key.clone(),
            config.onebot_admin_key.clone(),
            config.onebot_allowed_origins.clone(),
        );
        let ob_port = config.onebot_api_port;
        let ob_bind = config.onebot_api_bind.clone();
        let ob_bind_log = ob_bind.clone();
        tokio::spawn(async move {
            if let Err(e) = onebot_api::start(ob_state, &ob_bind, ob_port).await {
                kovi::log::error!("hermes: OneBot API server error: {e}");
            }
        });
        info!("hermes: OneBot API enabled on {ob_bind_log}:{ob_port}");
    }

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
    store: &Arc<SessionStore>,
    llm: &Arc<LlmClient>,
    queue: &Arc<SessionQueue>,
    notif_guard: &Arc<NotificationGuard>,
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

    kovi::log::info!(
        "hermes: received message_id={} from {} in {:?}",
        event.message_id,
        user_id.0,
        event.group_id
    );

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

    let mut image_urls = if config.image_recognition {
        extract_image_urls(bot, &event.message, llm.http(), config.image_mode).await
    } else {
        Vec::new()
    };

    if image_urls.is_empty() && config.image_recognition && event.message.contains("reply") {
        let reply_urls = extract_reply_image_urls(bot, &event.message, llm.http(), config.image_mode).await;
        if !reply_urls.is_empty() {
            kovi::log::info!("hermes: extracted {} image(s) from replied message", reply_urls.len());
            image_urls = reply_urls;
        }
    }

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
    let user_input = UserInput {
        text: if text.is_empty() && !image_urls.is_empty() {
            "[图片]".to_string()
        } else {
            text
        },
        image_urls,
    };
    let user_prompt = build_user_prompt(&user_input.text, &context_label);

    handle_chat(bot, llm, &cached.inner, queue, &route, &base_key, &session_id, &user_prompt, &user_input, use_stream).await;
}

struct ChatContext {
    llm: Arc<LlmClient>,
    config: Arc<HermesConfig>,
    bot: kovi::RuntimeBot,
    route: MessageRoute,
    session_id: String,
    user_prompt: String,
    user_input: UserInput,
}

#[allow(clippy::too_many_arguments)]
async fn handle_chat(
    bot: &kovi::RuntimeBot,
    llm: &Arc<LlmClient>,
    config: &Arc<HermesConfig>,
    queue: &Arc<SessionQueue>,
    route: &MessageRoute,
    base_key: &str,
    session_id: &str,
    user_prompt: &str,
    user_input: &UserInput,
    use_stream: bool,
) {
    let debounce_ms = config.queue_debounce_ms;
    let ctx = ChatContext {
        llm: Arc::clone(llm),
        config: Arc::clone(config),
        bot: bot.clone(),
        route: route.clone(),
        session_id: session_id.to_string(),
        user_prompt: user_prompt.to_string(),
        user_input: user_input.clone(),
    };

    queue.enqueue(base_key, move || {
        async move {
            if debounce_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(debounce_ms)).await;
            }

            if use_stream {
                handle_stream_reply(&ctx).await;
            } else {
                handle_normal_reply(&ctx).await;
            }
        }
    }).await;
}

async fn handle_normal_reply(ctx: &ChatContext) {
    let image_refs = ctx.user_input.image_urls_as_str();
    match ctx.llm.complete(&ctx.session_id, &ctx.user_prompt, None, &image_refs).await {
        Ok(reply) => {
            let cleaned = clean_outbound_text(&reply, ctx.config.format_markdown);
            let outbound = if cleaned.is_empty() {
                "这轮没有返回可发送的文本。".to_string()
            } else {
                cleaned
            };
            reply_text(&ctx.bot, &ctx.route, &ctx.config, &outbound).await;
        }
        Err(e) => {
            kovi::log::warn!("hermes: message handling failed for {}: {e}", ctx.session_id);
            reply_text(
                &ctx.bot,
                &ctx.route,
                &ctx.config,
                "请求失败，请稍后重试。",
            )
            .await;
        }
    }
}

async fn handle_stream_reply(ctx: &ChatContext) {
    let image_refs = ctx.user_input.image_urls_as_str();
    let mut rx = match ctx.llm.complete_stream(&ctx.session_id, &ctx.user_prompt, None, &image_refs).await {
        Ok(rx) => rx,
        Err(e) => {
            kovi::log::warn!("hermes: stream request failed for {}: {e}", ctx.session_id);
            reply_text(
                &ctx.bot,
                &ctx.route,
                &ctx.config,
                "请求失败，请稍后重试。",
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
                let should_flush = buffer.len() >= ctx.config.max_message_length
                    || buffer.ends_with("\n\n");

                if should_flush {
                    let cleaned = clean_outbound_text(&buffer, ctx.config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(&ctx.bot, &ctx.route, &ctx.config, &cleaned).await;
                        sent_any = true;
                    }
                    buffer.clear();
                }
            }
            StreamEvent::Done => {
                if !buffer.is_empty() {
                    let cleaned = clean_outbound_text(&buffer, ctx.config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(&ctx.bot, &ctx.route, &ctx.config, &cleaned).await;
                        sent_any = true;
                    }
                }
                if !sent_any {
                    reply_text(
                        &ctx.bot,
                        &ctx.route,
                        &ctx.config,
                        "这轮没有返回可发送的文本。",
                    )
                    .await;
                }
                break;
            }
            StreamEvent::Error(e) => {
                kovi::log::warn!("hermes: stream error for {}: {e}", ctx.session_id);
                if !buffer.is_empty() {
                    let cleaned = clean_outbound_text(&buffer, ctx.config.format_markdown);
                    if !cleaned.is_empty() {
                        reply_text(&ctx.bot, &ctx.route, &ctx.config, &cleaned).await;
                        sent_any = true;
                    }
                }
                if !sent_any {
                    reply_text(
                        &ctx.bot,
                        &ctx.route,
                        &ctx.config,
                        "请求失败，请稍后重试。",
                    )
                    .await;
                }
                break;
            }
        }
    }
}