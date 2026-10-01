//! ブラウザで位置ログを見るビューアのコントローラ。
//!
//! OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。ログインは無く、
//! Cloudflare 経由の印、送信元の許可リスト、Host の許可リストで守る (ADR-0013)。

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{any, get},
};
use ipnet::IpNet;

use kgd_application::BrowseLocationHistoryUseCase;

mod api;
mod assets;
mod dto;
mod guard;
mod presenter;

#[cfg(test)]
mod tests;

pub use self::guard::is_valid_allowed_host;

/// ビューアの設定。
#[derive(Debug, Clone)]
pub struct ViewerSettings {
    /// ビューアに届いてよい送信元
    pub allowed_cidrs: Vec<IpNet>,
    /// IP アドレス以外で Host に来てよい名前 (DNS rebinding を防ぐ)
    pub allowed_hosts: Vec<String>,
}

/// ビューアのルータを組み立てる。
///
/// ガードはこのルータに登録したルート (`/viewer` および `/viewer/...`) にだけかかる。
/// `.route_layer` を使うため、フォールバック (未登録のパス全般) には影響しない。OwnTracks のルータへ
/// merge すると、そのルータのフォールバック (通常は素の 404) が全体のフォールバックに
/// なるが、ここにガードはかからない。送信元を取るため、サーバーは
/// `into_make_service_with_connect_info::<SocketAddr>()` で起動すること。
pub fn viewer_router(
    use_case: Arc<BrowseLocationHistoryUseCase>,
    settings: ViewerSettings,
) -> Router {
    let allowlists = Arc::new(guard::Allowlists {
        cidrs: settings.allowed_cidrs,
        hosts: settings.allowed_hosts,
    });
    Router::new()
        .route("/viewer", get(assets::redirect_to_index))
        .route("/viewer/", get(assets::handle_index))
        .route("/viewer/{*path}", get(assets::handle_asset))
        .route("/viewer/api/calendar", get(api::handle_calendar))
        .route("/viewer/api/history", get(api::handle_history))
        // `{*rest}` は空の残りに当たらないため、`/viewer/api` と `/viewer/api/` も別に受ける
        .route("/viewer/api", any(api::handle_not_found))
        .route("/viewer/api/", any(api::handle_not_found))
        .route("/viewer/api/{*rest}", any(api::handle_not_found))
        .with_state(use_case)
        .route_layer(middleware::from_fn_with_state(allowlists, guard::guard))
}
