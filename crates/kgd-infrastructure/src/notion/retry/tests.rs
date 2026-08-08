//! 再試行とエラー分類の単体テスト。

use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context as _, anyhow};

use crate::notion::stub_server::{StubResponse, StubServer};

use super::*;

/// テスト用に待ち時間をなくした設定。
fn no_wait_policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::ZERO,
    }
}

/// 指定したステータスの [`NotionStatusError`] を anyhow エラーとして作る。
fn status_error(status: u16) -> anyhow::Error {
    anyhow::Error::new(NotionStatusError {
        operation: "test".to_string(),
        status: StatusCode::from_u16(status).unwrap(),
        body: String::new(),
    })
}

/// 長いレスポンスボディが表示時に切り詰められることを確認する。
///
/// このエラー文言は Discord にも出るため、長さの上限が必要になる。
#[test]
fn status_error_truncates_long_body() {
    let error = NotionStatusError {
        operation: "test".to_string(),
        status: StatusCode::BAD_REQUEST,
        body: "あ".repeat(1000),
    };

    let message = error.to_string();

    assert!(message.ends_with("..."));
    assert!(message.chars().count() < 400);
}

/// 待機時間が試行ごとに 2 倍になることを確認する。
#[test]
fn delay_after_doubles_per_attempt() {
    let policy = RetryPolicy {
        max_attempts: 4,
        base_delay: Duration::from_millis(100),
    };

    assert_eq!(policy.delay_after(0), Duration::from_millis(100));
    assert_eq!(policy.delay_after(1), Duration::from_millis(200));
    assert_eq!(policy.delay_after(2), Duration::from_millis(400));
}

/// 5xx と 429 のみを一時的な失敗として扱うことを確認する。
#[test]
fn retryable_status_covers_rate_limit_and_server_errors() {
    assert!(is_retryable_status(429));
    assert!(is_retryable_status(500));
    assert!(is_retryable_status(503));
    assert!(!is_retryable_status(400));
    assert!(!is_retryable_status(404));
}

/// context で包まれた原因までさかのぼって判定することを確認する。
#[test]
fn should_retry_walks_error_chain() {
    let error = Err::<(), _>(status_error(503))
        .context("Failed to create Notion page")
        .unwrap_err();

    assert!(should_retry(&error, RetryScope::Transient));
}

/// 副作用のある操作では 5xx を再試行しないことを確認する。
#[test]
fn connect_only_scope_ignores_server_errors() {
    let error = status_error(503);

    assert!(should_retry(&error, RetryScope::Transient));
    assert!(!should_retry(&error, RetryScope::ConnectOnly));
}

/// 接続を確立できなかった場合は、どちらのスコープでも再試行することを確認する。
///
/// 障害時に実際に発生したのはこの種類のエラーだった。
#[tokio::test]
async fn should_retry_connect_failure_in_both_scopes() {
    // 待ち受けていないポートへの接続は接続確立フェーズで失敗する
    let error = reqwest::Client::new()
        .get("http://127.0.0.1:1/")
        .send()
        .await
        .expect_err("connection to a closed port must fail");
    assert!(
        error.is_connect(),
        "expected a connect-phase error: {error}"
    );

    let error = Err::<(), _>(error)
        .context("Failed to create Notion page")
        .unwrap_err();

    assert!(should_retry(&error, RetryScope::Transient));
    assert!(should_retry(&error, RetryScope::ConnectOnly));
}

/// 確立済みの接続が応答前に切られた場合は、Transient でのみ再試行することを確認する。
///
/// リクエストが処理された可能性が残るため、副作用のある操作では再試行しない。
#[tokio::test]
async fn should_retry_connection_closed_only_in_transient_scope() {
    // 接続を受け付けた直後に応答せず切断するサーバーを立てる
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let address = listener
        .local_addr()
        .expect("failed to read listener address");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });

    let error = reqwest::Client::new()
        .get(format!("http://{}/", address))
        .send()
        .await
        .expect_err("request to a disconnecting server must fail");
    assert!(
        !error.is_connect(),
        "expected a post-connect error: {error}"
    );

    let error = Err::<(), _>(error)
        .context("Failed to append blocks")
        .unwrap_err();

    assert!(should_retry(&error, RetryScope::Transient));
    assert!(!should_retry(&error, RetryScope::ConnectOnly));
}

