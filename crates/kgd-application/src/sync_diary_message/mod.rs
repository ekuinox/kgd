//! Discord メッセージを Notion に同期するユースケース。

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use kgd_domain::{
    CompiledUrlRules, MessageBlock, OgpMetadata, SyncMessage, apply_ogp_to_bookmark,
    build_rich_text_and_url_blocks,
};

use super::ports::{AttachmentDownloader, DiaryRepository, ImageConverter, NotionApi, OgpClient};

mod attachments;
mod edits;

/// 同期結果の情報。
pub struct SyncResult {
    /// 同期が実行されたかどうか
    pub synced: bool,
    /// 作成されたブロック数
    pub block_count: usize,
}

/// メッセージを Notion に同期するユースケース。
///
/// 外部 IO はすべてポート経由で行うため、mockall のモックで単体テストできる。
pub struct SyncDiaryMessageUseCase {
    /// Notion API ポート
    notion: Arc<dyn NotionApi>,
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// 添付ファイルダウンローダーポート
    downloader: Arc<dyn AttachmentDownloader>,
    /// 画像変換ポート
    converter: Arc<dyn ImageConverter>,
    /// OGP クライアントポート（None の場合は OGP 取得を行わない）
    ogp: Option<Arc<dyn OgpClient>>,
    /// URL 変換ルール（コンパイル済み）
    url_rules: CompiledUrlRules,
}

impl SyncDiaryMessageUseCase {
    /// 新しい SyncDiaryMessageUseCase を作成する。
    pub fn new(
        notion: Arc<dyn NotionApi>,
        repo: Arc<dyn DiaryRepository>,
        downloader: Arc<dyn AttachmentDownloader>,
        converter: Arc<dyn ImageConverter>,
        ogp: Option<Arc<dyn OgpClient>>,
        url_rules: CompiledUrlRules,
    ) -> Self {
        Self {
            notion,
            repo,
            downloader,
            converter,
            ogp,
            url_rules,
        }
    }

    /// メッセージを Notion ページに同期する。
    ///
    /// テキストと添付ファイルのブロックを1回の API 呼び出しでまとめて追加することで、
    /// ブロック間に不要な空行が入るのを防ぐ。
    ///
    /// # Returns
    /// 同期結果（同期されたかどうかと作成されたブロック数）
    pub async fn sync(&self, page_id: &str, message: &SyncMessage) -> Result<SyncResult> {
        let has_content = !message.content.is_empty();
        let has_attachments = !message.attachments.is_empty();

        if !has_content && !has_attachments {
            return Ok(SyncResult {
                synced: false,
                block_count: 0,
            });
        }

        // ブロック JSON とメタ情報（block_type）を収集する
        // 順序: 添付ファイル（画像埋め込み → ファイルリンク） → テキスト
        let mut children: Vec<serde_json::Value> = Vec::new();
        let mut block_meta: Vec<String> = Vec::new(); // 各ブロックの種別

        // 添付ファイル: ファイルをアップロードしてブロック JSON を収集
        for attachment in &message.attachments {
            self.prepare_attachment_blocks(attachment, &mut children, &mut block_meta)
                .await?;
        }

        // テキストブロック（URL をリンク化 + ルールに基づく追加ブロック生成）
        // 出現順に paragraph / bookmark / embed ブロックが並ぶ
        if has_content {
            let result = build_rich_text_and_url_blocks(&message.content, &self.url_rules);

            // OGP メタデータを並列取得
            let ogp_map = self.fetch_ogp_for_bookmarks(&result.bookmark_urls).await;

            for (mut block_json, block_type) in result.blocks {
                // ブックマークブロックに OGP メタデータを適用
                if block_type == "bookmark"
                    && let Some(url) = block_json["bookmark"]["url"].as_str()
                    && let Some(ogp) = ogp_map.get(url)
                {
                    apply_ogp_to_bookmark(&mut block_json, ogp);
                }
                children.push(block_json);
                block_meta.push(block_type);
            }
        }

        if children.is_empty() {
            return Ok(SyncResult {
                synced: false,
                block_count: 0,
            });
        }

        // 全ブロックを一括で追加
        let block_ids = self.notion.append_blocks(page_id, children).await?;

        // DB にブロック情報を保存
        for (i, (block_id, block_type)) in block_ids.into_iter().zip(block_meta.iter()).enumerate()
        {
            let message_block = MessageBlock {
                message_id: message.message_id,
                block_id,
                block_type: block_type.to_string(),
                block_order: i as i32,
            };
            self.repo.insert_message_block(&message_block).await?;
        }

        Ok(SyncResult {
            synced: true,
            block_count: block_meta.len(),
        })
    }

    /// Bookmark URL の OGP メタデータを並列で取得する。
    async fn fetch_ogp_for_bookmarks(&self, urls: &[String]) -> HashMap<String, OgpMetadata> {
        let Some(ogp) = &self.ogp else {
            return HashMap::new();
        };

        if urls.is_empty() {
            return HashMap::new();
        }

        ogp.fetch_many(urls).await
    }
}

#[cfg(test)]
mod tests;
