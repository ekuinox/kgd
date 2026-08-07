//! Notion API 呼び出しのエラー分類と再試行。
//!
//! Notion への接続は一時的に切断されることがあり (`Connection reset by peer` など)、
//! 一度の失敗で諦めると日報スレッドの作成そのものが失敗してしまう。
//! ここでは「再試行してよい失敗か」を判定し、指数バックオフで再試行する。

use std::{error::Error as StdError, fmt, future::Future, time::Duration};

use anyhow::{Result, bail};
use reqwest::{Response, StatusCode};
use tracing::warn;

/// 再試行の対象とするエラーの範囲。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryScope {
    /// 接続の確立に失敗した場合のみ再試行する。
    ///
    /// リクエストがサーバーに届いていないことが確実なため、
    /// 再実行すると副作用が重複する操作 (ブロックの追加など) にも使える。
    ConnectOnly,
    /// 接続失敗に加えて、タイムアウトや 429 / 5xx でも再試行する。
    ///
    /// 再実行しても副作用が重複しない操作にのみ使う。
    Transient,
}

/// 既定の設定で、一時的な失敗に対して再試行しながら操作を実行する。
///
/// `call` には 0 起算の試行回数を渡す。2 回目以降であることを利用して、
/// 副作用の重複を避ける確認処理を挟むことができる。
pub(crate) async fn retry<T, F, Fut>(operation: &str, scope: RetryScope, call: F) -> Result<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    retry_with_policy(operation, scope, RetryPolicy::default(), call).await
}

/// レスポンスが成功ステータスであることを確認する。
///
/// 失敗した場合はステータスコードを保持した [`NotionStatusError`] を返し、
/// 再試行の判定に使えるようにする。
pub(crate) async fn ensure_success(response: Response, operation: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();
    bail!(NotionStatusError {
        operation: operation.to_string(),
        status,
        body,
    })
}

/// Notion API が成功以外のステータスを返したことを表すエラー。
#[derive(Debug)]
pub(crate) struct NotionStatusError {
    /// 実行しようとした操作の名前
    pub operation: String,
    /// 返ってきたステータスコード
    pub status: StatusCode,
    /// レスポンスボディ
    pub body: String,
}

impl fmt::Display for NotionStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} - {}", self.operation, self.status, self.body)
    }
}

impl StdError for NotionStatusError {}

/// 再試行の設定。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryPolicy {
    /// 最大試行回数 (初回を含む)
    pub max_attempts: u32,
    /// 最初の待機時間。試行ごとに 2 倍にする
    pub base_delay: Duration,
}

impl RetryPolicy {
    /// 0 起算で `attempt` 回目の試行が失敗したあとに待つ時間を返す。
    fn delay_after(&self, attempt: u32) -> Duration {
        self.base_delay * 2u32.pow(attempt)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
        }
    }
}

/// 設定を指定して、一時的な失敗に対して再試行しながら操作を実行する。
async fn retry_with_policy<T, F, Fut>(
    operation: &str,
    scope: RetryScope,
    policy: RetryPolicy,
    mut call: F,
) -> Result<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        let error = match call(attempt).await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };

        let is_last_attempt = attempt + 1 >= policy.max_attempts;
        if is_last_attempt || !should_retry(&error, scope) {
            return Err(error);
        }

        let delay = policy.delay_after(attempt);
        warn!(
            operation,
            attempt = attempt + 1,
            delay_ms = delay.as_millis() as u64,
            error = %error,
            "Notion request failed, retrying"
        );
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

/// エラーが再試行に値するかどうかを判定する。
fn should_retry(error: &anyhow::Error, scope: RetryScope) -> bool {
    error.chain().any(|cause| is_retryable(cause, scope))
}

/// 個々の原因が再試行に値するかどうかを判定する。
fn is_retryable(cause: &(dyn StdError + 'static), scope: RetryScope) -> bool {
    if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
        if error.is_connect() {
            return true;
        }
        if scope == RetryScope::ConnectOnly {
            return false;
        }
        if error.is_timeout() {
            return true;
        }
        if let Some(status) = error.status() {
            return is_retryable_status(status.as_u16());
        }
    }

    if scope == RetryScope::ConnectOnly {
        return false;
    }

    if let Some(error) = cause.downcast_ref::<NotionStatusError>() {
        return is_retryable_status(error.status.as_u16());
    }

    false
}

/// ステータスコードが再試行に値するかどうかを判定する。
///
/// レート制限 (429) とサーバー側の一時的な障害 (5xx) のみを対象とする。
fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

#[cfg(test)]
mod tests;
