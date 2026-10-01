//! ビューアのルータのテスト。送信元は `MockConnectInfo` で差し替える。

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone as _, Utc};
use serde_json::Value;
use tower::ServiceExt as _;

use kgd_application::{
    BrowseLocationHistoryUseCase, LocationHistorySettings, RecordLocationUseCase,
    ports::LocationRepository,
};
use kgd_domain::{OwnTracksMessage, TrackPoint};

use crate::{OwnTracksControllerSettings, owntracks_router};

use super::*;

/// 決まった点を返すか、常に失敗するリポジトリのスタブ。
///
/// kgd-application のモックは `#[cfg(test)]` で生成されるためクレート外からは使えない。
#[allow(clippy::type_complexity)] // テスト用のスタブで、型エイリアスを増やすほどではない
struct StubLocationRepository {
    /// `locations_between` が返す点
    points: Vec<TrackPoint>,
    /// `locations_between` を失敗させるかどうか
    should_fail: bool,
    /// `locations_between` に渡された範囲
    ranges: Arc<Mutex<Vec<(DateTime<Utc>, DateTime<Utc>)>>>,
}

impl StubLocationRepository {
    fn with_points(points: Vec<TrackPoint>) -> Self {
        Self {
            points,
            should_fail: false,
            ranges: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing() -> Self {
        Self {
            should_fail: true,
            ..Self::with_points(Vec::new())
        }
    }
}

#[async_trait::async_trait]
impl LocationRepository for StubLocationRepository {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        Ok(messages.len())
    }

    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>> {
        self.ranges.lock().unwrap().push((start, end));
        if self.should_fail {
            return Err(anyhow::anyhow!("stub: locations_between failed"));
        }
        Ok(self.points.clone())
    }
}

/// 2026-09-01 10:00 (Asia/Tokyo) の点を作る。
fn point() -> TrackPoint {
    TrackPoint {
        at: chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, 1, 10, 0, 0)
            .unwrap()
            .to_utc(),
        lat: 35.68,
        lon: 139.76,
        accuracy_m: Some(10),
        activity: None,
    }
}

/// テストのリクエストが既定で持つ Host。LAN の端末が IP アドレスで開いたときの形。
const LAN_HOST: &str = "192.168.1.5:8081";

/// ビューアのルータを作る。送信元の許可リストは 192.168.0.0/16、Host の許可リストは aoi.local だけにする。
fn viewer(repo: StubLocationRepository) -> Router {
    let use_case = Arc::new(BrowseLocationHistoryUseCase::new(
        Arc::new(repo),
        LocationHistorySettings {
            timezone: chrono_tz::Asia::Tokyo,
            max_accuracy_m: 200,
            max_track_points: 100,
        },
    ));
    viewer_router(
        use_case,
        ViewerSettings {
            allowed_cidrs: vec!["192.168.0.0/16".parse().unwrap()],
            allowed_hosts: vec!["aoi.local".to_string()],
        },
    )
}

/// 送信元を `peer` にして 1 件のリクエストを送る。
async fn send(
    router: Router,
    peer: &str,
    request: Request<Body>,
) -> (StatusCode, Vec<u8>, Option<String>) {
    let peer: SocketAddr = SocketAddr::new(peer.parse().unwrap(), 50000);
    let response = router
        .layer(MockConnectInfo(peer))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|value| value.to_str().unwrap().to_string());
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, body.to_vec(), content_type)
}

/// `Host` を `LAN_HOST` にした GET のリクエストを作る。
fn get(uri: &str) -> Request<Body> {
    get_with_host(uri, Some(LAN_HOST))
}

/// `Host` を指定した GET のリクエストを作る。`None` なら `Host` を付けない。
fn get_with_host(uri: &str, host: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(host) = host {
        builder = builder.header(header::HOST, host);
    }
    builder.body(Body::empty()).unwrap()
}

/// OwnTracks のルータにビューアのルータを merge した、本番と同じ形のルータを作る。
fn merged(repo: StubLocationRepository) -> Router {
    let owntracks = owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(
            StubLocationRepository::with_points(Vec::new()),
        ))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    );
    owntracks.merge(viewer(repo))
}

fn json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("body should be JSON")
}

