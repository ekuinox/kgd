//! 添付ファイルのアップロードとブロック JSON 生成。

use anyhow::{Context as _, Result};
use tracing::warn;

use kgd_domain::{
    FileType, SyncAttachment, classify_file, file_block_json, image_block_json,
    is_spoiler_attachment, replace_extension, spoiler_summary, toggle_block_json,
};

use super::SyncDiaryMessage;

impl SyncDiaryMessage {
    /// 添付ファイルをアップロードし、対応するブロック JSON とメタ情報を収集する。
    ///
    /// HEIC の場合は JPG 変換版（画像ブロック）と元ファイル（ファイルブロック）の 2 つを追加する。
    pub(super) async fn prepare_attachment_blocks(
        &self,
        attachment: &SyncAttachment,
        children: &mut Vec<serde_json::Value>,
        block_meta: &mut Vec<String>,
    ) -> Result<()> {
        let file_type = classify_file(&attachment.filename);
        let mut attachment_children = Vec::new();
        let mut attachment_block_meta = Vec::new();

        match file_type {
            FileType::Image => {
                let (data, content_type) = self.downloader.download(attachment).await?;
                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .context("Failed to upload image to Notion")?;
                attachment_children.push(image_block_json(&file_upload_id));
                attachment_block_meta.push("image".to_string());
            }
            FileType::Heic => {
                let (data, content_type) = self.downloader.download(attachment).await?;

                // HEIC を JPEG に変換してアップロード（変換不可ならスキップ）
                match self.converter.heic_to_jpeg(&data) {
                    Ok(jpeg_data) => {
                        let jpeg_filename = replace_extension(&attachment.filename, "jpg");
                        let jpeg_upload_id = self
                            .notion
                            .upload_file(&jpeg_filename, "image/jpeg", jpeg_data)
                            .await
                            .context("Failed to upload converted JPEG to Notion")?;
                        attachment_children.push(image_block_json(&jpeg_upload_id));
                        attachment_block_meta.push("image".to_string());
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to convert HEIC to JPEG, skipping conversion");
                    }
                }

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
                attachment_children.push(file_block_json(&file_upload_id, &attachment.filename));
                attachment_block_meta.push("file".to_string());
            }
            FileType::Other => {
                let (data, content_type) = self.downloader.download(attachment).await?;

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
                attachment_children.push(file_block_json(&file_upload_id, &attachment.filename));
                attachment_block_meta.push("file".to_string());
            }
        }

        if matches!(file_type, FileType::Image | FileType::Heic)
            && is_spoiler_attachment(&attachment.filename)
        {
            let summary = spoiler_summary(attachment.description.as_deref());
            children.push(toggle_block_json(&summary, attachment_children));
            block_meta.push("toggle".to_string());
        } else {
            children.extend(attachment_children);
            block_meta.extend(attachment_block_meta);
        }

        Ok(())
    }
}
