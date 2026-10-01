//! ビューアに届いたリクエストを、Cloudflare 経由の印、送信元、Host で通すかどうか決める。

use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode, Uri, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use tracing::warn;

/// Cloudflare のエッジが付けるヘッダのうち、あるだけで Cloudflare 経由とみなすもの。
///
/// エッジが付与するため、インターネット側の利用者には取り除けない。
const CLOUDFLARE_HEADERS: [&str; 2] = ["cf-connecting-ip", "cf-ray"];

/// Host の許可リストに書ける名前かを返す。
///
/// 英数字、`-`、`_`、`.` だけからなり、空でなく、先頭のドットや連続するドットを含まない名前を
/// 受け付ける。末尾の 1 つのドットは照合のときに除くため許す。これ以外の名前 (ポートやパス、
/// `user@` や空白を含むものなど) は Host と決して一致しないため、設定の検証で弾くために使う。
pub fn is_valid_allowed_host(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// ガードが照合する許可リスト。
#[derive(Debug)]
pub(super) struct Allowlists {
    /// ビューアに届いてよい送信元
    pub(super) cidrs: Vec<IpNet>,
    /// IP アドレス以外で Host に来てよい名前
    pub(super) hosts: Vec<String>,
}

/// リクエストを拒否した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Denial {
    /// Cloudflare を経由して届いた
    ViaCloudflare,
    /// 送信元が許可リストに無い
    OutsideAllowlist,
    /// Host が無いか読めないか、IP アドレスでも許可した名前でもない
    UnknownHost,
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

/// リクエストの Host を返す。`Host` ヘッダが無ければ URI の authority を使う。
///
/// HTTP/2 では `Host` ヘッダの代わりに `:authority` が URI に入るため。`Host` ヘッダが
/// UTF-8 として読めないときは、URI に頼らず `None` を返す。
pub(super) fn request_host<'a>(headers: &'a HeaderMap, uri: &'a Uri) -> Option<&'a str> {
    match headers.get(header::HOST) {
        Some(value) => value.to_str().ok(),
        None => uri.authority().map(|authority| authority.as_str()),
    }
}

/// Host から、リクエストを通すかどうかを決める。
///
/// IP アドレスなら通し、名前なら `allowed` にあるときだけ通す。名前は大文字小文字と
/// 末尾の 1 つのドットを区別しない。Host が無いか形が不正なら拒否する。
/// DNS rebinding では攻撃者の名前が Host に入り、ブラウザが攻撃者の名前を IP アドレスの
/// 形で送ることは無いため、IP アドレスは名前の許可リストと照合しなくてよい。
pub(super) fn decide_host(host: Option<&str>, allowed: &[String]) -> Result<(), Denial> {
    let name = host.and_then(host_name).ok_or(Denial::UnknownHost)?;
    if name.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let name = normalize_name(name);
    if allowed
        .iter()
        .any(|allowed| normalize_name(allowed) == name)
    {
        Ok(())
    } else {
        Err(Denial::UnknownHost)
    }
}

/// ビューアのルートにかけるミドルウェア。拒否したら 403 を返し、理由を warn のログに残す。
///
/// Cloudflare 経由の印、送信元、Host の順に調べ、すべてを満たすときだけ通す。
/// 送信元はソケットの相手アドレスだけを使い、`X-Forwarded-For` などのヘッダは見ない。
pub(super) async fn guard(
    State(allowlists): State<Arc<Allowlists>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let via_cloudflare = has_cloudflare_marks(request.headers());
    let host = request_host(request.headers(), request.uri());
    let decision = decide_access(peer.ip(), via_cloudflare, &allowlists.cidrs)
        .and_then(|()| decide_host(host, &allowlists.hosts));
    match decision {
        Ok(()) => next.run(request).await,
        Err(denial) => {
            warn!(
                %peer,
                ?denial,
                host = host.unwrap_or("-"),
                path = %request.uri().path(),
                "Rejected location viewer request"
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}

/// Host からポートを除いた部分を返す。形が不正なら `None` を返す。
///
/// `[fd00::1]:8081` のような角括弧付きの IPv6 は、括弧を外したアドレスを返す。
fn host_name(host: &str) -> Option<&str> {
    if let Some(rest) = host.strip_prefix('[') {
        let (address, after) = rest.split_once(']')?;
        address.parse::<Ipv6Addr>().ok()?;
        return (after.is_empty() || is_port(after.strip_prefix(':')?)).then_some(address);
    }
    let name = match host.rsplit_once(':') {
        Some((name, port)) if is_port(port) => name,
        Some(_) => return None,
        None => host,
    };
    (!name.is_empty() && !name.contains([':', '@'])).then_some(name)
}

/// ポート番号として読める数字の並びかを返す。
fn is_port(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// 名前を比べられる形に揃える。末尾のドットを 1 つ除き、小文字にする。
fn normalize_name(name: &str) -> String {
    name.strip_suffix('.').unwrap_or(name).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, Uri};

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

    /// Host の形ごとに、通すか拒否するかが決まることを確認する。
    ///
    /// DNS rebinding では攻撃者の名前が Host に入るため、IP アドレスで開いたときと
    /// 許可した名前で開いたときだけを通す。名前は大文字小文字と末尾の 1 つのドットを区別しない。
    #[test]
    fn decide_host_accepts_ip_literals_and_allowed_names() {
        let allowed = ["aoi.local".to_string()];
        let cases = [
            (Some("192.168.1.5"), Ok(())),
            (Some("192.168.1.5:8081"), Ok(())),
            (Some("[fd00::1]"), Ok(())),
            (Some("[fd00::1]:8081"), Ok(())),
            (Some("aoi.local"), Ok(())),
            (Some("aoi.local:8081"), Ok(())),
            (Some("AOI.Local"), Ok(())),
            (Some("aoi.local."), Ok(())),
            (Some("aoi.local.:8081"), Ok(())),
            (Some("attacker.example"), Err(Denial::UnknownHost)),
            (Some("attacker.example:8081"), Err(Denial::UnknownHost)),
            (Some("aoi.local.."), Err(Denial::UnknownHost)),
            (Some("aoi.local:"), Err(Denial::UnknownHost)),
            (Some("aoi.local:80:81"), Err(Denial::UnknownHost)),
            (Some("[aoi.local]:8081"), Err(Denial::UnknownHost)),
            (Some("[fd00::1]x"), Err(Denial::UnknownHost)),
            (Some("user@aoi.local"), Err(Denial::UnknownHost)),
            (Some(""), Err(Denial::UnknownHost)),
            (Some(":8081"), Err(Denial::UnknownHost)),
            (None, Err(Denial::UnknownHost)),
        ];

        for (host, expected) in cases {
            assert_eq!(decide_host(host, &allowed), expected, "host = {host:?}");
        }
    }

    /// 許可リストに書ける名前は、その名前で開いたときに必ず通ることを確認する。
    ///
    /// 検証を通るのに決して一致しない名前があると、起動はできるのに名前で開くと
    /// 常に 403 になり、設定の誤りに気づけないため。
    #[test]
    fn valid_allowed_hosts_always_match_their_own_host() {
        for name in [
            "aoi.local",
            "AOI.Local",
            "aoi.local.",
            "localhost",
            "my_host-1",
        ] {
            assert!(is_valid_allowed_host(name), "{name}");
            let allowed = [name.to_string()];
            assert_eq!(decide_host(Some(name), &allowed), Ok(()), "{name}");
            assert_eq!(
                decide_host(Some(&format!("{name}:8081")), &allowed),
                Ok(()),
                "{name}:8081"
            );
        }
    }

    /// Host と決して一致しない名前は、許可リストに書けないことを確認する。
    #[test]
    fn is_valid_allowed_host_rejects_names_that_never_match() {
        for name in [
            "",
            ".",
            ".aoi.local",
            "aoi..local",
            "aoi.local..",
            "aoi.local:8081",
            "http://aoi.local",
            "aoi.local/viewer",
            "me@aoi.local",
            " aoi.local",
            "aoi.local ",
            "[fd00::1]",
        ] {
            assert!(!is_valid_allowed_host(name), "{name:?}");
        }
    }

    /// 許可リストの名前も、大文字小文字と末尾のドットを揃えてから比べることを確認する。
    #[test]
    fn decide_host_normalizes_allowed_names() {
        let allowed = ["Aoi.Local.".to_string()];

        assert_eq!(decide_host(Some("aoi.local:8081"), &allowed), Ok(()));
    }

    /// 許可リストが空なら、名前では通さず IP アドレスだけを通すことを確認する。
    #[test]
    fn decide_host_rejects_every_name_without_allowed_hosts() {
        assert_eq!(
            decide_host(Some("aoi.local"), &[]),
            Err(Denial::UnknownHost)
        );
        assert_eq!(decide_host(Some("10.0.0.2:8081"), &[]), Ok(()));
    }

    /// Host ヘッダがあればそれを、無ければ URI の authority を使うことを確認する。
    ///
    /// HTTP/2 では Host ヘッダの代わりに `:authority` が URI に入るため。
    #[test]
    fn request_host_prefers_header_and_falls_back_to_authority() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("aoi.local:8081"));
        let absolute: Uri = "http://192.168.1.5:8081/viewer/".parse().unwrap();
        let relative: Uri = "/viewer/".parse().unwrap();

        assert_eq!(request_host(&headers, &absolute), Some("aoi.local:8081"));
        assert_eq!(
            request_host(&HeaderMap::new(), &absolute),
            Some("192.168.1.5:8081")
        );
        assert_eq!(request_host(&HeaderMap::new(), &relative), None);
    }

    /// Host ヘッダが UTF-8 として読めなければ、URI に頼らず不明とすることを確認する。
    #[test]
    fn request_host_returns_none_for_unreadable_header() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_bytes(b"\xffaoi").unwrap());
        let absolute: Uri = "http://192.168.1.5:8081/viewer/".parse().unwrap();

        assert_eq!(request_host(&headers, &absolute), None);
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