/// 応答を待ちきれなかった場合は、Transient でのみ再試行することを確認する。
#[tokio::test]
async fn should_retry_timeout_only_in_transient_scope() {
    // 接続は受け付けるが応答を返さないサーバーを立てる
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let address = listener
        .local_addr()
        .expect("failed to read listener address");
    tokio::spawn(async move {
        let mut accepted = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            // 応答せずに接続を保持する
            accepted.push(stream);
        }
    });

    let error = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap()
        .get(format!("http://{}/", address))
        .send()
        .await
        .expect_err("request must time out");
    assert!(error.is_timeout(), "expected a timeout error: {error}");

    let error = anyhow::Error::new(error);

    assert!(should_retry(&error, RetryScope::Transient));
    assert!(!should_retry(&error, RetryScope::ConnectOnly));
}

/// ステータスを持つ reqwest エラーもステータスで判定することを確認する。
#[tokio::test]
async fn should_retry_reqwest_error_carrying_status() {
    let server = StubServer::start(vec![
        StubResponse::status(503, "{}"),
        StubResponse::status(400, "{}"),
    ])
    .await;
    let client = reqwest::Client::new();

    let retryable = client
        .get(format!("{}/", server.base_url()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .expect_err("503 must be an error");
    let permanent = client
        .get(format!("{}/", server.base_url()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .expect_err("400 must be an error");

    assert!(should_retry(
        &anyhow::Error::new(retryable),
        RetryScope::Transient
    ));
    assert!(!should_retry(
        &anyhow::Error::new(permanent),
        RetryScope::Transient
    ));
}

/// 分類できないエラーは再試行しないことを確認する。
#[test]
fn should_not_retry_unknown_error() {
    let error = anyhow!("something went wrong");

    assert!(!should_retry(&error, RetryScope::Transient));
    assert!(!should_retry(&error, RetryScope::ConnectOnly));
}

/// 一時的な失敗のあと成功すれば、その結果を返すことを確認する。
#[tokio::test]
async fn retry_succeeds_after_transient_failures() {
    let calls = AtomicU32::new(0);

    let result = retry_with_policy("test", RetryScope::Transient, no_wait_policy(3), |_| {
        let calls = &calls;
        async move {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                return Err(status_error(503));
            }
            Ok("ok")
        }
    })
    .await;

    assert_eq!(result.unwrap(), "ok");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

/// 再試行できないエラーでは即座に諦めることを確認する。
#[tokio::test]
async fn retry_gives_up_immediately_on_permanent_error() {
    let calls = AtomicU32::new(0);

    let result = retry_with_policy("test", RetryScope::Transient, no_wait_policy(3), |_| {
        let calls = &calls;
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(status_error(400))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// 最大試行回数を超えて再試行しないことを確認する。
#[tokio::test]
async fn retry_stops_at_max_attempts() {
    let calls = AtomicU32::new(0);

    let result = retry_with_policy("test", RetryScope::Transient, no_wait_policy(3), |_| {
        let calls = &calls;
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(status_error(503))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

/// 試行回数が 0 起算で渡されることを確認する。
#[tokio::test]
async fn retry_passes_attempt_index_to_the_operation() {
    let seen = std::sync::Mutex::new(Vec::new());

    let result = retry_with_policy(
        "test",
        RetryScope::Transient,
        no_wait_policy(3),
        |attempt| {
            let seen = &seen;
            async move {
                seen.lock().unwrap().push(attempt);
                if attempt < 1 {
                    return Err(status_error(503));
                }
                Ok(())
            }
        },
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(*seen.lock().unwrap(), vec![0, 1]);
}
