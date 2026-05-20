use crate::config::HermesConfig;
use kovi::MsgEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerReason {
    Mention,
    Reply,
    Keyword,
    Command,
    MentionCommand,
    ReplyCommand,
    Open,
}

pub(crate) enum TriggerResult {
    Triggered(TriggerReason),
    NotTriggered,
}

fn command_reason(mentioned: bool, is_reply_to_bot: bool, allow_bare: bool) -> TriggerReason {
    if allow_bare && !mentioned && !is_reply_to_bot {
        TriggerReason::Command
    } else if mentioned {
        TriggerReason::MentionCommand
    } else {
        TriggerReason::ReplyCommand
    }
}

pub(crate) fn decide_group_trigger(
    config: &HermesConfig,
    event: &MsgEvent,
    self_id: i64,
    keyword_hit: bool,
    is_command: bool,
    is_reply_to_bot: bool,
) -> TriggerResult {
    let mentioned = has_at_self(event, self_id);

    let can_run_command =
        is_command && (mentioned || is_reply_to_bot || config.allow_bare_group_commands);

    if config.keyword_only_trigger {
        if keyword_hit {
            return TriggerResult::Triggered(TriggerReason::Keyword);
        }
        if can_run_command {
            return TriggerResult::Triggered(command_reason(
                mentioned,
                is_reply_to_bot,
                config.allow_bare_group_commands,
            ));
        }
        return TriggerResult::NotTriggered;
    }

    if config.require_mention {
        if keyword_hit {
            return TriggerResult::Triggered(TriggerReason::Keyword);
        }
        if can_run_command {
            return TriggerResult::Triggered(command_reason(
                mentioned,
                is_reply_to_bot,
                config.allow_bare_group_commands,
            ));
        }
        if mentioned {
            return TriggerResult::Triggered(TriggerReason::Mention);
        }
        if is_reply_to_bot {
            return TriggerResult::Triggered(TriggerReason::Reply);
        }
        return TriggerResult::NotTriggered;
    }

    if keyword_hit {
        return TriggerResult::Triggered(TriggerReason::Keyword);
    }
    if can_run_command {
        return TriggerResult::Triggered(command_reason(
            mentioned,
            is_reply_to_bot,
            config.allow_bare_group_commands,
        ));
    }

    TriggerResult::Triggered(TriggerReason::Open)
}

pub(crate) fn has_at_self(event: &MsgEvent, self_id: i64) -> bool {
    let msg = &event.message;
    for seg in msg.get("at") {
        if let Some(qq) = seg.data.get("qq").and_then(|v| v.as_str())
            && qq.parse::<i64>() == Ok(self_id)
        {
            return true;
        }
    }
    false
}

pub(crate) fn contains_keyword(text: &str, normalized_keywords: &[String]) -> bool {
    if normalized_keywords.is_empty() {
        return false;
    }
    let normalized = text.to_lowercase().replace(' ', "");
    normalized_keywords.iter().any(|kw| normalized.contains(kw))
}

pub(crate) fn is_command(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('/') && trimmed.len() > 1 && !trimmed.starts_with("//")
}

pub(crate) fn parse_command(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let without_slash = &trimmed[1..];
    let (name, rest) = without_slash
        .split_once(' ')
        .unwrap_or((without_slash, ""));
    if name.is_empty() {
        return None;
    }
    Some((name, rest.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_command() {
        assert!(is_command("/ping"));
        assert!(is_command("/model list"));
        assert!(!is_command("hello"));
        assert!(!is_command("//comment"));
        assert!(!is_command("/"));
        assert!(is_command(" /ping"));
    }

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("/ping"), Some(("ping", "")));
        assert_eq!(parse_command("/model list"), Some(("model", "list")));
        assert_eq!(parse_command("/model  gpt-4o "), Some(("model", "gpt-4o")));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command("/"), None);
    }

    #[test]
    fn test_contains_keyword() {
        let kws: Vec<String> = vec!["hello".to_string(), "world".to_string()];
        let normalized: Vec<String> = kws.iter().map(|k| k.to_lowercase().replace(' ', "")).collect();
        assert!(contains_keyword("say Hello there", &normalized));
        assert!(contains_keyword("the World is big", &normalized));
        assert!(!contains_keyword("no match here", &normalized));
    }

    #[test]
    fn test_contains_keyword_empty() {
        assert!(!contains_keyword("anything", &[]));
    }

    #[test]
    fn test_command_reason() {
        assert_eq!(command_reason(true, false, true), TriggerReason::MentionCommand);
        assert_eq!(command_reason(false, true, true), TriggerReason::ReplyCommand);
        assert_eq!(command_reason(false, false, true), TriggerReason::Command);
        assert_eq!(command_reason(false, false, false), TriggerReason::ReplyCommand);
    }
}