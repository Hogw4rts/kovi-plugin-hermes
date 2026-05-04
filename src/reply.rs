use crate::config::HermesConfig;
use crate::routing::MsgType;
use crate::routing::MessageRoute;
use kovi::MsgEvent;

pub async fn reply_text(
    bot: &kovi::RuntimeBot,
    route: &MessageRoute,
    config: &HermesConfig,
    text: &str,
) {
    let chunks = crate::message::split_message(text, config.max_message_length);

    if let Some(first) = chunks.first() {
        if route.msg_type == MsgType::Group {
            let mut msg = kovi::Message::new();
            msg = msg.add_reply(route.message_id);
            if config.mention_sender_in_group {
                msg = msg
                    .add_at(&route.user_id.to_string())
                    .add_text(format!(" {first}"));
            } else {
                msg = msg.add_text(first);
            }
            bot.send_group_msg(route.group_id.0, msg);
        } else {
            bot.send_private_msg(route.user_id.0, first);
        }
    }

    for chunk in chunks.iter().skip(1) {
        if route.msg_type == MsgType::Group {
            let msg = kovi::Message::new().add_text(chunk);
            bot.send_group_msg(route.group_id.0, msg);
        } else {
            bot.send_private_msg(route.user_id.0, chunk);
        }
        if config.rate_limit_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(config.rate_limit_ms)).await;
        }
    }
}

pub async fn is_reply_to_bot_message(
    bot: &kovi::RuntimeBot,
    event: &MsgEvent,
    self_id: i64,
) -> bool {
    let reply_segments = event.message.get("reply");
    if reply_segments.is_empty() {
        return false;
    }

    for seg in &reply_segments {
        if let Some(id_str) = seg.data.get("id").and_then(|v| v.as_str())
            && let Ok(msg_id) = id_str.parse::<i32>()
            && let Ok(resp) = bot.get_msg(msg_id).await
        {
            if let Some(sender_id) = resp
                .data
                .get("sender")
                .and_then(|s| s.get("user_id"))
                .and_then(serde_json::Value::as_i64)
            {
                return sender_id == self_id;
            }
            if let Some(user_id) = resp.data.get("user_id").and_then(serde_json::Value::as_i64) {
                return user_id == self_id;
            }
        }
    }
    false
}