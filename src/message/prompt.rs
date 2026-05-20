pub(crate) fn build_context_label(is_group: bool, group_id: i64, user_id: i64, sender_name: &str, is_admin: bool) -> String {
    let role = if is_admin { "管理员" } else { "普通用户" };
    if is_group {
        format!(
            "当前来自 QQ 群 {group_id}。发送者: {sender_name} ({user_id}) [{role}]。请按 QQ 聊天风格回复。"
        )
    } else {
        format!(
            "当前来自 QQ 私聊用户 {sender_name} ({user_id}) [{role}]。请按 QQ 聊天风格回复。"
        )
    }
}

pub(crate) fn build_user_prompt(message: &str, context_label: &str) -> String {
    if context_label.is_empty() {
        message.to_string()
    } else {
        format!("{context_label}\n\n{message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_context_label_group() {
        let label = build_context_label(true, 12345, 67890, "Alice", true);
        assert!(label.contains("12345"));
        assert!(label.contains("Alice"));
        assert!(label.contains("67890"));
        assert!(label.contains("管理员"));
    }

    #[test]
    fn test_build_context_label_private() {
        let label = build_context_label(false, 0, 67890, "Bob", false);
        assert!(label.contains("Bob"));
        assert!(label.contains("67890"));
        assert!(label.contains("普通用户"));
    }

    #[test]
    fn test_build_user_prompt() {
        assert_eq!(build_user_prompt("hello", ""), "hello");
        assert_eq!(build_user_prompt("hello", "ctx"), "ctx\n\nhello");
    }
}