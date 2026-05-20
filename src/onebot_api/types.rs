use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
#[allow(dead_code)]
pub(crate) enum WriteMode {
    Disabled,
    Confirm,
    Direct,
}

#[allow(dead_code)]
pub(crate) enum OnebotError {
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
pub(crate) struct GroupMemberQuery {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) no_cache: bool,
}

#[derive(Deserialize)]
pub(crate) struct UserIdQuery {
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) no_cache: bool,
}

#[derive(Deserialize)]
pub(crate) struct MsgIdQuery {
    pub(crate) message_id: i32,
}

#[derive(Deserialize)]
pub(crate) struct ForwardMsgQuery {
    pub(crate) id: String,
}

#[derive(Deserialize)]
pub(crate) struct GroupHonorQuery {
    pub(crate) group_id: i64,
    #[serde(default)]
    pub(crate) honor_type: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GroupMemberListQuery {
    pub(crate) group_id: i64,
    #[serde(default = "default_limit")]
    pub(crate) limit: u32,
    #[serde(default)]
    pub(crate) offset: u32,
}

#[derive(Deserialize)]
pub(crate) struct GroupInfoQuery {
    pub(crate) group_id: i64,
    #[serde(default)]
    pub(crate) no_cache: bool,
}

fn default_limit() -> u32 {
    100
}

pub(crate) const MAX_MEMBER_LIMIT: u32 = 200;

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum MessageInput {
    Text(String),
    Segments(Vec<SegmentInput>),
}

#[derive(Deserialize)]
pub(crate) struct SegmentInput {
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) data: serde_json::Value,
}

impl MessageInput {
    pub(crate) fn to_message(&self) -> kovi::Message {
        match self {
            MessageInput::Text(text) => kovi::Message::new().add_text(text),
            MessageInput::Segments(segments) => {
                let mut msg = kovi::Message::new();
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
pub(crate) struct SendGroupMsgBody {
    pub(crate) group_id: i64,
    pub(crate) message: MessageInput,
}

#[derive(Deserialize)]
pub(crate) struct SendPrivateMsgBody {
    pub(crate) user_id: i64,
    pub(crate) message: MessageInput,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupBanBody {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) duration: u64,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupKickBody {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) reject_add_request: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupWholeBanBody {
    pub(crate) group_id: i64,
    pub(crate) enable: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupAdminBody {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    pub(crate) enable: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupAnonymousBody {
    pub(crate) group_id: i64,
    pub(crate) enable: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupAnonymousBanBody {
    pub(crate) group_id: i64,
    #[serde(default)]
    pub(crate) anonymous: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) flag: Option<String>,
    #[serde(default)]
    pub(crate) duration: u64,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupCardBody {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) card: String,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupNameBody {
    pub(crate) group_id: i64,
    pub(crate) group_name: String,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupLeaveBody {
    pub(crate) group_id: i64,
    #[serde(default)]
    pub(crate) is_dismiss: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupSpecialTitleBody {
    pub(crate) group_id: i64,
    pub(crate) user_id: i64,
    #[serde(default)]
    pub(crate) special_title: String,
}

#[derive(Deserialize)]
pub(crate) struct SendLikeBody {
    pub(crate) user_id: i64,
    pub(crate) times: u32,
}

#[derive(Deserialize)]
pub(crate) struct SetFriendAddRequestBody {
    pub(crate) flag: String,
    #[serde(default)]
    pub(crate) approve: bool,
    #[serde(default)]
    pub(crate) remark: String,
}

#[derive(Deserialize)]
pub(crate) struct SetGroupAddRequestBody {
    pub(crate) flag: String,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    #[serde(default)]
    pub(crate) approve: bool,
    #[serde(default)]
    pub(crate) reason: String,
}

#[derive(Deserialize)]
pub(crate) struct DeleteMsgBody {
    pub(crate) message_id: i32,
}

#[derive(Deserialize)]
pub(crate) struct DomainQuery {
    pub(crate) domain: String,
}

#[derive(Deserialize)]
pub(crate) struct GetRecordQuery {
    pub(crate) file: String,
    pub(crate) out_format: String,
}

#[derive(Deserialize)]
pub(crate) struct GetImageQuery {
    pub(crate) file: String,
}

#[derive(Deserialize)]
pub(crate) struct GroupMsgHistoryQuery {
    pub(crate) group_id: i64,
    #[serde(default)]
    pub(crate) message_seq: Option<i64>,
    #[serde(default)]
    pub(crate) count: Option<u32>,
    #[serde(default)]
    pub(crate) reverse: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct GroupFileSystemInfoQuery {
    pub(crate) group_id: i64,
}

#[derive(Deserialize)]
pub(crate) struct GroupRootFilesQuery {
    pub(crate) group_id: i64,
}

#[derive(Deserialize)]
pub(crate) struct GroupFilesByFolderQuery {
    pub(crate) group_id: i64,
    pub(crate) folder_id: String,
}

#[derive(Deserialize)]
pub(crate) struct GroupFileUrlQuery {
    pub(crate) group_id: i64,
    pub(crate) file_id: String,
}

#[derive(Deserialize)]
pub(crate) struct GetFileQuery {
    #[serde(default)]
    pub(crate) file_id: Option<String>,
    #[serde(default)]
    pub(crate) file: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GroupAtAllRemainQuery {
    pub(crate) group_id: i64,
}

#[derive(Deserialize)]
pub(crate) struct EssenceMsgListQuery {
    pub(crate) group_id: i64,
}

#[derive(Deserialize)]
pub(crate) struct DownloadFileBody {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) thread_cnt: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct UploadGroupFileBody {
    pub(crate) group_id: i64,
    pub(crate) file: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) folder: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UploadPrivateFileBody {
    pub(crate) user_id: i64,
    pub(crate) file: String,
    pub(crate) name: String,
}

#[derive(Deserialize)]
pub(crate) struct DeleteGroupFileBody {
    pub(crate) group_id: i64,
    pub(crate) file_id: String,
    #[serde(default)]
    pub(crate) busid: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct CreateGroupFileFolderBody {
    pub(crate) group_id: i64,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) parent_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DeleteGroupFolderBody {
    pub(crate) group_id: i64,
    pub(crate) folder_id: String,
}

#[derive(Deserialize)]
pub(crate) struct SetEssenceMsgBody {
    pub(crate) message_id: i32,
}

#[derive(Deserialize)]
pub(crate) struct DeleteEssenceMsgBody {
    pub(crate) message_id: i32,
}

#[derive(Deserialize)]
pub(crate) struct SendGroupForwardMsgBody {
    pub(crate) group_id: i64,
    pub(crate) messages: serde_json::Value,
}