//! Discord メッセージを Notion に同期する機能を提供する。

use anyhow::{Context as _, Result};
use serenity::model::channel::{Attachment, Message};

use crate::config::DiaryConfig;

use super::url_parser;
use super::{DiaryStore, MessageBlock, NotionClient};

/// 同期結果の情報。
pub struct SyncResult {
    /// 同期が実行されたかどうか
    pub synced: bool,
    /// 作成されたブロック数
    pub block_count: usize,
}

/// メッセージを Notion に同期するためのシンクロナイザー。
pub struct MessageSyncer<'a> {
    /// Notion クライアント
    notion: &'a NotionClient,
    /// 日報ストア
    store: &'a DiaryStore,
    /// HTTP クライアント（画像ダウンロード用）
    http_client: reqwest::Client,
    /// URL 変換ルール（コンパイル済み）
    url_rules: url_parser::CompiledUrlRules,
}

impl<'a> MessageSyncer<'a> {
    /// 新しい MessageSyncer を作成する。
    ///
    /// # Arguments
    /// * `notion` - Notion クライアント
    /// * `store` - 日報ストア
    /// * `diary_config` - 日報設定（URL ルールとデフォルト変換を含む）
    pub fn new(
        notion: &'a NotionClient,
        store: &'a DiaryStore,
        diary_config: &DiaryConfig,
    ) -> Result<Self> {
        let url_rules = url_parser::compile_url_rules(
            &diary_config.url_rules,
            &diary_config.default_convert_to,
        )?;

        Ok(Self {
            notion,
            store,
            http_client: reqwest::Client::new(),
            url_rules,
        })
    }

    /// メッセージを Notion ページに同期する。
    ///
    /// テキストと添付ファイルのブロックを1回の API 呼び出しでまとめて追加することで、
    /// ブロック間に不要な空行が入るのを防ぐ。
    ///
    /// # Returns
    /// 同期結果（同期されたかどうかと作成されたブロック情報）
    pub async fn sync_message(&self, page_id: &str, message: &Message) -> Result<SyncResult> {
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
            let result =
                url_parser::build_rich_text_and_url_blocks(&message.content, &self.url_rules);

            for (block_json, block_type) in result.blocks {
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
            self.store_message_block(message.id.get(), block_id, block_type, i as i32)
                .await?;
        }

        Ok(SyncResult {
            synced: true,
            block_count: block_meta.len(),
        })
    }

    /// メッセージが更新されたときに Notion ブロックを更新する。
    ///
    /// テキストブロックのみ更新可能。画像・ブックマークブロックは更新されない。
    pub async fn update_message(&self, message: &Message) -> Result<bool> {
        let blocks = self.store.get_blocks_by_message(message.id.get()).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // テキストブロックのみ更新（URL をリンク化）
        let result = url_parser::build_rich_text_and_url_blocks(&message.content, &self.url_rules);
        let text_rich_texts: Vec<Vec<serde_json::Value>> = result
            .blocks
            .iter()
            .filter(|(_, block_type)| block_type == "text")
            .filter_map(|(block_json, _)| block_json["paragraph"]["rich_text"].as_array().cloned())
            .collect();

        let text_blocks: Vec<&MessageBlock> =
            blocks.iter().filter(|b| b.block_type == "text").collect();

        for (block, rich_text) in text_blocks.iter().zip(text_rich_texts.iter()) {
            self.notion
                .update_text_block(&block.block_id, rich_text.clone())
                .await?;
        }

        Ok(true)
    }

    /// メッセージが削除されたときに対応する Notion ブロックを削除する。
    pub async fn delete_message(&self, message_id: u64) -> Result<bool> {
        let blocks = self.store.get_blocks_by_message(message_id).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // すべてのブロックを削除
        for block in &blocks {
            self.notion.delete_block(&block.block_id).await?;
        }

        // DB からブロック情報を削除
        self.store.delete_blocks_by_message(message_id).await?;

        Ok(true)
    }