/// LAN の送信元には、期間の集計と軌跡を JSON で返すことを確認する。
#[tokio::test]
async fn history_returns_json_to_lan_peers() {
    let router = viewer(StubLocationRepository::with_points(vec![point()]));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-02"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = json(&body);
    assert_eq!(body["range"]["from"], "2026-09-01");
    assert_eq!(body["range"]["timezone"], "Asia/Tokyo");
    assert_eq!(body["days"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"]["point_count"], 1);
    assert_eq!(body["track"]["type"], "FeatureCollection");
}

/// loopback の送信元は拒否することを確認する。
///
/// 同じホストの cloudflared はこの送信元で届くため。
#[tokio::test]
async fn history_rejects_loopback_peers() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(
        router,
        "127.0.0.1",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// LAN の送信元でも Cloudflare 経由の印があれば拒否し、リポジトリを読まないことを確認する。
#[tokio::test]
async fn history_rejects_requests_through_cloudflare() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();
    let request = Request::builder()
        .uri("/viewer/api/history?from=2026-09-01&to=2026-09-01")
        .header(header::HOST, LAN_HOST)
        .header("Cf-Connecting-IP", "203.0.113.5")
        .body(Body::empty())
        .unwrap();

    let (status, _, _) = send(viewer(repo), "192.168.1.10", request).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(ranges.lock().unwrap().is_empty());
}

/// 日付の形式が不正なら、理由を JSON で添えて 400 を返すことを確認する。
#[tokio::test]
async fn history_rejects_malformed_dates() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=not-a-date&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json(&body)["error"].is_string());
}

/// 開始日が終了日より後なら 400 を返すことを確認する。
#[tokio::test]
async fn history_rejects_reversed_ranges() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-02&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json(&body)["error"].is_string());
}

