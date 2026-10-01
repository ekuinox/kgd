//! `/viewer/api/` 以下のハンドラ。

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

use kgd_application::{BrowseLocationHistoryError, BrowseLocationHistoryUseCase};

use super::{
    dto::{CalendarResponse, ErrorResponse, HistoryQuery},
    presenter::present_history,
};

/// `GET /viewer/api/calendar`。暦日を区切るタイムゾーンを返す。
///
/// 画面は「今日」やよく使う範囲を、ブラウザの現地ではなくこの暦で決める。
pub(super) async fn handle_calendar(
    State(use_case): State<Arc<BrowseLocationHistoryUseCase>>,
) -> Json<CalendarResponse> {
    Json(CalendarResponse {
        timezone: use_case.timezone().name().to_string(),
    })
}

/// `GET /viewer/api/history`。期間の集計と地図用の軌跡を返す。
///
/// 期間の検証はユースケースが行い、受け付けられない期間は 400 にする。
pub(super) async fn handle_history(
    State(use_case): State<Arc<BrowseLocationHistoryUseCase>>,
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(rejection) => return error_response(StatusCode::BAD_REQUEST, rejection.body_text()),
    };

    match use_case.browse(query.from, query.to).await {
        Ok(history) => Json(present_history(&history)).into_response(),
        Err(BrowseLocationHistoryError::InvalidRange(reason)) => {
            error_response(StatusCode::BAD_REQUEST, reason.to_string())
        }
        Err(BrowseLocationHistoryError::Failed(error)) => {
            error!(?error, from = %query.from, to = %query.to, "Failed to browse location history");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        }
    }
}

/// `/viewer/api/` 以下の未知のパスに JSON の 404 を返す。
pub(super) async fn handle_not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, "not found".to_string())
}

/// エラーの応答を作る。
fn error_response(status: StatusCode, message: String) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}
