//! Discord メッセージを Notion に同期する機能を提供する。

use anyhow::{Context as _, Result};
use handlebars::Handlebars;
use serde::Serialize;
use serenity::model::channel::{Attachment, Message};

use super::{DiaryStore, MessageBlock, NotionClient};

/// 同期結果の情報。
pub struct SyncResult {
    /// 同期が実行されたかどうか
    pub synced: bool,
    /// 作成されたブロック数
    pub block_count: usize,
}

/// テンプレートに渡すコンテキスト。
#[derive(Serialize)]
struct TemplateContext<'a> {
    /// メッセージ本文
    content: &'a str,
    /// 投稿者名
    author: &'a str,
    /// 投稿日時（ISO 8601 形式）
    timestamp: String,
}

/// メッセージを Notion に同期するためのシンクロナイザー。
pub struct MessageSyncer<'a> {
    /// Notion クライアント
    notion: &'a NotionClient,
    /// 日報ストア
    store: &'a DiaryStore,
    /// HTTP クライアント（画像ダウンロード用）
    http_client: reqwest::Client,
    /// メッセージフォーマット用テンプレート
    template: Handlebars<'a>,
}

impl<'a> MessageSyncer<'a> {
    /// 新しい MessageSyncer を作成する。
    ///
    /// # Arguments
    /// * `notion` - Notion クライアント
    /// * `store` - 日報ストア
    /// * `message_template` - メッセージフォーマット用 Handlebars テンプレート
    pub fn new(notion: &'a NotionClient, store: &'a DiaryStore, message_template: &str) -> Self {
        let mut template = Handlebars::new();
        // テンプレートのパースに失敗した場合はデフォルトテンプレートを使用
        if template
            .register_template_string("message", message_template)
            .is_err()
        {
            template
                .register_template_string("message", "{{content}}")
                .expect("Default template should be valid");
        }

        Self {
            notion,
            store,
            http_client: reqwest::Client::new(),
            template,
        }
    }

    /// メッセージ内容をテンプレートでフォーマットする。
    fn format_message(&self, message: &Message) -> String {
        let context = TemplateContext {
            content: &message.content,
            author: &message.author.name,
            timestamp: message.timestamp.to_rfc3339().unwrap_or_default(),
        };

        self.template
            .render("message", &context)
            .unwrap_or_else(|_| message.content.clone())
    }

    /// メッセージを Notion ページに同期する。
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

        let mut block_count = 0;
        let mut block_order = 0;

        // テキストコンテンツを同期
        if has_content {
            let formatted_content = self.format_message(message);
            let block_id = self
                .notion
                .append_text_block_with_id(page_id, &formatted_content)
                .await?;

            // DB にブロック情報を保存
            let message_block = MessageBlock {
                message_id: message.id.get(),
                block_id,
                block_type: "text".to_string(),
                block_order,
            };
            self.store.insert_message_block(&message_block).await?;

            block_count += 1;
            block_order += 1;
        }

        // 添付ファイルを同期
        for attachment in &message.attachments {
            let synced = self
                .sync_attachment_with_tracking(page_id, message.id.get(), attachment, block_order)
                .await?;
            block_count += synced;
            block_order += synced as i32;
        }

