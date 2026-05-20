use crate::config::HermesConfig;
use crate::llm::LlmClient;
use crate::session::SessionStore;
use kovi::MsgEvent;

#[derive(Debug)]
#[must_use]
pub(crate) enum CommandResult {
    Handled,
    NotACommand,
    #[allow(dead_code)]
    Failed(String),
}

pub(crate) async fn handle_command(
    event: &MsgEvent,
    text: &str,
    config: &HermesConfig,
    llm: &LlmClient,
    store: &SessionStore,
    base_key: &str,
    is_admin: bool,
) -> CommandResult {
    let Some((name, args)) = crate::trigger::parse_command(text) else {
        return CommandResult::NotACommand;
    };

    match name {
        "ping" => {
            event.reply("pong");
            CommandResult::Handled
        }
        "help" => {
            if is_admin {
                let help_text = [
                    "可用命令 (管理员):",
                    "/ping - 连通性检查",
                    "/help - 查看帮助",
                    "/model - 查看当前模型与默认模型",
                    "/model list - 查看可用模型",
                    "/model <模型名> - 切换模型",
                    "/model reset - 恢复默认模型",
                    "/new - 新建会话",
                    "/reset - 新建会话",
                ]
                .join("\n");
                event.reply(help_text);
            } else {
                let help_text = [
                    "可用命令:",
                    "/ping - 连通性检查",
                    "/help - 查看帮助",
                ]
                .join("\n");
                event.reply(help_text);
            }
            CommandResult::Handled
        }
        "model" | "new" | "reset" => {
            if !is_admin {
                event.reply("该命令仅管理员可用。");
                return CommandResult::Handled;
            }
            match name {
                "model" => {
                    handle_model_command(event, args, config, llm, store).await;
                }
                "new" | "reset" => {
                    let new_id = store.bump_session(base_key).await;
                    kovi::log::info!("hermes: session bumped to {new_id}");
                    event.reply("已创建新会话。");
                }
                _ => unreachable!(),
            }
            CommandResult::Handled
        }
        _ => CommandResult::NotACommand,
    }
}

async fn handle_model_command(
    event: &MsgEvent,
    args: &str,
    config: &HermesConfig,
    llm: &LlmClient,
    store: &SessionStore,
) {
    if args.is_empty() {
        let selected = store.selected_model().await;
        let current = if selected.is_empty() {
            &config.model
        } else {
            &selected
        };
        event.reply(format!(
            "当前模型: {}\n默认模型: {}\n用法: /model list | /model <模型名> | /model reset",
            current, config.model
        ));
        return;
    }

    let sub = args.trim().to_lowercase();

    if sub == "list" || sub == "ls" || sub == "all" {
        match llm.list_models().await {
            Ok(models) => {
                let selected = store.selected_model().await;
                let current = if selected.is_empty() {
                    &config.model
                } else {
                    &selected
                };
                let mut lines = vec![
                    format!("当前模型: {}", current),
                    format!("默认模型: {}", config.model),
                    String::new(),
                    "可用模型:".to_string(),
                ];
                if models.is_empty() {
                    lines.push("- 当前 key 没有返回可用模型".to_string());
                } else {
                    for m in &models {
                        let prefix = if m == current { "* " } else { "- " };
                        lines.push(format!("{prefix}{m}"));
                    }
                }
                event.reply(lines.join("\n"));
            }
            Err(e) => {
                event.reply(format!("获取模型列表失败: {e}"));
            }
        }
        return;
    }

    if sub == "reset" || sub == "default" {
        store.clear_selected_model().await;
        event.reply(format!("已恢复默认模型: {}", config.model));
        return;
    }

    let requested = args.trim();
    if requested.is_empty() {
        event.reply("请提供模型名，例如: /model gpt-4o");
        return;
    }

    match llm.list_models().await {
        Ok(models) if !models.is_empty() && !models.iter().any(|m| m == requested) => {
            event.reply(format!(
                "当前 key 不支持模型: {requested}\n先执行 /model list 查看可用模型。"
            ));
        }
        _ => {
            store.set_selected_model(requested).await;
            event.reply(format!("已切换模型: {requested}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_result_debug() {
        let handled = CommandResult::Handled;
        let not_cmd = CommandResult::NotACommand;
        let failed = CommandResult::Failed("error".to_string());
        assert!(format!("{:?}", handled).contains("Handled"));
        assert!(format!("{:?}", not_cmd).contains("NotACommand"));
        assert!(format!("{:?}", failed).contains("error"));
    }
}