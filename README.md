# kovi-plugin-hermes

Kovi 插件 — 将 QQ 消息桥接到 Hermes Agent Gateway（OpenAI 兼容 API），实现 LLM 对话、会话连续性、流式响应、模型切换和管理员权限控制。

> **Reference**: 本项目参考自 [hermes_qq](https://github.com/constansino/hermes_qq)（Node.js 实现），使用 Rust + Kovi 框架重写。

## 架构

```
QQ 用户
  │
  ▼
NapCat (OneBot v11)
  │
  ▼
Kovi Bot ─── kovi-plugin-hermes
  │              │
  │              ├─ 触发判断 (mention / keyword / command / reply / open)
  │              ├─ 权限过滤 (admin / whitelist / blacklist)
  │              ├─ 命令处理 (/ping /help /model /new /reset)
  │              ├─ 会话队列 (per-session 串行，读锁快速路径)
  │              ├─ API 限流 (Token Bucket)
  │              ├─ 消息清洗 (Markdown → 纯文本, 分段)
  │              ├─ 图片识别 (OpenAI Vision 多模态, 提取消息图片 URL)
  │              ├─ 流式响应 (SSE chunked delivery)
  │              └─ 管理员权重 ([管理员] / [普通用户] 标签)
  │
  ▼
Hermes Gateway (port 8642)
  │
  ▼
OpenAI-compatible /v1/chat/completions
  │
  ▼
LLM Provider (xiaomi / anthropic / ...)
```

## 项目结构

```
src/
├── lib.rs           # 插件入口、消息路由、handle_chat 分发
├── config.rs        # HermesConfig 配置加载、system prompt、关键词预归一化
├── trigger.rs       # 群聊触发判定 (mention / keyword / command / reply / open)
├── cmd.rs           # 命令处理器 (/ping /help /model /new /reset)
├── routing.rs       # MsgType, MessageRoute, UserId/GroupId newtype
├── queue.rs         # SessionQueue — per-session 串行队列 (RwLock + Mutex)
├── guard.rs         # NotificationGuard — 冷却通知 + TTL 淘汰
├── reply.rs         # reply_text 分段发送、is_reply_to_bot_message
├── message.rs       # Markdown 清洗 (20+ regex)、消息分段、上下文标签
├── session.rs       # SessionStore — debounce 持久化、TTL+LRU 会话淘汰
├── ratelimit.rs     # RateLimiter — Token Bucket API 限流
├── secret.rs        # SecretString — API key 内存安全 (volatile zeroing)
└── llm/
    ├── mod.rs       # LlmClient — 非流式/流式请求、重试、限流
    ├── types.rs     # Role enum, ChatRequest, ChatResponse, SSE chunk 类型
    └── error.rs     # LlmError (thiserror)
```

## 模块职责

### lib.rs — 入口与路由

- `#[kovi::plugin]` 入口，加载配置、初始化 `LlmClient` / `SessionStore` / `SessionQueue` / `RateLimiter`
- `CachedConfig` 预构建 system prompt 和归一化关键词，避免每次请求重复计算
- `on_msg` 注册消息监听，每条消息经过完整处理流水线
- `handle_message` 主流程：过滤自身消息 → 黑名单/白名单 → 群触发判定 → 命令分发 → LLM 调用
- `handle_chat` / `handle_normal_reply` / `handle_stream_reply` 分离 LLM 调用与回复逻辑

### config.rs — 配置

- `HermesConfig` 结构体（`#[non_exhaustive]`），JSON 持久化到 `data/kovi-plugin-hermes/hermes.json`
- `api_key` 使用 `SecretString`，Debug/Display 输出脱敏，drop 时内存清零
- `build_system_prompt()` 构建系统提示词，包含管理员权重声明
- `normalize_keywords()` 预归一化关键词（小写 + 去空格），启动时缓存
- 权限判断：admin 身份通过 Kovi 框架 `bot.get_all_admin()` 获取，用户/群组过滤由框架 ACL（kovi-plugin-acl）管理，参数使用 `UserId` / `GroupId` newtype

### trigger.rs — 群聊触发

三种模式由配置驱动：

| 模式 | 触发条件 |
|------|----------|
| `require_mention=true` (默认) | @机器人 / 关键词 / 命令 / 回复机器人 |
| `keyword_only_trigger=true` | 仅关键词 + 命令 |
| 两者都 false | 开放模式，所有消息都触发 |

- `TriggerResult` 为类型安全的 enum（`Triggered(TriggerReason)` / `NotTriggered`），消除非法状态
- `command_reason()` 辅助函数统一三种命令触发原因
- `has_at_self` 使用 `parse::<i64>()` 替代字符串比较，避免分配

### cmd.rs — 命令

| 命令 | 权限 | 说明 |
|------|------|------|
| `/ping` | 所有人 | 连通性检查 |
| `/help` | 所有人 | 查看帮助（普通用户只看基础命令） |
| `/model` | 管理员 | 查看当前/默认模型 |
| `/model list` | 管理员 | 列出可用模型 |
| `/model <name>` | 管理员 | 切换模型 |
| `/model reset` | 管理员 | 恢复默认模型 |
| `/new` / `/reset` | 管理员 | 新建会话 |

- `CommandResult` 包含 `Failed(String)` 变体，命令失败时记录日志

### llm/ — LLM 客户端

- **非流式**：`complete()` — 标准 `/v1/chat/completions` 请求，可配置重试
- **流式**：`complete_stream()` — SSE 流式响应，逐 chunk 发送至 QQ，缓冲区满或段落结束时 flush
- **图片识别**：当消息包含图片且 `image_recognition=true` 时，用户消息使用 OpenAI Vision 多模态格式（`content: [{type:"text"}, {type:"image_url"}]`），历史消息仅保存文本
- `X-Hermes-Session-Id` header 维持会话连续性
- `Authorization: Bearer <key>` 认证（key 通过 `SecretString` 安全存储）
- `RateLimiter` 集成：所有 API 请求前 `acquire()` 令牌
- SSE 解析使用 `Vec<u8>` buffer + consumed 指针，正确处理 `\r\n`、UTF-8 chunk 边界、连接断开
- `Role` enum（`System`/`User`/`Assistant`）替代字符串，`#[serde(rename_all = "lowercase")]`
- `LlmError` 使用 `thiserror` 实现 `std::error::Error`

### message.rs — 消息处理

- **Markdown 清洗**：20+ 正则（`LazyLock` 编译期初始化），`re_replace_all` 辅助函数
- **图片提取**：`extract_image_urls()` 从 OneBot 消息中提取 `image` 段的 `url` 字段，支持多图
- **消息分段**：`Vec::with_capacity(4)` 预分配，按段落/换行/空格拆分
- **上下文标签**：`build_context_label()` 标注来源群、发送者、管理员/普通用户身份
- **用户提示词**：`build_user_prompt()` 拼接上下文标签 + 用户消息

### session.rs — 会话状态

- `session-state.json` 持久化，重启不丢失
- **Debounce 持久化**：`AtomicBool` 脏标记 + 5 秒定时 flush（`kovi::spawn`），避免每次写操作都序列化
- `bump_session()` 立即 flush（会话重置应即时持久化）
- 会话版本控制：`/new` 命令递增版本号，旧会话历史自动清除
- 模型选择：`/model <name>` 切换，`/model reset` 恢复默认
- 对话历史：per-session 存储，可配置最大条数，FIFO 淘汰
- **TTL + LRU 淘汰**：`MAX_SESSIONS=500`，`SESSION_TTL_SECS=86400`，超限时先清过期再 LRU
- 全部文件 I/O 使用 `tokio::fs`，不阻塞 async runtime

### queue.rs — 会话队列

- `SessionQueue` 基于 `RwLock<HashMap<String, Arc<Mutex<()>>>>`
- 已存在 key 走读锁快速路径，新 key 才升级写锁，避免 TOCTOU 竞态

### guard.rs — 通知冷却

- `NotificationGuard` 读锁快速路径 + double-check 写锁
- `MAX_ENTRIES=10000`，`EVICT_THRESHOLD=8000`，双层淘汰（过期 + LRU）

### ratelimit.rs — API 限流

- Token Bucket 实现，`max_rpm` 配置每分钟最大请求数
- `RateLimiter::unlimited()` 使用 `LimiterKind::Unlimited` 枚举变体，零开销跳过

### secret.rs — API Key 安全

- `SecretString` 包装 `Vec<u8>`，`Drop` 时 `ptr::write_volatile` 清零 + `compiler_fence`
- `Debug` / `Display` 输出 `***REDACTED***`
- `Deref<Target=str>` 透明访问，`Serialize`/`Deserialize` 支持 JSON 持久化

### routing.rs — 类型安全路由

- `UserId(i64)` / `GroupId(i64)` newtype，`Display`/`Hash`/`Eq` 派生
- `MsgType` enum（`Private` / `Group`）
- `MessageRoute` 携带完整路由信息（`msg_type`, `user_id`, `group_id`, `message_id`, `sender_name`）

## 配置

配置文件位于 `data/kovi-plugin-hermes/hermes.json`，首次运行自动生成默认值。

```json
{
  "api_base_url": "http://hermes.tenant-1.svc.cluster.local:8642/v1",
  "api_key": "your-api-key",
  "model": "hermes-agent",
  "system_prompt": "",
  "bot_name": "塔菲",
  "require_mention": true,
  "admin_only_chat": false,
  "notify_non_admin_blocked": false,
  "non_admin_blocked_message": "当前仅白名单或管理员可触发机器人。",
  "keyword_only_trigger": false,
  "keyword_triggers": [],
  "allow_bare_group_commands": true,
  "format_markdown": true,
  "mention_sender_in_group": false,
  "max_message_length": 1200,
  "rate_limit_ms": 800,
  "request_timeout_ms": 180000,
  "max_retries": 1,
  "retry_delay_ms": 2000,
  "group_sessions_per_user": false,
  "local_history_enabled": true,
  "local_history_max_messages": 24,
  "queue_debounce_ms": 0,
  "api_rate_limit_rpm": 60,
  "stream_response": false,
  "image_recognition": true
}
```

### 配置项说明

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `api_base_url` | string | `""` | LLM API 地址（必填） |
| `api_key` | string | `""` | API 密钥（必填，内存中安全存储） |
| `model` | string | `"hermes-agent"` | 默认模型 |
| `system_prompt` | string | `""` | 追加系统提示词 |
| `bot_name` | string | `"Hermes"` | 机器人名称，用于 system prompt |
| `require_mention` | bool | `true` | 群聊是否需要 @机器人 才触发 |
| `admin_only_chat` | bool | `false` | 仅管理员可对话 |
| `notify_non_admin_blocked` | bool | `false` | 非管理员被拦截时是否通知 |
| `non_admin_blocked_message` | string | 见上 | 拦截通知消息 |
| `keyword_only_trigger` | bool | `false` | 仅关键词触发模式 |
| `keyword_triggers` | string[] | `[]` | 触发关键词列表 |
| `allow_bare_group_commands` | bool | `true` | 群聊允许不带 @ 的命令 |
| `format_markdown` | bool | `true` | 清洗 Markdown 格式 |
| `mention_sender_in_group` | bool | `false` | 群聊回复时 @发送者 |
| `max_message_length` | usize | `1200` | 单条消息最大字符数 |
| `rate_limit_ms` | u64 | `800` | 分段发送间隔（毫秒） |
| `request_timeout_ms` | u64 | `180000` | LLM 请求超时 |
| `max_retries` | u32 | `1` | 最大重试次数 |
| `retry_delay_ms` | u64 | `2000` | 重试间隔 |
| `group_sessions_per_user` | bool | `false` | 群聊每人独立会话 |
| `local_history_enabled` | bool | `true` | 启用本地对话历史 |
| `local_history_max_messages` | usize | `24` | 每会话最大历史条数 |
| `queue_debounce_ms` | u64 | `0` | 队列防抖延迟 |
| `api_rate_limit_rpm` | u64 | `60` | API 每分钟请求上限（0=不限） |
| `stream_response` | bool | `false` | 启用 SSE 流式响应 |
| `image_recognition` | bool | `true` | 启用图片识别（提取消息中的图片 URL 发送给 LLM） |

## 管理员权重

管理员身份由 Kovi 框架管理（`kovi.conf.toml` 中的 `main_admin` 和 `deputy_admins`），插件通过 `bot.get_all_admin()` 查询，无需在 hermes.json 中重复配置。

管理员享有以下特权：

1. **身份标签**：发送给 LLM 的消息标注 `[管理员]`，普通用户标注 `[普通用户]`
2. **系统提示词**：明确声明管理员指令优先于普通用户
3. **专属命令**：`/model`、`/new`、`/reset` 仅管理员可用，普通用户执行会收到"该命令仅管理员可用"
4. **帮助分级**：`/help` 根据身份显示不同命令列表

> **用户/群组过滤**：由框架 ACL 插件（kovi-plugin-acl）统一管理，hermes 不再维护 `admins`、`allowed_users`、`allowed_groups`、`blocked_users` 配置项。

## 安全特性

- **API Key 内存安全**：`SecretString` 在 drop 时通过 `ptr::write_volatile` 清零，`Debug`/`Display` 输出脱敏
- **API 限流**：Token Bucket 限流器，防止突发请求压垮上游
- **会话隔离**：per-session 串行队列，同一会话不并发请求 LLM
- **会话淘汰**：TTL (24h) + LRU，防止内存无限增长

## 部署

插件作为 Kovi 框架的子模块编译，通过 ConfigMap 挂载 `hermes.json` 到 `/app/data/kovi-plugin-hermes/` 目录。

依赖服务：
- **Hermes Gateway**：需启用 API Server（`API_SERVER_HOST=0.0.0.0`，`API_SERVER_KEY=<key>`），暴露 8642 端口
- **LLM Provider**：Hermes 连接的模型服务（xiaomi/anthropic 等）

## License

GPL-3.0