/// 10 年を超える範囲は、リポジトリを読まずに 400 を返すことを確認する。
///
/// 年の桁を誤った URL で、数十万日ぶんの日ごとの集計を作らないため。
#[tokio::test]
async fn history_rejects_ranges_longer_than_ten_years() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();

    let (status, _, _) = send(
        viewer(repo),
        "192.168.1.10",
        get("/viewer/api/history?from=0026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(ranges.lock().unwrap().is_empty());
}

/// 10 年ちょうど (3659 日の差) の範囲は受け付けることを確認する。
#[tokio::test]
async fn history_accepts_ranges_of_exactly_ten_years() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2016-01-01&to=2026-01-07"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

/// 開始日と終了日の差が 3660 日になると 400 を返すことを確認する。
#[tokio::test]
async fn history_rejects_ranges_one_day_over_ten_years() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();

    let (status, _, _) = send(
        viewer(repo),
        "192.168.1.10",
        get("/viewer/api/history?from=2016-01-01&to=2026-01-08"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(ranges.lock().unwrap().is_empty());
}

/// 年が 1 から 9999 の外にある日付は、リポジトリを読まずに 400 を返すことを確認する。
///
/// chrono で表せる端の日付は、翌日や前日の 0 時を求める段でパニックを起こすため。
#[tokio::test]
async fn history_rejects_years_out_of_range() {
    for (from, to) in [
        ("%2B262142-12-31", "%2B262142-12-31"),
        ("-262143-01-01", "-262143-01-01"),
        ("0000-12-31", "0000-12-31"),
        ("%2B10000-01-01", "%2B10000-01-01"),
    ] {
        let repo = StubLocationRepository::with_points(Vec::new());
        let ranges = repo.ranges.clone();

        let (status, body, _) = send(
            viewer(repo),
            "192.168.1.10",
            get(&format!("/viewer/api/history?from={from}&to={to}")),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{from}..{to}");
        assert!(
            json(&body)["error"].as_str().unwrap().contains("year"),
            "{from}..{to}: {}",
            String::from_utf8_lossy(&body)
        );
        assert!(ranges.lock().unwrap().is_empty(), "{from}..{to}");
    }
}

/// リポジトリが失敗したら、中身を出さずに 500 と `internal error` を返すことを確認する。
#[tokio::test]
async fn history_hides_repository_errors() {
    let router = viewer(StubLocationRepository::failing());

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(json(&body)["error"], "internal error");
}

/// `/viewer/api/` 以下の未知のパスには JSON の 404 を返すことを確認する。
///
/// HTML の 200 を返すと、画面側で原因のわかりにくいスキーマ検証のエラーになるため。
#[tokio::test]
async fn unknown_api_paths_return_json_not_found() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, content_type) =
        send(router, "192.168.1.10", get("/viewer/api/histroy")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert!(json(&body)["error"].is_string());
}

/// OwnTracks のルータと合わせても、`/pub` はこれまでどおり loopback から Basic 認証で通ることを確認する。
///
/// cloudflared は loopback から届くため、ガードを `/pub` にかけると位置の受信が止まる。
#[tokio::test]
async fn pub_still_accepts_loopback_requests_when_merged() {
    let router = merged(StubLocationRepository::with_points(Vec::new()));
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .header("Cf-Connecting-IP", "203.0.113.5")
        .body(Body::from(
            r#"{"_type":"location","tst":1,"lat":1.0,"lon":2.0}"#,
        ))
        .unwrap();

    let (status, body, _) = send(router, "127.0.0.1", request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"[]");
}

/// OwnTracks のルータと合わせたとき、未登録のパスへの loopback からのリクエストは
/// ガードにかからず、素の 404 のままであることを確認する。
///
/// `.layer` だとルータ全体のフォールバックにもガードがかかり、cloudflared が届ける
/// `/favicon.ico` のような未登録パスまで「ビューアへの拒否」として 403 になり、
/// 誤解を招く warn ログも出てしまうため。
#[tokio::test]
async fn unmatched_paths_stay_not_found_when_merged_with_owntracks() {
    let router = merged(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(router, "127.0.0.1", get("/foo")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// OwnTracks のルータと合わせた本番と同じ形でも、loopback からのビューアの API は 403 になることを確認する。
///
/// merge の後でもガードがビューアのルートにかかっていることを、本番の組み立て方で確かめるため。
#[tokio::test]
async fn history_rejects_loopback_peers_when_merged_with_owntracks() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();

    let (status, _, _) = send(
        merged(repo),
        "127.0.0.1",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(ranges.lock().unwrap().is_empty());
}

/// LAN の送信元でも、Host が許可していない名前なら拒否し、リポジトリを読まないことを確認する。
///
/// DNS rebinding では、攻撃者の名前を LAN のアドレスへ向け直し、LAN のブラウザから
/// 同じオリジンとして API を読ませる。そのとき Host には攻撃者の名前が入る。
#[tokio::test]
async fn history_rejects_unknown_host_names() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();

    let (status, _, _) = send(
        viewer(repo),
        "192.168.1.10",
        get_with_host(
            "/viewer/api/history?from=2026-09-01&to=2026-09-01",
            Some("attacker.example:8081"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(ranges.lock().unwrap().is_empty());
}

/// Host が IP アドレスか許可した名前なら、LAN の送信元に応答することを確認する。
#[tokio::test]
async fn history_accepts_ip_literal_and_allowed_hosts() {
    for host in [
        "192.168.1.5:8081",
        "[fd00::1]:8081",
        "aoi.local:8081",
        "AOI.local.",
    ] {
        let router = viewer(StubLocationRepository::with_points(Vec::new()));

        let (status, _, _) = send(
            router,
            "192.168.1.10",
            get_with_host(
                "/viewer/api/history?from=2026-09-01&to=2026-09-01",
                Some(host),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{host}");
    }
}

/// Host が無いリクエストは拒否することを確認する。
///
/// 名前を確かめられないリクエストを通すと、Host の検査を抜け道にできるため。
#[tokio::test]
async fn history_rejects_requests_without_host() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(
        router,
        "192.168.1.10",
        get_with_host("/viewer/api/history?from=2026-09-01&to=2026-09-01", None),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// `/viewer` は `/viewer/` へ転送することを確認する。
///
/// Vite の `base` が `/viewer/` のため、末尾のスラッシュが無いと相対パスの読み込みがずれる。
#[tokio::test]
async fn viewer_without_trailing_slash_redirects() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));
    let response = router
        .layer(MockConnectInfo(SocketAddr::new(
            "192.168.1.10".parse().unwrap(),
            50000,
        )))
        .oneshot(get("/viewer"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(response.headers()[header::LOCATION], "/viewer/");
}

/// 画面のパスには、ビルド済みなら index.html を、未ビルドなら案内のページを HTML で返すことを確認する。
///
/// `web/dist` の有無は実行環境によって違うため、どちらでも HTML が返ることだけを確かめる。
#[tokio::test]
async fn viewer_pages_return_html() {
    for uri in ["/viewer/", "/viewer/some/page"] {
        let router = viewer(StubLocationRepository::with_points(Vec::new()));

        let (status, _, content_type) = send(router, "192.168.1.10", get(uri)).await;

        assert!(
            status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
            "{uri}: {status}"
        );
        assert!(
            content_type.is_some_and(|value| value.starts_with("text/html")),
            "{uri}"
        );
    }
}

/// 画面のパスにもガードがかかることを確認する。
#[tokio::test]
async fn viewer_pages_reject_loopback_peers() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(router, "127.0.0.1", get("/viewer/")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// 画面を配信するルートを足しても、`/viewer/api/` 以下の未知のパスは JSON の 404 のままであることを確認する。
#[tokio::test]
async fn unknown_api_paths_are_not_served_as_pages() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, content_type) =
        send(router, "192.168.1.10", get("/viewer/api/nested/unknown")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/json"));
}
