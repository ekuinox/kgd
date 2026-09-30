//! `/viewer/api/` 以下のハンドラ。

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

use kgd_application::BrowseLocationHistoryUseCase;

use super::{
    dto::{ErrorResponse, HistoryQuery},
    presenter::present_history,
};

/// 一度に選べる期間の上限 (日数)。
///
/// 期間の長さは原則として制限しないが、年の桁を誤った URL で数十万日ぶんの
/// 日ごとの集計を作らないよう、10 年を超える範囲だけは拒否する。
const MAX_RANGE_DAYS: i64 = 3660;

/// `GET /viewer/api/history`。期間の集計と地図用の軌跡を返す。
pub(super) async fn handle_history(
    State(use_case): State<Arc<BrowseLocationHistoryUseCase>>,
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(rejection) => return error_response(StatusCode::BAD_REQUEST, rejection.body_text()),
    };
    if query.from > query.to {
        return error_response(
            StatusCode::BAD_REQUEST,
            "from must not be after to".to_string(),
        );
    }
    if (query.to - query.from).num_days() >= MAX_RANGE_DAYS {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!("the range must be at most {MAX_RANGE_DAYS} days"),
        );
    }

    match use_case.browse(query.from, query.to).await {
        Ok(history) => Json(present_history(&history)).into_response(),
        Err(error) => {
            error!(?error, from = %query.from, to = %query.to, "Failed to browse location history");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        }
    }
}

/// `/viewer/api/` 以下の未知のパス。
pub(super) async fn handle_not_found() -> Response {
    not_found()
}

/// JSON の 404 を返す。
pub(super) fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, "not found".to_string())
}

/// エラーの応答を作る。
fn error_response(status: StatusCode, message: String) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}
