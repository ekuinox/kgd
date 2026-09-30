//! ビューアに届いたリクエストを、送信元と Cloudflare 経由の印で通すかどうか決める。

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use tracing::warn;

/// Cloudflare のエッジが付けるヘッダのうち、あるだけで Cloudflare 経由とみなすもの。
///
/// エッジが付与するため、インターネット側の利用者には取り除けない。
const CLOUDFLARE_HEADERS: [&str; 2] = ["cf-connecting-ip", "cf-ray"];

/// リクエストを拒否した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Denial {
    /// Cloudflare を経由して届いた
    ViaCloudflare,
    /// 送信元が許可リストに無い
    OutsideAllowlist,
}

/// Cloudflare を経由したことを示すヘッダがあるかを返す。
///
/// `Cf-Connecting-IP` か `Cf-Ray` があるか、`Cdn-Loop` に `cloudflare` が含まれていれば true。
pub(super) fn has_cloudflare_marks(headers: &HeaderMap) -> bool {
    CLOUDFLARE_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name))
        || headers.get_all("cdn-loop").iter().any(|value| {
            value
                .to_str()
                .is_ok_and(|value| value.to_ascii_lowercase().contains("cloudflare"))
        })
}

/// 送信元と Cloudflare 経由の印から、リクエストを通すかどうかを決める。
///
/// Cloudflare 経由なら送信元によらず拒否する。送信元は IPv4 射影アドレスを IPv4 に
/// 戻してから許可リストと照合する。
pub(super) fn decide_access(
    peer: IpAddr,
    via_cloudflare: bool,
    allowed: &[IpNet],
) -> Result<(), Denial> {
    if via_cloudflare {
        return Err(Denial::ViaCloudflare);
    }
    let peer = peer.to_canonical();
    if allowed.iter().any(|net| net.contains(&peer)) {
        Ok(())
    } else {
        Err(Denial::OutsideAllowlist)
    }
}

/// ビューアのルートにかけるミドルウェア。拒否したら 403 を返し、理由を warn のログに残す。
///
/// 送信元はソケットの相手アドレスだけを使い、`X-Forwarded-For` などのヘッダは見ない。
pub(super) async fn guard(
    State(allowed): State<Arc<[IpNet]>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let via_cloudflare = has_cloudflare_marks(request.headers());
    match decide_access(peer.ip(), via_cloudflare, &allowed) {
        Ok(()) => next.run(request).await,
        Err(denial) => {
            warn!(
                %peer,
                ?denial,
                path = %request.uri().path(),
                "Rejected location viewer request"
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn lan() -> Vec<IpNet> {
        ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
            .iter()
            .map(|net| net.parse().unwrap())
            .collect()
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    /// 送信元と Cloudflare の印の組み合わせごとに、通すか拒否の理由が決まることを確認する。
    ///
    /// loopback は既定の許可リストに無いため、同じホストの cloudflared から届くリクエストは拒否される。
    /// IPv4 射影アドレスは IPv4 に戻してから照合する (`[::]` で待ち受けたとき LAN の端末がこの形で届くため)。
    #[test]
    fn decide_access_follows_the_allowlist_and_cloudflare_marks() {
        let cases = [
            ("192.168.1.10", false, Ok(())),
            ("10.1.2.3", false, Ok(())),
            ("fd12::1", false, Ok(())),
            ("::ffff:192.168.1.10", false, Ok(())),
            ("127.0.0.1", false, Err(Denial::OutsideAllowlist)),
            ("::1", false, Err(Denial::OutsideAllowlist)),
            ("203.0.113.5", false, Err(Denial::OutsideAllowlist)),
            ("192.168.1.10", true, Err(Denial::ViaCloudflare)),
            ("127.0.0.1", true, Err(Denial::ViaCloudflare)),
        ];

        for (peer, via_cloudflare, expected) in cases {
            assert_eq!(
                decide_access(ip(peer), via_cloudflare, &lan()),
                expected,
                "peer = {peer}, via_cloudflare = {via_cloudflare}"
            );
        }
    }

    /// Cloudflare のエッジが付けるヘッダのどれか 1 つでもあれば、Cloudflare 経由とみなすことを確認する。
    #[test]
    fn has_cloudflare_marks_detects_each_header() {
        let marked = [
            ("cf-connecting-ip", "203.0.113.5"),
            ("cf-ray", "8c1f2a3b4c5d6e7f-NRT"),
            ("cdn-loop", "cloudflare; loops=1"),
            ("cdn-loop", "Cloudflare"),
        ];
        for (name, value) in marked {
            let mut headers = HeaderMap::new();
            headers.insert(name, HeaderValue::from_static(value));
            assert!(has_cloudflare_marks(&headers), "{name}: {value}");
        }
    }

    /// Cloudflare と関係の無いヘッダだけなら、Cloudflare 経由とみなさないことを確認する。
    #[test]
    fn has_cloudflare_marks_ignores_other_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.5"));
        headers.insert("cdn-loop", HeaderValue::from_static("fastly"));

        assert!(!has_cloudflare_marks(&headers));
    }
}
