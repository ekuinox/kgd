//! Notion アダプタの単体テスト。
//!
//! 実際の HTTP でスタブサーバーとやり取りし、リクエストの組み立てと
//! 再試行の挙動を合わせて確認する。

use kgd_application::ports::NotionApi;

use super::*;

use super::stub_server::{StubResponse, StubServer};

/// スタブサーバーを指すクライアントを作る。
///
/// 戻り値を [`NotionApi`] にすることで、固有メソッドではなく
/// 再試行を挟むトレイト実装の側を必ず呼ぶようにしている。
fn client(server: &StubServer) -> impl NotionApi {
    NotionClient::with_base_url(server.base_url(), "token", "db", "名前", vec![])
        .expect("failed to build client")
}

/// 通常の生成では Notion の URL を向くことを確認する。
///
/// テストはスタブサーバーを向くため、本番の向き先はここで押さえる。
#[test]
fn new_points_at_the_notion_api() {
    let client = NotionClient::new("token", "db", "名前", vec![]).unwrap();

    assert_eq!(client.base_url, "https://api.notion.com/v1");
}

/// ページ検索が結果の先頭からページ ID と URL を取り出すことを確認する。
#[tokio::test]
async fn find_diary_page_by_title_returns_first_result() {
    let server = StubServer::start(vec![StubResponse::ok(
        r#"{"results":[{"id":"page-1","url":"https://notion.example/page-1"}]}"#,
    )])
    .await;

    let found = client(&server)
        .find_diary_page_by_title("2026-08-06")
        .await
        .unwrap();

    assert_eq!(
        found,
        Some((
            "page-1".to_string(),
            "https://notion.example/page-1".to_string()
        ))
    );
    assert_eq!(server.requests(), vec!["POST /databases/db/query"]);
}