    /// 添付ファイルをアップロードし、対応するブロック JSON とメタ情報を収集する。
    ///
    /// HEIC の場合は JPG 変換版（画像ブロック）と元ファイル（ファイルブロック）の 2 つを追加する。
    async fn prepare_attachment_blocks(
        &self,
        attachment: &Attachment,
        children: &mut Vec<serde_json::Value>,
        block_meta: &mut Vec<String>,
    ) -> Result<()> {
        let file_type = classify_file(&attachment.filename);

        match file_type {
            FileType::Image => {
                let (data, content_type) = self.download_attachment(attachment).await?;
                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .context("Failed to upload image to Notion")?;
                children.push(image_block_json(&file_upload_id));
                block_meta.push("image".to_string());
            }
            FileType::Heic => {
                let (data, content_type) = self.download_attachment(attachment).await?;

                // HEIC を JPEG に変換してアップロード (Unix のみ)
                #[cfg(unix)]
                match heif::convert_heic_to_jpeg(&data) {
                    Ok(jpeg_data) => {
                        let jpeg_filename = replace_extension(&attachment.filename, "jpg");
                        let jpeg_upload_id = self
                            .notion
                            .upload_file(&jpeg_filename, "image/jpeg", jpeg_data)
                            .await
                            .context("Failed to upload converted JPEG to Notion")?;
                        children.push(image_block_json(&jpeg_upload_id));
                        block_meta.push("image".to_string());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to convert HEIC to JPEG, skipping conversion");
                    }
                }

                #[cfg(not(unix))]
                tracing::warn!(
                    filename = %attachment.filename,
                    "HEIC to JPEG conversion is not supported on this platform"
                );

                // 元の HEIC ファイルもアップロード
                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to upload file to Notion: filename={}, content_type={}",
                            attachment.filename, content_type
                        )
                    })?;
                children.push(file_block_json(&file_upload_id, &attachment.filename));
                block_meta.push("file".to_string());
            }
            FileType::Other => {
                let (data, content_type) = self.download_attachment(attachment).await?;

                tracing::debug!(
                    filename = %attachment.filename,
                    content_type = %content_type,
                    size = data.len(),
                    "Uploading file to Notion"
                );

                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to upload file to Notion: filename={}, content_type={}",
                            attachment.filename, content_type
                        )
                    })?;
                children.push(file_block_json(&file_upload_id, &attachment.filename));
                block_meta.push("file".to_string());
            }
        }

        Ok(())
    }

    /// メッセージブロック情報を DB に保存する。
    async fn store_message_block(
        &self,
        message_id: u64,
        block_id: String,
        block_type: &str,
        block_order: i32,
    ) -> Result<()> {
        let message_block = MessageBlock {
            message_id,
            block_id,
            block_type: block_type.to_string(),
            block_order,
        };
        self.store.insert_message_block(&message_block).await?;
        Ok(())
    }

    /// Discord から添付ファイルをダウンロードする。
    async fn download_attachment(&self, attachment: &Attachment) -> Result<(Vec<u8>, String)> {
        let response = self
            .http_client
            .get(&attachment.url)
            .send()
            .await
            .context("Failed to download file from Discord")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download file: status = {}", response.status());
        }

        let header_content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // Discord が返す Content-Type が汎用的な場合、ファイル名の拡張子から推定する
        let content_type = if header_content_type == "application/octet-stream"
            || header_content_type.is_empty()
        {
            guess_content_type(&attachment.filename).unwrap_or(header_content_type)
        } else {
            header_content_type
        };

        let data = response
            .bytes()
            .await
            .context("Failed to read file data")?
            .to_vec();

        Ok((data, content_type))
    }
}

/// アップロード済み画像の画像ブロック JSON を生成する。
fn image_block_json(file_upload_id: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "image",
        "image": {
            "type": "file_upload",
            "file_upload": {
                "id": file_upload_id
            }
        }
    })
}

/// アップロード済みファイルのファイルブロック JSON を生成する。
fn file_block_json(file_upload_id: &str, filename: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "file",
        "file": {
            "type": "file_upload",
            "file_upload": {
                "id": file_upload_id
            },
            "name": filename
        }
    })
}

/// ファイル名の拡張子から Content-Type を推定する。
fn guess_content_type(filename: &str) -> Option<String> {
    mime_guess::from_path(filename)
        .first()
        .map(|mime| mime.to_string())
}

