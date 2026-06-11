//! Notion へのファイルアップロード。

use anyhow::{Context as _, Result, bail};
use reqwest::multipart;

use super::{
    NOTION_API_VERSION, NotionClient,
    types::{CreateFileUploadRequest, FileUploadResponse},
};

impl NotionClient {
    /// ファイルをNotionにアップロードし、ファイルアップロードIDを返す。
    pub async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String> {
        // 1. Create file upload
        let create_request = CreateFileUploadRequest {
            mode: "single_part".to_string(),
            filename: filename.to_string(),
            content_type: content_type.to_string(),
        };

        let create_response = self
            .http_client
            .post("https://api.notion.com/v1/file_uploads")
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .json(&create_request)
            .send()
            .await
            .context("Failed to create file upload")?;

        if !create_response.status().is_success() {
            let status = create_response.status();
            let body = create_response.text().await.unwrap_or_default();
            bail!("Failed to create file upload: {} - {}", status, body);
        }

        let file_upload: FileUploadResponse = create_response
            .json()
            .await
            .context("Failed to parse file upload response")?;

        let file_upload_id = file_upload.id;

        // 2. Send file content
        let part = multipart::Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(content_type)
            .context("Invalid content type")?;

        let form = multipart::Form::new().part("file", part);

        let send_response = self
            .http_client
            .post(format!(
                "https://api.notion.com/v1/file_uploads/{}/send",
                file_upload_id
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .multipart(form)
            .send()
            .await
            .context("Failed to send file upload")?;

        if !send_response.status().is_success() {
            let status = send_response.status();
            let body = send_response.text().await.unwrap_or_default();
            bail!("Failed to send file upload: {} - {}", status, body);
        }

        let upload_result: FileUploadResponse = send_response
            .json()
            .await
            .context("Failed to parse send response")?;

        if upload_result.status != "uploaded" {
            bail!(
                "File upload not completed: status = {}",
                upload_result.status
            );
        }

        Ok(file_upload_id)
    }
}
