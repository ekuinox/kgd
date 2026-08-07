//! Notion へのファイルアップロード。

use anyhow::{Context as _, Result, bail};
use reqwest::{Method, multipart};

use super::{
    NotionClient,
    retry::ensure_success,
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
            .request(Method::POST, "/file_uploads")
            .json(&create_request)
            .send()
            .await
            .context("Failed to create file upload")?;

        let create_response =
            ensure_success(create_response, "Failed to create file upload").await?;

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
            .request(
                Method::POST,
                format!("/file_uploads/{}/send", file_upload_id),
            )
            .multipart(form)
            .send()
            .await
            .context("Failed to send file upload")?;

        let send_response = ensure_success(send_response, "Failed to send file upload").await?;

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