/// ファイルの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileType {
    /// 画像ファイル（.png, .jpg, .jpeg, .gif, .webp）
    Image,
    /// HEIC/HEIF ファイル（変換が必要）
    Heic,
    /// その他のファイル
    Other,
}

/// ファイル名からファイル種類を判定する。
fn classify_file(filename: &str) -> FileType {
    let lower = filename.to_lowercase();

    let image_extensions = [".png", ".jpg", ".jpeg", ".gif", ".webp"];
    if image_extensions.iter().any(|ext| lower.ends_with(ext)) {
        return FileType::Image;
    }

    let heic_extensions = [".heic", ".heif"];
    if heic_extensions.iter().any(|ext| lower.ends_with(ext)) {
        return FileType::Heic;
    }

    FileType::Other
}

/// ファイル名の拡張子を置き換える。
fn replace_extension(filename: &str, new_ext: &str) -> String {
    if let Some(pos) = filename.rfind('.') {
        format!("{}.{}", &filename[..pos], new_ext)
    } else {
        format!("{}.{}", filename, new_ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_file_image() {
        assert_eq!(classify_file("photo.png"), FileType::Image);
        assert_eq!(classify_file("photo.PNG"), FileType::Image);
        assert_eq!(classify_file("image.jpg"), FileType::Image);
        assert_eq!(classify_file("image.JPG"), FileType::Image);
        assert_eq!(classify_file("picture.jpeg"), FileType::Image);
        assert_eq!(classify_file("animation.gif"), FileType::Image);
        assert_eq!(classify_file("modern.webp"), FileType::Image);
    }

    #[test]
    fn test_classify_file_heic() {
        assert_eq!(classify_file("photo.heic"), FileType::Heic);
        assert_eq!(classify_file("photo.HEIC"), FileType::Heic);
        assert_eq!(classify_file("image.heif"), FileType::Heic);
        assert_eq!(classify_file("image.HEIF"), FileType::Heic);
    }

    #[test]
    fn test_classify_file_other() {
        assert_eq!(classify_file("document.pdf"), FileType::Other);
        assert_eq!(classify_file("archive.zip"), FileType::Other);
        assert_eq!(classify_file("script.js"), FileType::Other);
        assert_eq!(classify_file("noextension"), FileType::Other);
    }

    #[test]
    fn test_classify_file_rejects_similar_names() {
        // ドットなしの拡張子文字列で終わるファイル名は画像として判定されない
        assert_eq!(classify_file("somepng"), FileType::Other);
        assert_eq!(classify_file("filejpg"), FileType::Other);
        assert_eq!(classify_file("imageheic"), FileType::Other);
    }

    #[test]
    fn test_guess_content_type() {
        assert_eq!(
            guess_content_type("photo.heic"),
            Some("image/heic".to_string())
        );
        assert_eq!(
            guess_content_type("photo.HEIC"),
            Some("image/heic".to_string())
        );
        assert_eq!(
            guess_content_type("image.heif"),
            Some("image/heif".to_string())
        );
        assert_eq!(
            guess_content_type("photo.png"),
            Some("image/png".to_string())
        );
        assert_eq!(
            guess_content_type("photo.jpg"),
            Some("image/jpeg".to_string())
        );
        assert_eq!(
            guess_content_type("doc.pdf"),
            Some("application/pdf".to_string())
        );
        assert_eq!(
            guess_content_type("archive.zip"),
            Some("application/zip".to_string())
        );
        assert_eq!(
            guess_content_type("data.gpx"),
            Some("application/gpx+xml".to_string())
        );
        assert_eq!(guess_content_type("noextension"), None);
    }

    #[test]
    fn test_replace_extension() {
        assert_eq!(replace_extension("photo.heic", "jpg"), "photo.jpg");
        assert_eq!(replace_extension("image.HEIC", "jpg"), "image.jpg");
        assert_eq!(replace_extension("my.photo.heic", "jpg"), "my.photo.jpg");
        assert_eq!(replace_extension("noextension", "jpg"), "noextension.jpg");
    }
}
