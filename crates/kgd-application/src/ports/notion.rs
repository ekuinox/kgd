//! Notion API へのアクセスを抽象化するポート。

use anyhow::Result;

/// Notion API へのアクセスを抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait NotionApi: Send + Sync {
    /// 指定したタイトルの日報ページを検索し、存在すれば (ページ ID, URL) を返す。
    async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>>;

    /// 日報ページを作成し、(ページ ID, URL) を返す。
    async fn create_diary_page(&self, title: &str) -> Result<(String, String)>;

    /// ファイルをアップロードし、ファイルアップロード ID を返す。
    async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String>;

    /// 複数のブロックを一括でページに追加し、作成されたブロック ID のリストを返す。
    async fn append_blocks(
        &self,
        page_id: &str,
        children: Vec<serde_json::Value>,
    ) -> Result<Vec<String>>;

    /// テキストブロックの rich_text を更新する。
    async fn update_text_block(
        &self,
        block_id: &str,
        rich_text: Vec<serde_json::Value>,
    ) -> Result<()>;

    /// ブロックを削除する。
    async fn delete_block(&self, block_id: &str) -> Result<()>;
}
