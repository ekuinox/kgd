//! Notion API のリクエスト・レスポンス型定義。

use serde::{Deserialize, Serialize};

/// Notion タグ設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct NotionTagConfig {
    /// プロパティ名
    pub property: String,
    /// 設定する値
    pub value: String,
    /// マルチセレクトかどうか（デフォルト: false）
    #[serde(default)]
    pub multi_select: bool,
}

/// ファイルアップロードのレスポンス。
#[derive(Debug, Deserialize)]
pub(super) struct FileUploadResponse {
    pub(super) id: String,
    pub(super) status: String,
}

/// ファイルアップロードのリクエストボディ。
#[derive(Debug, Serialize)]
pub(super) struct CreateFileUploadRequest {
    pub(super) mode: String,
    pub(super) filename: String,
    pub(super) content_type: String,
}

/// ブロック追加レスポンスのブロック情報。
#[derive(Debug, Deserialize)]
pub(super) struct BlockInfo {
    pub(super) id: String,
}

/// ブロック追加レスポンス。
#[derive(Debug, Deserialize)]
pub(super) struct AppendBlockChildrenResponse {
    pub(super) results: Vec<BlockInfo>,
}

/// データベースクエリレスポンスのページ情報。
#[derive(Debug, Deserialize)]
pub(super) struct PageInfo {
    pub(super) id: String,
    pub(super) url: String,
}

/// データベースクエリレスポンス。
#[derive(Debug, Deserialize)]
pub(super) struct DatabaseQueryResponse {
    pub(super) results: Vec<PageInfo>,
}