/// 検索結果が空の場合は None を返すことを確認する。
#[tokio::test]
async fn find_diary_page_by_title_returns_none_when_empty() {
    let server = StubServer::start(vec![StubResponse::ok(r#"{"results":[]}"#)]).await;

    let found = client(&server)
        .find_diary_page_by_title("2026-08-06")
        .await
        .unwrap();

    assert_eq!(found, None);
}

/// ページ作成が /pages へ送られ、応答からページ ID と URL を取り出すことを確認する。
#[tokio::test]
async fn create_diary_page_posts_to_pages() {
    let server = StubServer::start(vec![StubResponse::ok(
        r#"{"id":"page-2","url":"https://notion.example/page-2"}"#,
    )])
    .await;

    let created = client(&server)
        .create_diary_page("2026-08-06")
        .await
        .unwrap();

    assert_eq!(
        created,
        (
            "page-2".to_string(),
            "https://notion.example/page-2".to_string()
        )
    );
    assert_eq!(server.requests(), vec!["POST /pages"]);
}

/// ページ作成が 503 で失敗しても、再試行して成功することを確認する。
#[tokio::test]
async fn create_diary_page_retries_server_error() {
    let server = StubServer::start(vec![
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
        // 再試行時は同名ページの有無を先に確認する
        StubResponse::ok(r#"{"results":[]}"#),
        StubResponse::ok(r#"{"id":"page-3","url":"https://notion.example/page-3"}"#),
    ])
    .await;

    let created = client(&server)
        .create_diary_page("2026-08-06")
        .await
        .unwrap();

    assert_eq!(created.0, "page-3");
    assert_eq!(
        server.requests(),
        vec!["POST /pages", "POST /databases/db/query", "POST /pages"]
    );
}

/// 再試行時に同名ページが見つかった場合、作り直さずにそれを使うことを確認する。
///
/// 応答が返る前に失敗した場合、実際にはページが作られている可能性があるため。
#[tokio::test]
async fn create_diary_page_reuses_existing_page_on_retry() {
    let server = StubServer::start(vec![
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
        StubResponse::ok(r#"{"results":[{"id":"page-4","url":"https://notion.example/page-4"}]}"#),
    ])
    .await;

    let created = client(&server)
        .create_diary_page("2026-08-06")
        .await
        .unwrap();

    assert_eq!(created.0, "page-4");
    // 2 回目の POST /pages は行われない
    assert_eq!(
        server.requests(),
        vec!["POST /pages", "POST /databases/db/query"]
    );
}

/// 再試行しても回復しない場合、ステータスを含むエラーを返すことを確認する。
#[tokio::test]
async fn create_diary_page_fails_after_exhausting_retries() {
    let server = StubServer::start(vec![
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
        StubResponse::ok(r#"{"results":[]}"#),
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
        StubResponse::ok(r#"{"results":[]}"#),
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
    ])
    .await;

    let error = client(&server)
        .create_diary_page("2026-08-06")
        .await
        .unwrap_err();

    assert!(format!("{:#}", error).contains("503"));
    // 3 回試行して諦める
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| *request == "POST /pages")
            .count(),
        3
    );
}

/// クライアントエラーは再試行せず、その場で失敗することを確認する。
#[tokio::test]
async fn create_diary_page_does_not_retry_client_error() {
    let server = StubServer::start(vec![StubResponse::status(400, r#"{"message":"bad"}"#)]).await;

    let error = client(&server)
        .create_diary_page("2026-08-06")
        .await
        .unwrap_err();

    assert!(format!("{:#}", error).contains("400"));
    assert_eq!(server.requests(), vec!["POST /pages"]);
}

/// ブロック追加が作成されたブロック ID を返すことを確認する。
#[tokio::test]
async fn append_blocks_returns_created_block_ids() {
    let server = StubServer::start(vec![StubResponse::ok(
        r#"{"results":[{"id":"block-1"},{"id":"block-2"}]}"#,
    )])
    .await;

    let ids = client(&server)
        .append_blocks("page-1", vec![serde_json::json!({})])
        .await
        .unwrap();

    assert_eq!(ids, vec!["block-1", "block-2"]);
    assert_eq!(server.requests(), vec!["PATCH /blocks/page-1/children"]);
}

/// ブロックが空の場合は API を呼ばないことを確認する。
#[tokio::test]
async fn append_blocks_skips_request_when_empty() {
    let server = StubServer::start(vec![]).await;

    let ids = client(&server)
        .append_blocks("page-1", vec![])
        .await
        .unwrap();

    assert!(ids.is_empty());
    assert!(server.requests().is_empty());
}

/// ブロック追加は副作用があるため、5xx でも再試行しないことを確認する。
///
/// 応答が返る前に失敗した場合、リクエストが処理済みでブロックが二重に増えうるため。
#[tokio::test]
async fn append_blocks_does_not_retry_server_error() {
    let server = StubServer::start(vec![
        StubResponse::status(503, r#"{"message":"unavailable"}"#),
        StubResponse::ok(r#"{"results":[{"id":"block-1"}]}"#),
    ])
    .await;

    let error = client(&server)
        .append_blocks("page-1", vec![serde_json::json!({})])
        .await
        .unwrap_err();

    assert!(format!("{:#}", error).contains("503"));
    assert_eq!(server.requests(), vec!["PATCH /blocks/page-1/children"]);
}

/// ブロック削除が DELETE を送ることを確認する。
#[tokio::test]
async fn delete_block_sends_delete_request() {
    let server = StubServer::start(vec![StubResponse::ok("{}")]).await;

    client(&server).delete_block("block-1").await.unwrap();

    assert_eq!(server.requests(), vec!["DELETE /blocks/block-1"]);
}

/// ブロック更新が PATCH を送ることを確認する。
#[tokio::test]
async fn update_text_block_sends_patch_request() {
    let server = StubServer::start(vec![StubResponse::ok("{}")]).await;

    client(&server)
        .update_text_block("block-1", vec![serde_json::json!({})])
        .await
        .unwrap();

    assert_eq!(server.requests(), vec!["PATCH /blocks/block-1"]);
}

/// ファイルアップロードが作成と送信の 2 段階で行われることを確認する。
#[tokio::test]
async fn upload_file_creates_then_sends() {
    let server = StubServer::start(vec![
        StubResponse::ok(r#"{"id":"upload-1","status":"pending"}"#),
        StubResponse::ok(r#"{"id":"upload-1","status":"uploaded"}"#),
    ])
    .await;

    let id = client(&server)
        .upload_file("a.png", "image/png", vec![1, 2, 3])
        .await
        .unwrap();

    assert_eq!(id, "upload-1");
    assert_eq!(
        server.requests(),
        vec!["POST /file_uploads", "POST /file_uploads/upload-1/send"]
    );
}

/// アップロードが完了状態にならなかった場合は失敗として扱うことを確認する。
#[tokio::test]
async fn upload_file_fails_when_not_uploaded() {
    let server = StubServer::start(vec![
        StubResponse::ok(r#"{"id":"upload-1","status":"pending"}"#),
        StubResponse::ok(r#"{"id":"upload-1","status":"failed"}"#),
    ])
    .await;

    let error = client(&server)
        .upload_file("a.png", "image/png", vec![1, 2, 3])
        .await
        .unwrap_err();

    assert!(format!("{:#}", error).contains("failed"));
}
