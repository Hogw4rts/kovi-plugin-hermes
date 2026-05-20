use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use kovi::bot::runtimebot::CanSendApi;

use super::types::*;
use super::{OnebotState, OnebotError};

pub(crate) fn require_admin(headers: &HeaderMap, state: &OnebotState) -> Result<(), OnebotError> {
    let admin_key = headers
        .get("X-Admin-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !admin_key.is_empty()
        && !state.admin_key.is_empty()
        && subtle::ConstantTimeEq::ct_eq(
            admin_key.as_bytes(),
            state.admin_key.as_str().as_bytes(),
        )
        .into()
    {
        return Ok(());
    }

    Err(OnebotError::Forbidden("unauthorized".to_string()))
}

pub(crate) async fn login_info(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_login_info()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_login_info: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn friend_list(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_friend_list()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_friend_list: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn group_list(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_group_list()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_group_list: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn group_info(
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

pub(crate) async fn group_member_list(
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

pub(crate) async fn group_member_info(
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

pub(crate) async fn stranger_info(
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

pub(crate) async fn get_msg(
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

pub(crate) async fn send_group_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendGroupMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let msg = body.message.to_message();
    let msg_id = state
        .bot
        .send_group_msg_return(body.group_id, msg)
        .await
        .map_err(|e| OnebotError::Internal(format!("send_group_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_group_msg group_id={} message_id={}",
        body.group_id,
        msg_id
    );
    Ok(Json(serde_json::json!({ "ok": true, "message_id": msg_id })))
}

pub(crate) async fn send_private_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendPrivateMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let msg = body.message.to_message();
    let msg_id = state
        .bot
        .send_private_msg_return(body.user_id, msg)
        .await
        .map_err(|e| OnebotError::Internal(format!("send_private_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_private_msg user_id={} message_id={}",
        body.user_id,
        msg_id
    );
    Ok(Json(serde_json::json!({ "ok": true, "message_id": msg_id })))
}

pub(crate) async fn set_group_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state
        .bot
        .set_group_ban(body.group_id, body.user_id, body.duration as usize);
    kovi::log::info!(
        "OneBot API: set_group_ban group_id={} user_id={} duration={}",
        body.group_id,
        body.user_id,
        body.duration
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_whole_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupWholeBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_whole_ban(body.group_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_whole_ban group_id={} enable={}",
        body.group_id,
        body.enable
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_kick(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupKickBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state
        .bot
        .set_group_kick(body.group_id, body.user_id, body.reject_add_request);
    kovi::log::info!(
        "OneBot API: set_group_kick group_id={} user_id={}",
        body.group_id,
        body.user_id
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_admin(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAdminBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_admin(body.group_id, body.user_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_admin group_id={} user_id={} enable={}",
        body.group_id,
        body.user_id,
        body.enable
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_anonymous(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAnonymousBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_anonymous(body.group_id, body.enable);
    kovi::log::info!(
        "OneBot API: set_group_anonymous group_id={} enable={}",
        body.group_id,
        body.enable
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_anonymous_ban(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAnonymousBanBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
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
        "OneBot API: set_group_anonymous_ban group_id={} duration={}",
        body.group_id,
        body.duration
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_card(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupCardBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_card(body.group_id, body.user_id, &body.card);
    kovi::log::info!(
        "OneBot API: set_group_card group_id={} user_id={}",
        body.group_id,
        body.user_id
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_name(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupNameBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_name(body.group_id, &body.group_name);
    kovi::log::info!(
        "OneBot API: set_group_name group_id={}",
        body.group_id
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_leave(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupLeaveBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_leave(body.group_id, body.is_dismiss);
    kovi::log::info!(
        "OneBot API: set_group_leave group_id={} is_dismiss={}",
        body.group_id,
        body.is_dismiss
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_special_title(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupSpecialTitleBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_group_special_title(body.group_id, body.user_id, &body.special_title);
    kovi::log::info!(
        "OneBot API: set_group_special_title group_id={} user_id={}",
        body.group_id,
        body.user_id
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn send_like(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendLikeBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.send_like(body.user_id, body.times as usize);
    kovi::log::info!(
        "OneBot API: send_like user_id={} times={}",
        body.user_id,
        body.times
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_friend_add_request(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetFriendAddRequestBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.set_friend_add_request(&body.flag, body.approve, &body.remark);
    kovi::log::info!(
        "OneBot API: set_friend_add_request flag={} approve={}",
        body.flag,
        body.approve
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn set_group_add_request(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetGroupAddRequestBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.send_api("set_group_add_request", serde_json::json!({
        "flag": body.flag,
        "sub_type": body.type_,
        "approve": body.approve,
        "reason": body.reason,
    }));
    kovi::log::info!(
        "OneBot API: set_group_add_request flag={} type={} approve={}",
        body.flag,
        body.type_,
        body.approve
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn delete_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.delete_msg(body.message_id);
    kovi::log::info!(
        "OneBot API: delete_msg message_id={}",
        body.message_id
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn clean_cache(
    State(state): State<OnebotState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    state.bot.clean_cache();
    kovi::log::info!("OneBot API: clean_cache");
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn can_send_image(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .can_send_image()
        .await
        .map_err(|e| OnebotError::Internal(format!("can_send_image: {e}")))?;
    Ok(Json(serde_json::json!({ "yes": resp })))
}

pub(crate) async fn can_send_record(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .can_send_record()
        .await
        .map_err(|e| OnebotError::Internal(format!("can_send_record: {e}")))?;
    Ok(Json(serde_json::json!({ "yes": resp })))
}

pub(crate) async fn cookies(
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

pub(crate) async fn csrf_token(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_csrf_token()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_csrf_token: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn credentials(
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

pub(crate) async fn record(
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

pub(crate) async fn image(
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

pub(crate) async fn get_forward_msg(
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

pub(crate) async fn group_honor_info(
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

pub(crate) async fn status(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_status()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_status: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn version_info(
    State(state): State<OnebotState>,
) -> Result<impl IntoResponse, OnebotError> {
    let resp = state
        .bot
        .get_version_info()
        .await
        .map_err(|e| OnebotError::Internal(format!("get_version_info: {e}")))?;
    Ok(Json(resp.data))
}

pub(crate) async fn get_group_msg_history(
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

pub(crate) async fn get_group_file_system_info(
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

pub(crate) async fn get_group_root_files(
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

pub(crate) async fn get_group_files_by_folder(
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

pub(crate) async fn get_group_file_url(
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

pub(crate) async fn get_file(
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

pub(crate) async fn get_group_at_all_remain(
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

pub(crate) async fn get_essence_msg_list(
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

pub(crate) async fn download_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DownloadFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let mut params = serde_json::json!({"url": body.url});
    if let Some(cnt) = body.thread_cnt {
        params["thread_cnt"] = cnt.into();
    }
    let resp = state
        .bot
        .send_api_return("download_file", params)
        .await
        .map_err(|e| OnebotError::Internal(format!("download_file: {e}")))?;
    kovi::log::info!("OneBot API: download_file url={}", body.url);
    Ok(Json(resp.data))
}

pub(crate) async fn upload_group_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<UploadGroupFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
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
        "OneBot API: upload_group_file group_id={} name={}",
        body.group_id,
        body.name
    );
    Ok(Json(resp.data))
}

pub(crate) async fn upload_private_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<UploadPrivateFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
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
        "OneBot API: upload_private_file user_id={} name={}",
        body.user_id,
        body.name
    );
    Ok(Json(resp.data))
}

pub(crate) async fn delete_group_file(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteGroupFileBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
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
        "OneBot API: delete_group_file group_id={} file_id={}",
        body.group_id,
        body.file_id
    );
    Ok(Json(resp.data))
}

pub(crate) async fn create_group_file_folder(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<CreateGroupFileFolderBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
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
        "OneBot API: create_group_file_folder group_id={} name={}",
        body.group_id,
        body.name
    );
    Ok(Json(resp.data))
}

pub(crate) async fn delete_group_folder(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteGroupFolderBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let resp = state
        .bot
        .send_api_return("delete_group_folder", serde_json::json!({
            "group_id": body.group_id,
            "folder_id": body.folder_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("delete_group_folder: {e}")))?;
    kovi::log::info!(
        "OneBot API: delete_group_folder group_id={} folder_id={}",
        body.group_id,
        body.folder_id
    );
    Ok(Json(resp.data))
}

pub(crate) async fn set_essence_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SetEssenceMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let resp = state
        .bot
        .send_api_return("set_essence_msg", serde_json::json!({
            "message_id": body.message_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("set_essence_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: set_essence_msg message_id={}",
        body.message_id
    );
    Ok(Json(resp.data))
}

pub(crate) async fn delete_essence_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<DeleteEssenceMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let resp = state
        .bot
        .send_api_return("delete_essence_msg", serde_json::json!({
            "message_id": body.message_id,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("delete_essence_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: delete_essence_msg message_id={}",
        body.message_id
    );
    Ok(Json(resp.data))
}

pub(crate) async fn send_group_forward_msg(
    State(state): State<OnebotState>,
    headers: HeaderMap,
    Json(body): Json<SendGroupForwardMsgBody>,
) -> Result<impl IntoResponse, OnebotError> {
    require_admin(&headers, &state)?;
    let resp = state
        .bot
        .send_api_return("send_group_forward_msg", serde_json::json!({
            "group_id": body.group_id,
            "messages": body.messages,
        }))
        .await
        .map_err(|e| OnebotError::Internal(format!("send_group_forward_msg: {e}")))?;
    kovi::log::info!(
        "OneBot API: send_group_forward_msg group_id={}",
        body.group_id
    );
    Ok(Json(resp.data))
}