        Ok(SyncResult {
            synced: true,
            block_count,
        })
    }

    /// メッセージが更新されたときに Notion ブロックを更新する。
    ///
    /// テキストブロックのみ更新可能。画像ブロックは更新されない。
    pub async fn update_message(&self, message: &Message) -> Result<bool> {
        let blocks = self.store.get_blocks_by_message(message.id.get()).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // テキストブロックのみ更新
        let formatted_content = self.format_message(message);
        for block in blocks.iter().filter(|b| b.block_type == "text") {
            self.notion
                .update_text_block(&block.block_id, &formatted_content)
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

    /// 添付ファイルを Notion に同期し、ブロック情報を追跡する。
    ///
    /// 同期されたブロック数を返す（HEIC の場合は JPG 変換版と元ファイルで 2 つ）。
    async fn sync_attachment_with_tracking(
        &self,
        page_id: &str,
        message_id: u64,
        attachment: &Attachment,
        block_order: i32,
    ) -> Result<usize> {
        let file_type = classify_file(&attachment.filename);

        match file_type {
            FileType::Image => {
                let id = self.upload_image_with_id(page_id, attachment).await?;
                self.store_message_block(message_id, id, "image", block_order)
                    .await?;
                Ok(1)
            }
            FileType::Heic => {
                self.sync_heic_attachment(page_id, message_id, attachment, block_order)
                    .await
            }
            FileType::Other => {
                // その他のファイルはファイルブロックとしてアップロード
                let id = self.upload_file_with_id(page_id, attachment).await?;
                self.store_message_block(message_id, id, "file", block_order)
                    .await?;
                Ok(1)
            }
        }
    }

    /// HEIC ファイルを同期する。
    ///
    /// heic-support feature が有効な場合は JPG に変換してアップロードし、元の HEIC もアップロードする。
    /// 無効な場合は HEIC ファイルをそのままアップロードする。
    #[cfg(feature = "heic-support")]
    async fn sync_heic_attachment(
        &self,
        page_id: &str,
        message_id: u64,
        attachment: &Attachment,
        block_order: i32,
    ) -> Result<usize> {
        let mut block_count = 0;

        // JPG に変換してアップロード
        if let Some(id) = self
            .upload_heic_as_jpeg_with_id(page_id, attachment)
            .await?
        {
            self.store_message_block(message_id, id, "image", block_order + block_count as i32)
                .await?;
            block_count += 1;
        }

        // 元の HEIC ファイルもアップロード
        let id = self.upload_file_with_id(page_id, attachment).await?;
        self.store_message_block(message_id, id, "file", block_order + block_count as i32)
            .await?;
        block_count += 1;

        Ok(block_count)
    }

    /// HEIC ファイルを同期する（heic-support feature が無効な場合）。
    ///
    /// HEIC ファイルをそのままファイルブロックとしてアップロードする。
    #[cfg(not(feature = "heic-support"))]
    async fn sync_heic_attachment(
        &self,
        page_id: &str,
        message_id: u64,
        attachment: &Attachment,
        block_order: i32,
    ) -> Result<usize> {
        // HEIC ファイルをそのままアップロード
        let id = self.upload_file_with_id(page_id, attachment).await?;
        self.store_message_block(message_id, id, "file", block_order)
            .await?;
        Ok(1)
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

    /// 画像をダウンロードしてNotionにアップロードし、ブロック ID を返す。
    async fn upload_image_with_id(&self, page_id: &str, attachment: &Attachment) -> Result<String> {
        let (data, content_type) = self.download_attachment(attachment).await?;

        // Notion にアップロード
        let file_upload_id = self
            .notion
            .upload_file(&attachment.filename, &content_type, data)
            .await
            .context("Failed to upload image to Notion")?;

        // 画像ブロックを追加して ID を返す
        self.notion
            .append_uploaded_image_block_with_id(page_id, &file_upload_id)
            .await
            .context("Failed to append uploaded image block")
    }

    /// ファイルをダウンロードしてNotionにアップロードし、ブロック ID を返す。
    async fn upload_file_with_id(&self, page_id: &str, attachment: &Attachment) -> Result<String> {
        let (data, content_type) = self.download_attachment(attachment).await?;

        tracing::debug!(
            filename = %attachment.filename,
            content_type = %content_type,
            size = data.len(),
            "Uploading file to Notion"
        );

        // Notion にアップロード
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

        // ファイルブロックを追加して ID を返す
        self.notion
            .append_uploaded_file_block_with_id(page_id, &file_upload_id, &attachment.filename)
            .await
            .context("Failed to append uploaded file block")
    }

    /// HEIC ファイルを JPG に変換してNotionにアップロードし、ブロック ID を返す。
    ///
    /// 変換に失敗した場合は None を返す（元ファイルのみアップロードされる）。
    #[cfg(feature = "heic-support")]
    async fn upload_heic_as_jpeg_with_id(
        &self,
        page_id: &str,
        attachment: &Attachment,
    ) -> Result<Option<String>> {
        let (data, _content_type) = self.download_attachment(attachment).await?;

        // HEIC を JPEG に変換
        let jpeg_data = match convert_heic_to_jpeg(&data) {
            Ok(jpeg) => jpeg,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to convert HEIC to JPEG, skipping conversion");
                return Ok(None);
            }
        };

        // JPG ファイル名を生成
        let jpeg_filename = replace_extension(&attachment.filename, "jpg");

        // Notion にアップロード
        let file_upload_id = self
            .notion
            .upload_file(&jpeg_filename, "image/jpeg", jpeg_data)
            .await
            .context("Failed to upload converted JPEG to Notion")?;

        // 画像ブロックを追加して ID を返す
        let block_id = self
            .notion
            .append_uploaded_image_block_with_id(page_id, &file_upload_id)
            .await
            .context("Failed to append uploaded image block")?;

        Ok(Some(block_id))
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

/// ファイル名の拡張子から Content-Type を推定する。
fn guess_content_type(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    let content_type = if lower.ends_with(".heic") {
        "image/heic"
    } else if lower.ends_with(".heif") {
        "image/heif"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".mov") {
        "video/quicktime"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else {
        return None;
    };
    Some(content_type.to_string())
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
#[cfg(feature = "heic-support")]
fn replace_extension(filename: &str, new_ext: &str) -> String {
    if let Some(pos) = filename.rfind('.') {
        format!("{}.{}", &filename[..pos], new_ext)
    } else {
        format!("{}.{}", filename, new_ext)
    }
}

/// HEIC データを JPEG に変換する。
#[cfg(feature = "heic-support")]
fn convert_heic_to_jpeg(heic_data: &[u8]) -> Result<Vec<u8>> {
    use libheif_rs::{ColorSpace, HeifContext, RgbChroma};

    // HEIC コンテキストを作成
    let context = HeifContext::read_from_bytes(heic_data).context("Failed to read HEIC data")?;

    // プライマリ画像を取得
    let handle = context
        .primary_image_handle()
        .context("Failed to get primary image handle")?;

    // RGB にデコード
    let image = handle
        .decode(ColorSpace::Rgb(RgbChroma::Rgb), None)
        .context("Failed to decode HEIC image")?;

    // 画像データを取得
    let planes = image.planes();
    let interleaved = planes.interleaved.context("No interleaved plane found")?;

    let width = image.width() as u32;
    let height = image.height() as u32;

    // image クレートで JPEG にエンコード
    use image::{ImageBuffer, Rgb};

    let img: ImageBuffer<Rgb<u8>, _> =
        ImageBuffer::from_raw(width, height, interleaved.data.to_vec())
            .context("Failed to create image buffer")?;

    let mut jpeg_data = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_data);

    img.write_to(&mut cursor, image::ImageFormat::Jpeg)
        .context("Failed to encode JPEG")?;

    Ok(jpeg_data)
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
        assert_eq!(guess_content_type("archive.zip"), None);
        assert_eq!(guess_content_type("noextension"), None);
    }

    #[test]
    #[cfg(feature = "heic-support")]
    fn test_replace_extension() {
        assert_eq!(replace_extension("photo.heic", "jpg"), "photo.jpg");
        assert_eq!(replace_extension("image.HEIC", "jpg"), "image.jpg");
        assert_eq!(replace_extension("my.photo.heic", "jpg"), "my.photo.jpg");
        assert_eq!(replace_extension("noextension", "jpg"), "noextension.jpg");
    }

    #[test]
    fn test_template_default() {
        let mut template = Handlebars::new();
        template
            .register_template_string("message", "{{content}}")
            .unwrap();

        let context = TemplateContext {
            content: "Hello, world!",
            author: "testuser",
            timestamp: "2024-01-01T12:00:00+00:00".to_string(),
        };

        let result = template.render("message", &context).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_template_with_author() {
        let mut template = Handlebars::new();
        template
            .register_template_string("message", "{{author}}: {{content}}")
            .unwrap();

        let context = TemplateContext {
            content: "Hello, world!",
            author: "testuser",
            timestamp: "2024-01-01T12:00:00+00:00".to_string(),
        };

        let result = template.render("message", &context).unwrap();
        assert_eq!(result, "testuser: Hello, world!");
    }

    #[test]
    fn test_template_with_timestamp() {
        let mut template = Handlebars::new();
        template
            .register_template_string("message", "[{{timestamp}}] {{content}}")
            .unwrap();

        let context = TemplateContext {
            content: "Hello, world!",
            author: "testuser",
            timestamp: "2024-01-01T12:00:00+00:00".to_string(),
        };

        let result = template.render("message", &context).unwrap();
        assert_eq!(result, "[2024-01-01T12:00:00+00:00] Hello, world!");
    }

    #[test]
    fn test_template_with_all_variables() {
        let mut template = Handlebars::new();
        template
            .register_template_string("message", "[{{timestamp}}] {{author}}: {{content}}")
            .unwrap();

        let context = TemplateContext {
            content: "Hello, world!",
            author: "testuser",
            timestamp: "2024-01-01T12:00:00+00:00".to_string(),
        };

        let result = template.render("message", &context).unwrap();
        assert_eq!(
            result,
            "[2024-01-01T12:00:00+00:00] testuser: Hello, world!"
        );
    }
}
