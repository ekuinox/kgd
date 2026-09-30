//! ブラウザで位置ログを見るビューアのコントローラ。
//!
//! OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。ログインは無く、
//! 送信元の許可リストと Cloudflare 経由の印で守る (ADR-0013)。

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{any, get},
};
use ipnet::IpNet;

use kgd_application::BrowseLocationHistoryUseCase;

mod api;
mod dto;
mod guard;
mod presenter;

#[cfg(test)]
mod tests;

/// ビューアの設定。
#[derive(Debug, Clone)]
pub struct ViewerSettings {
    /// ビューアに届いてよい送信元
    pub allowed_cidrs: Vec<IpNet>,
}

/// ビューアのルータを組み立てる。
///
/// ガードはこのルータのルートにだけかかる。OwnTracks のルータへ merge しても、
/// `/pub` と `/healthz` には影響しない。送信元を取るため、サーバーは
/// `into_make_service_with_connect_info::<SocketAddr>()` で起動すること。
pub fn viewer_router(
    use_case: Arc<BrowseLocationHistoryUseCase>,
    settings: ViewerSettings,
) -> Router {
    let allowed: Arc<[IpNet]> = settings.allowed_cidrs.into();
    Router::new()
        .route("/viewer/api/history", get(api::handle_history))
        .route("/viewer/api/{*rest}", any(api::handle_not_found))
        .with_state(use_case)
        .layer(middleware::from_fn_with_state(allowed, guard::guard))
}
