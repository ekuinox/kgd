use std::sync::{Arc, Mutex};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

use anyhow::Result;
use kgd_application::{RecordLocationUseCase, ports::LocationRepository};
use kgd_domain::OwnTracksMessage;

use super::*;

/// 呼び出し引数を記録しつつ、設定した結果を返すリポジトリのスタブ。
///
/// kgd-application のモックは `#[cfg(test)]` で生成されるためクレート外からは
/// 参照できない。ここで検証したいのはハンドラがユースケースへ渡す
/// `user_id`/`device_id` の導出結果とメッセージの中身なので、手書きのスタブに
/// 呼び出し引数を記録する。
struct StubLocationRepository {
    /// 呼び出しごとに渡されたメッセージ一覧
    calls: Arc<Mutex<Vec<Vec<OwnTracksMessage>>>>,
    /// `insert_messages` を失敗させるかどうか
    should_fail: bool,
}

#[async_trait::async_trait]
impl LocationRepository for StubLocationRepository {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        self.calls.lock().unwrap().push(messages.to_vec());
        if self.should_fail {
            return Err(anyhow::anyhow!("stub: insert_messages failed"));
        }
        Ok(messages.len())
    }
}

/// 呼び出し引数を記録するルータを作る。
///
/// `should_fail` が true の場合、リポジトリの保存は常に失敗する。
fn test_router_with(
    calls: Arc<Mutex<Vec<Vec<OwnTracksMessage>>>>,
    should_fail: bool,
) -> axum::Router {
    let repo = StubLocationRepository { calls, should_fail };
    owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(repo))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    )
}

/// テスト用のルータを作る。保存は常に成功し、呼び出し引数は検証しない。
fn test_router() -> axum::Router {
    test_router_with(Arc::new(Mutex::new(Vec::new())), false)
}

/// 認証付きの POST に 200 と空の JSON 配列を返し、
/// `X-Limit-U`/`X-Limit-D` ヘッダの値がそのまま user_id/device_id に
/// 使われることを確認する。
///
/// 応答が空ボディや非配列だと OwnTracks が送信失敗と解釈するため。
#[tokio::test]
async fn post_pub_returns_empty_json_array_and_uses_header_identifiers() {
    let calls = Arc::new(Mutex::new(Vec::new()));
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

    let response = test_router_with(calls.clone(), false)
        .oneshot(request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"[]");

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].len(), 1);
    assert_eq!(recorded[0][0].user_id, "ekuinox");
    assert_eq!(recorded[0][0].device_id, "ohtori");
}

/// `X-Limit-U`/`X-Limit-D` ヘッダが無い場合、`?u=&d=` クエリの値が
/// user_id/device_id に使われることを確認する。
///
/// OwnTracks はヘッダではなくクエリで名乗ることがあるため。
#[tokio::test]
async fn post_pub_uses_query_identifiers_when_headers_are_absent() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let request = Request::builder()
        .method("POST")
        .uri("/pub?u=foo&d=bar")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"_type":"location","tst":1}"#))
        .unwrap();

    let response = test_router_with(calls.clone(), false)
        .oneshot(request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].len(), 1);
    assert_eq!(recorded[0][0].user_id, "foo");
    assert_eq!(recorded[0][0].device_id, "bar");
}

/// リポジトリへの保存が失敗しても 200 と空の JSON 配列を返すことを確認する。
///
/// 端末に再送させても直らないため、記録に失敗しても送信成功として扱う。
/// これは仕様の「常に 200」を保証する要のふるまい。
#[tokio::test]
async fn post_pub_returns_empty_json_array_when_repository_fails() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"_type":"location","tst":1}"#))
        .unwrap();

    let response = test_router_with(calls.clone(), true)
        .oneshot(request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"[]");
    assert_eq!(calls.lock().unwrap().len(), 1);
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

/// 壊れた JSON でも 200 と空の JSON 配列を返すことを確認する。
///
/// 仕様上、`401` (認証失敗) と `413` (ボディ超過) 以外は常に `200` + `[]` を
/// 返すことが必須要件のため、JSON の解釈に失敗しても送信成功として扱う。
#[tokio::test]
async fn post_pub_returns_empty_json_array_for_invalid_json() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"[]");
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
