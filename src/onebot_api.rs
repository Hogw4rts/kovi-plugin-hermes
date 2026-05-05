use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use kovi::bot::runtimebot::CanSendApi;
use kovi::{Message, RuntimeBot};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::secret::SecretString;

#[derive(Clone)]
pub struct OnebotState {
    pub bot: Arc<RuntimeBot>,
    pub api_key: SecretString,
    pub admin_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
#[allow(dead_code)]
pub enum WriteMode {
    Disabled,
    Confirm,
    Direct,
}

#[allow(dead_code)]
enum OnebotError {
    Unauthorized,
    Forbidden(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for OnebotError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            OnebotError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            OnebotError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            OnebotError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            OnebotError::Internal(detail) => {
                kovi::log::error!("OneBot API: internal error: {}", detail);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

#[derive(Deserialize)]
struct GroupMemberQuery {
    group_id: i64,
    user_id: i64,
    #[serde(default)]
    no_cache: bool,
}

#[derive(Deserialize)]
struct UserIdQuery {
    user_id: i64,
    #[serde(default)]
    no_cache: bool,
}

#[derive(Deserialize)]
struct MsgIdQuery {
    message_id: i32,
}

#[derive(Deserialize)]
struct ForwardMsgQuery {
    id: String,
}

#[derive(Deserialize)]
struct GroupHonorQuery {
    group_id: i64,
    #[serde(default)]
    honor_type: Option<String>,
}

#[derive(Deserialize)]
struct GroupMemberListQuery {
    group_id: i64,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}

#[derive(Deserialize)]
struct GroupInfoQuery {
    group_id: i64,
    #[serde(default)]
    no_cache: bool,
}

fn default_limit() -> u32 {
    100
}

const MAX_MEMBER_LIMIT: u32 = 200;

#[derive(Deserialize)]
#[serde(untagged)]
enum MessageInput {
    Text(String),
    Segments(Vec<SegmentInput>),
}

#[derive(Deserialize)]
struct SegmentInput {
    #[serde(rename = "type")]
    type_: String,
    data: Value,
}

impl MessageInput {
    fn to_message(&self) -> Message {
        match self {
            MessageInput::Text(text) => Message::new().add_text(text),
            MessageInput::Segments(segments) => {
                let mut msg = Message::new();
                for seg in segments {
                    match seg.type_.as_str() {
                        "text" => {
                            if let Some(text) = seg.data.get("text").and_then(|v| v.as_str()) {
                                msg = msg.add_text(text);
                            }
                        }
                        "image" => {
                            if let Some(file) = seg.data.get("file").and_then(|v| v.as_str()) {
                                msg = msg.add_image(file);
                            }
                        }
                        "at" => {
                            if let Some(qq) = seg.data.get("qq").and_then(|v| v.as_str()) {
                                msg = msg.add_at(qq);
                            }
                        }
                        "reply" => {
                            if let Some(id) = seg.data.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i32>().ok()) {
                                msg = msg.add_reply(id);
                            }
                        }
                        "face" => {
                            if let Some(id) = seg.data.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()) {
                                msg = msg.add_face(id);
                            }
                        }
                        _ => {
                            let seg_val = serde_json::json!({
                                "type": seg.type_,
                                "data": seg.data,
                            });
                            msg = msg.add_segment(seg_val);
                        }
                    }
                }
                msg
            }
        }
    }
}

#[derive(Deserialize)]
struct SendGroupMsgBody {
    group_id: i64,
    message: MessageInput,
}

#[derive(Deserialize)]
struct SendPrivateMsgBody {
    user_id: i64,
    message: MessageInput,
}

#[derive(Deserialize)]
struct SetGroupBanBody {
    group_id: i64,
    user_id: i64,
    #[serde(default)]
    duration: u64,
}

#[derive(Deserialize)]
struct SetGroupKickBody {
    group_id: i64,
    user_id: i64,
    #[serde(default)]
    reject_add_request: bool,
}

#[derive(Deserialize)]
struct SetGroupWholeBanBody {
    group_id: i64,
    enable: bool,
}

#[derive(Deserialize)]
struct SetGroupAdminBody {
    group_id: i64,
    user_id: i64,
    enable: bool,
}

#[derive(Deserialize)]
struct SetGroupAnonymousBody {
    group_id: i64,
    enable: bool,
}

#[derive(Deserialize)]
struct SetGroupAnonymousBanBody {
    group_id: i64,
    #[serde(default)]
    anonymous: Option<serde_json::Value>,
    #[serde(default)]
    flag: Option<String>,
    #[serde(default)]
    duration: u64,
}

#[derive(Deserialize)]
struct SetGroupCardBody {
    group_id: i64,
    user_id: i64,
    #[serde(default)]
    card: String,
}

#[derive(Deserialize)]
struct SetGroupNameBody {
    group_id: i64,
    group_name: String,
}

#[derive(Deserialize)]
struct SetGroupLeaveBody {
    group_id: i64,
    #[serde(default)]
    is_dismiss: bool,
}

#[derive(Deserialize)]
struct SetGroupSpecialTitleBody {
    group_id: i64,
    user_id: i64,
    #[serde(default)]
    special_title: String,
}

#[derive(Deserialize)]
struct SendLikeBody {
    user_id: i64,
    times: u32,
}

#[derive(Deserialize)]
struct SetFriendAddRequestBody {
    flag: String,
    #[serde(default)]
    approve: bool,
    #[serde(default)]
    remark: String,
}

#[derive(Deserialize)]
struct SetGroupAddRequestBody {
    flag: String,
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    approve: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
struct DeleteMsgBody {
    message_id: i32,
}

#[derive(Deserialize)]
struct DomainQuery {
    domain: String,
}

#[derive(Deserialize)]
struct GetRecordQuery {
    file: String,
    out_format: String,
}

#[derive(Deserialize)]
struct GetImageQuery {
    file: String,
}

pub fn router() -> Router<OnebotState> {
    Router::new()
        .route("/onebot/login_info", get(login_info))
        .route("/onebot/friend_list", get(friend_list))
        .route("/onebot/group_list", get(group_list))
        .route("/onebot/group_info", get(group_info))
        .route("/onebot/group_member_list", get(group_member_list))
        .route("/onebot/group_member_info", get(group_member_info))
        .route("/onebot/stranger_info", get(stranger_info))
        .route("/onebot/get_msg", get(get_msg))
        .route("/onebot/get_forward_msg", get(get_forward_msg))
        .route("/onebot/group_honor_info", get(group_honor_info))
        .route("/onebot/status", get(status))
        .route("/onebot/version_info", get(version_info))
        .route("/onebot/can_send_image", get(can_send_image))
        .route("/onebot/can_send_record", get(can_send_record))
        .route("/onebot/cookies", get(cookies))
        .route("/onebot/csrf_token", get(csrf_token))
        .route("/onebot/credentials", get(credentials))
        .route("/onebot/record", get(record))
        .route("/onebot/image", get(image))
        .route("/onebot/send_group_msg", post(send_group_msg))
        .route("/onebot/send_private_msg", post(send_private_msg))
        .route("/onebot/send_like", post(send_like))
        .route("/onebot/set_group_ban", post(set_group_ban))
        .route("/onebot/set_group_whole_ban", post(set_group_whole_ban))
        .route("/onebot/set_group_kick", post(set_group_kick))
        .route("/onebot/set_group_admin", post(set_group_admin))
        .route("/onebot/set_group_anonymous", post(set_group_anonymous))
        .route("/onebot/set_group_anonymous_ban", post(set_group_anonymous_ban))
        .route("/onebot/set_group_card", post(set_group_card))
        .route("/onebot/set_group_name", post(set_group_name))
        .route("/onebot/set_group_leave", post(set_group_leave))
        .route("/onebot/set_group_special_title", post(set_group_special_title))
        .route("/onebot/set_friend_add_request", post(set_friend_add_request))
        .route("/onebot/set_group_add_request", post(set_group_add_request))
        .route("/onebot/delete_msg", post(delete_msg))
        .route("/onebot/clean_cache", post(clean_cache))
        // ── NapCat extended: read ──
        .route("/onebot/get_group_msg_history", get(get_group_msg_history))
        .route("/onebot/get_group_file_system_info", get(get_group_file_system_info))
        .route("/onebot/get_group_root_files", get(get_group_root_files))
        .route("/onebot/get_group_files_by_folder", get(get_group_files_by_folder))
        .route("/onebot/get_group_file_url", get(get_group_file_url))
        .route("/onebot/get_file", get(get_file))
        .route("/onebot/get_group_at_all_remain", get(get_group_at_all_remain))
        .route("/onebot/get_essence_msg_list", get(get_essence_msg_list))
        .route("/onebot/download_file", post(download_file))
        // ── NapCat extended: write ──
        .route("/onebot/upload_group_file", post(upload_group_file))
        .route("/onebot/upload_private_file", post(upload_private_file))
        .route("/onebot/delete_group_file", post(delete_group_file))
        .route("/onebot/create_group_file_folder", post(create_group_file_folder))
        .route("/onebot/delete_group_folder", post(delete_group_folder))
        .route("/onebot/set_essence_msg", post(set_essence_msg))
        .route("/onebot/delete_essence_msg", post(delete_essence_msg))
        .route("/onebot/send_group_forward_msg", post(send_group_forward_msg))
}

pub async fn start(
    state: OnebotState,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use axum::http::{Method, header};
    use tower_http::cors::{AllowOrigin, CorsLayer};
    use tower_http::trace::TraceLayer;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    let app = Router::new()
        .merge(router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    kovi::log::info!("hermes: OneBot API listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn auth_middleware(
    State(state): State<OnebotState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let path = req.uri().path().to_string();

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth_header {
        Some(key) if key == state.api_key.as_str() => {}
        _ => {
            kovi::log::warn!("OneBot API: unauthorized request to {}", path);
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            )
                .into_response();
        }
    }

    next.run(req).await
}

fn require_admin(headers: &HeaderMap, admin_ids: &[i64]) -> Result<(), OnebotError> {
    let admin_id = headers
        .get("X-Admin-Id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

    match admin_id {
        Some(id) if admin_ids.contains(&id) => Ok(()),
        Some(id) => Err(OnebotError::Forbidden(format!(
            "admin id {id} not authorized"
        ))),
        None => Err(OnebotError::Forbidden(
            "X-Admin-Id header required for write operations".to_string(),
        )),
    }
}

async fn login_info(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_login_info()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_login_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn friend_list(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_friend_list()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_friend_list: {e}")))?;
    Ok(Json(resp.data))
}

async fn group_list(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_group_list()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_list: {e}")))?;
    Ok(Json(resp.data))
}

async fn group_info(
    State(state): State<OnebotState>,
    Query(query): Query<GroupInfoQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_group_info(query.group_id, query.no_cache)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn group_member_list(
    State(state): State<OnebotState>,
    Query(query): Query<GroupMemberListQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let limit = query.limit.min(MAX_MEMBER_LIMIT);
    let resp = state
        .bot
        .get_group_member_list(query.group_id)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_member_list: {e}")))?;

    let members = resp
        .data
        .as_array()
        .map(|arr| {
            let start = query.offset as usize;
            let end = (start + limit as usize).min(arr.len());
            if start < arr.len() {
                arr[start..end].to_vec()
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "group_id": query.group_id,
        "offset": query.offset,
        "limit": limit,
        "total": resp.data.as_array().map(|a| a.len()).unwrap_or(0),
        "members": members,
    })))
}

async fn group_member_info(
    State(state): State<OnebotState>,
    Query(query): Query<GroupMemberQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_group_member_info(query.group_id, query.user_id, query.no_cache)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_member_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn stranger_info(
    State(state): State<OnebotState>,
    Query(query): Query<UserIdQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_stranger_info(query.user_id, query.no_cache)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_stranger_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_msg(
    State(state): State<OnebotState>,
    Query(query): Query<MsgIdQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_msg(query.message_id)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_msg: {e}")))?;
    Ok(Json(resp.data))
}

async fn send_group_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendGroupMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let msg = body.message.to_message();
    let msg_id = state
        .bot
        .send_group_msg_return(body.group_id, msg)
        .await
        .map_err(|e| OnebotError::Internal(format!("send_group_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_group_msg group_id={} admin={:?} message_id={}",
        body.group_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok()),
        msg_id
    );
    Ok(Json(serde_json::json!({ "ok": true, "message_id": msg_id })))
}

async fn send_private_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendPrivateMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let msg = body.message.to_message();
    let msg_id = state
        .bot
        .send_private_msg_return(body.user_id, msg)
        .await
        .map_err(|e| OnebotError::Internal(format!("send_private_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_private_msg user_id={} admin={:?} message_id={}",
        body.user_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok()),
        msg_id
    );
    Ok(Json(serde_json::json!({ "ok": true, "message_id": msg_id })))
}

async fn set_group_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state
        .bot
        .set_group_ban(body.group_id, body.user_id, body.duration as usize);
    kovi::log::info!(
        "OneBot API: set_group_ban group_id={} user_id={} duration={} admin={:?}",
        body.group_id,
        body.user_id,
        body.duration,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_whole_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupWholeBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_whole_ban(body.group_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_whole_ban group_id={} enable={} admin={:?}",
        body.group_id,
        body.enable,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_kick(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupKickBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state
        .bot
        .set_group_kick(body.group_id, body.user_id, body.reject_add_request);
    kovi::log::info!(
        "OneBot API: set_group_kick group_id={} user_id={} admin={:?}",
        body.group_id,
        body.user_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_admin(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAdminBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_admin(body.group_id, body.user_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_admin group_id={} user_id={} enable={} admin={:?}",
        body.group_id,
        body.user_id,
        body.enable,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_anonymous(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAnonymousBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_anonymous(body.group_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_anonymous group_id={} enable={} admin={:?}",
        body.group_id,
        body.enable,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_anonymous_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAnonymousBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    match (&body.anonymous, &body.flag) {
        (Some(anonymous), _) => {
            state.bot.set_group_anonymous_ban_use_anonymous(body.group_id, anonymous.clone(), body.duration as usize);
        }
        (None, Some(flag)) => {
            state.bot.set_group_anonymous_ban_use_flag(body.group_id, flag, body.duration as usize);
        }
        (None, None) => {
            return Err(OnebotError::BadRequest(
                "either 'anonymous' or 'flag' must be provided".to_string(),
            ));
        }
    }
    kovi::log::info!(
        "OneBot API: set_group_anonymous_ban group_id={} duration={} admin={:?}",
        body.group_id,
        body.duration,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_card(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupCardBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_card(body.group_id, body.user_id, &body.card);
    kovi::log::info!(
        "OneBot API: set_group_card group_id={} user_id={} admin={:?}",
        body.group_id,
        body.user_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_name(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupNameBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_name(body.group_id, &body.group_name);
    kovi::log::info!(
        "OneBot API: set_group_name group_id={} admin={:?}",
        body.group_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_leave(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupLeaveBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_leave(body.group_id, body.is_dismiss);
    kovi::log::info!(
        "OneBot API: set_group_leave group_id={} is_dismiss={} admin={:?}",
        body.group_id,
        body.is_dismiss,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_special_title(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupSpecialTitleBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_group_special_title(body.group_id, body.user_id, &body.special_title);
    kovi::log::info!(
        "OneBot API: set_group_special_title group_id={} user_id={} admin={:?}",
        body.group_id,
        body.user_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn send_like(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendLikeBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.send_like(body.user_id, body.times as usize);
    kovi::log::info!(
        "OneBot API: send_like user_id={} times={} admin={:?}",
        body.user_id,
        body.times,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_friend_add_request(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetFriendAddRequestBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.set_friend_add_request(&body.flag, body.approve, &body.remark);
    kovi::log::info!(
        "OneBot API: set_friend_add_request flag={} approve={} admin={:?}",
        body.flag,
        body.approve,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_group_add_request(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAddRequestBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.send_api("set_group_add_request", serde_json::json!({
        "flag": body.flag,
        "sub_type": body.type_,
        "approve": body.approve,
        "reason": body.reason,
    }));
    kovi::log::info!(
        "OneBot API: set_group_add_request flag={} type={} approve={} admin={:?}",
        body.flag,
        body.type_,
        body.approve,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn clean_cache(
    State(state): State<OnebotState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.clean_cache();
    kovi::log::info!(
        "OneBot API: clean_cache admin={:?}",
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn can_send_image(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .can_send_image()
        .await
        .map_err(|e| OnebotError::Internal(format!("can_send_image: {e}")))?;
    Ok(Json(serde_json::json!({ "yes": resp })))
}

async fn can_send_record(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .can_send_record()
        .await
        .map_err(|e| OnebotError::Internal(format!("can_send_record: {e}")))?;
    Ok(Json(serde_json::json!({ "yes": resp })))
}

async fn cookies(
    State(state): State<OnebotState>,
    Query(query): Query<DomainQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_cookies(&query.domain)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_cookies: {e}")))?;
    Ok(Json(resp.data))
}

async fn csrf_token(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_csrf_token()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_csrf_token: {e}")))?;
    Ok(Json(resp.data))
}

async fn credentials(
    State(state): State<OnebotState>,
    Query(query): Query<DomainQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_credentials(&query.domain)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_credentials: {e}")))?;
    Ok(Json(resp.data))
}

async fn record(
    State(state): State<OnebotState>,
    Query(query): Query<GetRecordQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_record(&query.file, &query.out_format)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_record: {e}")))?;
    Ok(Json(resp.data))
}

async fn image(
    State(state): State<OnebotState>,
    Query(query): Query<GetImageQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_image(&query.file)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_image: {e}")))?;
    Ok(Json(resp.data))
}

async fn delete_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    state.bot.delete_msg(body.message_id);
    kovi::log::info!(
        "OneBot API: delete_msg message_id={} admin={:?}",
        body.message_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_forward_msg(
    State(state): State<OnebotState>,
    Query(query): Query<ForwardMsgQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_forward_msg(&query.id)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_forward_msg: {e}")))?;
    Ok(Json(resp.data))
}

async fn group_honor_info(
    State(state): State<OnebotState>,
    Query(query): Query<GroupHonorQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let honor_type_str = query.honor_type.as_deref().unwrap_or("all");
    let resp = state
        .bot
        .send_api_return("get_group_honor_info", serde_json::json!({
            "group_id": query.group_id,
            "type": honor_type_str,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_honor_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn status(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_status()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_status: {e}")))?;
    Ok(Json(resp.data))
}

async fn version_info(State(state): State<OnebotState>) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_version_info()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_version_info: {e}")))?;
    Ok(Json(resp.data))
}

// ── NapCat extended query structs ──

#[derive(Deserialize)]
struct GroupMsgHistoryQuery {
    group_id: i64,
    #[serde(default)]
    message_seq: Option<i64>,
    #[serde(default)]
    count: Option<u32>,
    #[serde(default)]
    reverse: Option<bool>,
}

#[derive(Deserialize)]
struct GroupFileSystemInfoQuery {
    group_id: i64,
}

#[derive(Deserialize)]
struct GroupRootFilesQuery {
    group_id: i64,
}

#[derive(Deserialize)]
struct GroupFilesByFolderQuery {
    group_id: i64,
    folder_id: String,
}

#[derive(Deserialize)]
struct GroupFileUrlQuery {
    group_id: i64,
    file_id: String,
}

#[derive(Deserialize)]
struct GetFileQuery {
    #[serde(default)]
    file_id: Option<String>,
    #[serde(default)]
    file: Option<String>,
}

#[derive(Deserialize)]
struct GroupAtAllRemainQuery {
    group_id: i64,
}

#[derive(Deserialize)]
struct EssenceMsgListQuery {
    group_id: i64,
}

// ── NapCat extended body structs ──

#[derive(Deserialize)]
struct DownloadFileBody {
    url: String,
    #[serde(default)]
    thread_cnt: Option<u32>,
    #[serde(default)]
    headers: Option<String>,
}

#[derive(Deserialize)]
struct UploadGroupFileBody {
    group_id: i64,
    file: String,
    name: String,
    #[serde(default)]
    folder: Option<String>,
}

#[derive(Deserialize)]
struct UploadPrivateFileBody {
    user_id: i64,
    file: String,
    name: String,
}

#[derive(Deserialize)]
struct DeleteGroupFileBody {
    group_id: i64,
    file_id: String,
    #[serde(default)]
    busid: Option<i64>,
}

#[derive(Deserialize)]
struct CreateGroupFileFolderBody {
    group_id: i64,
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Deserialize)]
struct DeleteGroupFolderBody {
    group_id: i64,
    folder_id: String,
}

#[derive(Deserialize)]
struct SetEssenceMsgBody {
    message_id: i32,
}

#[derive(Deserialize)]
struct DeleteEssenceMsgBody {
    message_id: i32,
}

#[derive(Deserialize)]
struct SendGroupForwardMsgBody {
    group_id: i64,
    messages: serde_json::Value,
}

// ── NapCat extended handlers ──

async fn get_group_msg_history(
    State(state): State<OnebotState>,
    Query(query): Query<GroupMsgHistoryQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let mut params = serde_json::json!({"group_id": query.group_id});
    if let Some(seq) = query.message_seq {
        params["message_seq"] = seq.into();
    }
    if let Some(count) = query.count {
        params["count"] = count.into();
    }
    if let Some(reverse) = query.reverse {
        params["reverse"] = reverse.into();
    }
    let resp = state
        .bot
        .send_api_return("get_group_msg_history", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_msg_history: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_group_file_system_info(
    State(state): State<OnebotState>,
    Query(query): Query<GroupFileSystemInfoQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_group_file_system_info", serde_json::json!({"group_id": query.group_id}))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_file_system_info: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_group_root_files(
    State(state): State<OnebotState>,
    Query(query): Query<GroupRootFilesQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_group_root_files", serde_json::json!({"group_id": query.group_id}))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_root_files: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_group_files_by_folder(
    State(state): State<OnebotState>,
    Query(query): Query<GroupFilesByFolderQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_group_files_by_folder", serde_json::json!({
            "group_id": query.group_id,
            "folder_id": query.folder_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_files_by_folder: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_group_file_url(
    State(state): State<OnebotState>,
    Query(query): Query<GroupFileUrlQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_group_file_url", serde_json::json!({
            "group_id": query.group_id,
            "file_id": query.file_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_file_url: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_file(
    State(state): State<OnebotState>,
    Query(query): Query<GetFileQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let mut params = serde_json::json!({});
    if let Some(file_id) = query.file_id {
        params["file_id"] = file_id.into();
    }
    if let Some(file) = query.file {
        params["file"] = file.into();
    }
    let resp = state
        .bot
        .send_api_return("get_file", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("get_file: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_group_at_all_remain(
    State(state): State<OnebotState>,
    Query(query): Query<GroupAtAllRemainQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_group_at_all_remain", serde_json::json!({"group_id": query.group_id}))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_at_all_remain: {e}")))?;
    Ok(Json(resp.data))
}

async fn get_essence_msg_list(
    State(state): State<OnebotState>,
    Query(query): Query<EssenceMsgListQuery>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .send_api_return("get_essence_msg_list", serde_json::json!({"group_id": query.group_id}))
        .await
        .map_err(|e| OnebotError::Internal(format!("get_essence_msg_list: {e}")))?;
    Ok(Json(resp.data))
}

async fn download_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DownloadFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let mut params = serde_json::json!({"url": body.url});
    if let Some(cnt) = body.thread_cnt {
        params["thread_cnt"] = cnt.into();
    }
    if let Some(hdrs) = body.headers {
        params["headers"] = hdrs.into();
    }
    let resp = state
        .bot
        .send_api_return("download_file", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("download_file: {e}")))?;
    kovi::log::info!(
        "OneBot API: download_file url={} admin={:?}",
        body.url,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn upload_group_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<UploadGroupFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let mut params = serde_json::json!({
        "group_id": body.group_id,
        "file": body.file,
        "name": body.name,
    });
    if let Some(folder) = body.folder {
        params["folder"] = folder.into();
    }
    let resp = state
        .bot
        .send_api_return("upload_group_file", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("upload_group_file: {e}")))?;
    kovi::log::info!(
        "OneBot API: upload_group_file group_id={} name={} admin={:?}",
        body.group_id,
        body.name,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn upload_private_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<UploadPrivateFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let resp = state
        .bot
        .send_api_return("upload_private_file", serde_json::json!({
            "user_id": body.user_id,
            "file": body.file,
            "name": body.name,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("upload_private_file: {e}")))?;
    kovi::log::info!(
        "OneBot API: upload_private_file user_id={} name={} admin={:?}",
        body.user_id,
        body.name,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn delete_group_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteGroupFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let mut params = serde_json::json!({
        "group_id": body.group_id,
        "file_id": body.file_id,
    });
    if let Some(busid) = body.busid {
        params["busid"] = busid.into();
    }
    let resp = state
        .bot
        .send_api_return("delete_group_file", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("delete_group_file: {e}")))?;
    kovi::log::info!(
        "OneBot API: delete_group_file group_id={} file_id={} admin={:?}",
        body.group_id,
        body.file_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn create_group_file_folder(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<CreateGroupFileFolderBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let mut params = serde_json::json!({
        "group_id": body.group_id,
        "name": body.name,
    });
    if let Some(parent_id) = body.parent_id {
        params["parent_id"] = parent_id.into();
    }
    let resp = state
        .bot
        .send_api_return("create_group_file_folder", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("create_group_file_folder: {e}")))?;
    kovi::log::info!(
        "OneBot API: create_group_file_folder group_id={} name={} admin={:?}",
        body.group_id,
        body.name,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn delete_group_folder(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteGroupFolderBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let resp = state
        .bot
        .send_api_return("delete_group_folder", serde_json::json!({
            "group_id": body.group_id,
            "folder_id": body.folder_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("delete_group_folder: {e}")))?;
    kovi::log::info!(
        "OneBot API: delete_group_folder group_id={} folder_id={} admin={:?}",
        body.group_id,
        body.folder_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn set_essence_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetEssenceMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let resp = state
        .bot
        .send_api_return("set_essence_msg", serde_json::json!({
            "message_id": body.message_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("set_essence_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: set_essence_msg message_id={} admin={:?}",
        body.message_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn delete_essence_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteEssenceMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let resp = state
        .bot
        .send_api_return("delete_essence_msg", serde_json::json!({
            "message_id": body.message_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("delete_essence_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: delete_essence_msg message_id={} admin={:?}",
        body.message_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

async fn send_group_forward_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendGroupForwardMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state.admin_ids)?;
    let resp = state
        .bot
        .send_api_return("send_group_forward_msg", serde_json::json!({
            "group_id": body.group_id,
            "messages": body.messages,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("send_group_forward_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_group_forward_msg group_id={} admin={:?}",
        body.group_id,
        headers.get("X-Admin-Id").and_then(|v| v.to_str().ok())
    );
    Ok(Json(resp.data))
}

use serde::Serialize;