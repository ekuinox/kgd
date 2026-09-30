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

/// ビューアのルータを作る。許可リストは 192.168.0.0/16 だけにする。
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

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
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
    let owntracks = owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(
            StubLocationRepository::with_points(Vec::new()),
        ))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    );
    let router = owntracks.merge(viewer(StubLocationRepository::with_points(Vec::new())));
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
