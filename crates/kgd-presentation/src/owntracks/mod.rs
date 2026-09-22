//! OwnTracks からの HTTP リクエストを受けてユースケースを呼び出すコントローラ。

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn};

use kgd_application::RecordLocationUseCase;
use kgd_domain::sanitize_identifier;

mod auth;

#[cfg(test)]
mod tests;

use auth::is_valid_basic_auth;

/// ボディの上限 (1 MiB)。
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// OwnTracks コントローラの設定。
#[derive(Debug, Clone)]
pub struct OwnTracksControllerSettings {
    /// Basic 認証のユーザー名
    pub username: String,
    /// Basic 認証のパスワード
    pub password: String,
}

/// ハンドラ間で共有する状態。
#[derive(Clone)]
struct OwnTracksState {
    /// 受信ユースケース
    use_case: Arc<RecordLocationUseCase>,
    /// 認証設定
    settings: OwnTracksControllerSettings,
}

/// URL クエリで渡される端末識別子。
///
/// OwnTracks はヘッダではなく `?u=&d=` で名乗ることがある。
#[derive(Debug, Deserialize)]
struct DeviceQuery {
    /// ユーザー識別子
    u: Option<String>,
    /// デバイス識別子
    d: Option<String>,
}

/// OwnTracks 受信用のルータを組み立てる。
pub fn owntracks_router(
    use_case: Arc<RecordLocationUseCase>,
    settings: OwnTracksControllerSettings,
) -> Router {
    Router::new()
        .route("/pub", post(handle_pub))
        .route("/healthz", get(handle_healthz))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(OwnTracksState { use_case, settings })
}

/// 死活確認。
async fn handle_healthz() -> Json<Value> {
    Json(json!({ "ok": true }))
}

/// メッセージを受け取って保存する。
///
/// 応答は常に JSON 配列。OwnTracks は配列以外を失敗として扱うため。
async fn handle_pub(
    State(state): State<OwnTracksState>,
    Query(query): Query<DeviceQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if !is_valid_basic_auth(
        authorization,
        &state.settings.username,
        &state.settings.password,
    ) {
        warn!("Rejected unauthenticated OwnTracks request");
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"owntracks\"")],
        )
            .into_response();
    }

    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        warn!("Rejected malformed OwnTracks payload");
        return StatusCode::BAD_REQUEST.into_response();
    };

    let payloads = match value {
        Value::Array(values) => values,
        other => vec![other],
    };

    let user_id = sanitize_identifier(
        header_or_query(&headers, "X-Limit-U", query.u.as_deref()),
        "unknown",
    );
    let device_id = sanitize_identifier(
        header_or_query(&headers, "X-Limit-D", query.d.as_deref()),
        "device",
    );

    match state.use_case.record(&user_id, &device_id, payloads).await {
        Ok(outcome) => {
            info!(
                user_id,
                device_id,
                parsed = outcome.parsed,
                stored = outcome.stored,
                skipped = outcome.skipped,
                "Recorded OwnTracks messages"
            );
        }
        Err(error) => {
            // 端末側に再送させても直らないため、記録に失敗しても 200 を返す。
            // 失われるのは 1 バッチぶんで、端末のキューは次の送信で流れる。
            warn!(?error, "Failed to record OwnTracks messages");
        }
    }

    Json(Vec::<Value>::new()).into_response()
}

/// ヘッダを優先し、無ければクエリの値を使う。
fn header_or_query<'a>(
    headers: &'a HeaderMap,
    name: &str,
    fallback: Option<&'a str>,
) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .or(fallback)
}
