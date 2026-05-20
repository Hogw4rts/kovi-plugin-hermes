mod handlers;
mod types;

use axum::http::{Method, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use kovi::RuntimeBot;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::secret::SecretString;
use handlers::*;
use types::OnebotError;

const API_RATE_LIMIT_RPM: u32 = 120;
const API_RATE_WINDOW_SECS: u64 = 60;

#[derive(Clone)]
pub(crate) struct OnebotState {
    pub(crate) bot: Arc<RuntimeBot>,
    pub(crate) api_key: SecretString,
    pub(crate) admin_key: SecretString,
    pub(crate) allowed_origins: Vec<String>,
    rate_limiters: Arc<RwLock<HashMap<String, RateEntry>>>,
}

impl OnebotState {
    pub(crate) fn new(
        bot: Arc<RuntimeBot>,
        api_key: SecretString,
        admin_key: SecretString,
        allowed_origins: Vec<String>,
    ) -> Self {
        Self {
            bot,
            api_key,
            admin_key,
            allowed_origins,
            rate_limiters: Default::default(),
        }
    }
}

struct RateEntry {
    count: u32,
    window_start: Instant,
}

pub(crate) async fn start(
    state: OnebotState,
    bind_addr: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let origins: Vec<axum::http::HeaderValue> = if state.allowed_origins.is_empty() {
        vec![axum::http::HeaderValue::from_static("http://127.0.0.1")]
    } else {
        state
            .allowed_origins
            .iter()
            .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
            .collect()
    };

    if origins.is_empty() {
        kovi::log::warn!("hermes: no valid CORS origins parsed from config, browser requests will be rejected");
    }

    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
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
            rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state);

    let addr = format!("{bind_addr}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    kovi::log::info!("hermes: OneBot API listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn router() -> Router<OnebotState> {
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
        .route("/onebot/get_group_msg_history", get(get_group_msg_history))
        .route("/onebot/get_group_file_system_info", get(get_group_file_system_info))
        .route("/onebot/get_group_root_files", get(get_group_root_files))
        .route("/onebot/get_group_files_by_folder", get(get_group_files_by_folder))
        .route("/onebot/get_group_file_url", get(get_group_file_url))
        .route("/onebot/get_file", get(get_file))
        .route("/onebot/get_group_at_all_remain", get(get_group_at_all_remain))
        .route("/onebot/get_essence_msg_list", get(get_essence_msg_list))
        .route("/onebot/download_file", post(download_file))
        .route("/onebot/upload_group_file", post(upload_group_file))
        .route("/onebot/upload_private_file", post(upload_private_file))
        .route("/onebot/delete_group_file", post(delete_group_file))
        .route("/onebot/create_group_file_folder", post(create_group_file_folder))
        .route("/onebot/delete_group_folder", post(delete_group_folder))
        .route("/onebot/set_essence_msg", post(set_essence_msg))
        .route("/onebot/delete_essence_msg", post(delete_essence_msg))
        .route("/onebot/send_group_forward_msg", post(send_group_forward_msg))
}

async fn auth_middleware(
    state: axum::extract::State<OnebotState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let path = req.uri().path().to_string();

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let authorized = match auth_header {
        Some(key) => {
            use subtle::ConstantTimeEq;
            key.as_bytes().ct_eq(state.api_key.as_str().as_bytes()).into()
        }
        None => false,
    };

    if !authorized {
        kovi::log::warn!("OneBot API: unauthorized request to {path}");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    next.run(req).await
}

async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<OnebotState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let key = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    let now = Instant::now();
    let allowed = {
        let mut map = state.rate_limiters.write().await;

        if map.len() > 10000 {
            let cutoff = now - std::time::Duration::from_secs(API_RATE_WINDOW_SECS * 2);
            map.retain(|_, e| e.window_start > cutoff);
        }

        let entry = map.entry(key).or_insert(RateEntry {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start).as_secs() >= API_RATE_WINDOW_SECS {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= API_RATE_LIMIT_RPM
    };

    if !allowed {
        kovi::log::warn!("OneBot API: rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "rate limit exceeded" })),
        )
            .into_response();
    }

    next.run(req).await
}