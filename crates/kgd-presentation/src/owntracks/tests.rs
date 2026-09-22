use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

use anyhow::Result;
use kgd_application::{RecordLocationUseCase, ports::LocationRepository};
use kgd_domain::OwnTracksMessage;

use super::*;

/// 常に成功するリポジトリのスタブ。
///
/// kgd-application のモックは `#[cfg(test)]` で生成されるためクレート外からは
/// 参照できない。ルータの検証に必要なのは「保存が成功する」ことだけなので、
/// ここでは手書きのスタブを置く。
struct StubLocationRepository;

#[async_trait::async_trait]
impl LocationRepository for StubLocationRepository {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        Ok(messages.len())
    }
}

/// テスト用のルータを作る。保存は常に成功する。
fn test_router() -> axum::Router {
    owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(StubLocationRepository))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    )
}

/// 認証付きの POST に 200 と空の JSON 配列を返すことを確認する。
///
/// 応答が空ボディや非配列だと OwnTracks が送信失敗と解釈するため。
#[tokio::test]
async fn post_pub_returns_empty_json_array() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .header("X-Limit-U", "ekuinox")
        .header("X-Limit-D", "ohtori")
        .body(Body::from(
            r#"{"_type":"location","tst":1,"lat":1.0,"lon":2.0}"#,
        ))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"[]");
}

/// 認証ヘッダが無い POST を 401 で弾き、WWW-Authenticate を返すことを確認する。
#[tokio::test]
async fn post_pub_rejects_unauthenticated_request() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .body(Body::from("{}"))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().contains_key("WWW-Authenticate"));
}

/// 配列で送られた複数メッセージを受理することを確認する。
#[tokio::test]
async fn post_pub_accepts_array_body() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"[{"_type":"location","tst":1},{"_type":"location","tst":2}]"#,
        ))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// 1 MiB を超えるボディを 413 で弾くことを確認する。
#[tokio::test]
async fn post_pub_rejects_oversized_body() {
    let oversized = "a".repeat(1024 * 1024 + 1);
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(oversized))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// 壊れた JSON を 400 で弾くことを確認する。
#[tokio::test]
async fn post_pub_rejects_invalid_json() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// 死活確認の GET が 200 を返すことを確認する。
///
/// トンネルと死活監視がこの口を使う。
#[tokio::test]
async fn get_healthz_returns_ok() {
    let request = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
