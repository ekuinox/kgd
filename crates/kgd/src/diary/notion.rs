//! Notion API との連携機能を提供する。

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use notion_client::{
    endpoints::{Client, blocks::append::request::AppendBlockChildrenRequest},
    objects::{
        block::{Block, BlockType, ImageValue, ParagraphValue},
        file::{ExternalFile, File},
        page::PageProperty,
        parent::Parent,
        rich_text::{RichText, Text},
    },
};

/// Notion API クライアントのラッパー。
pub struct NotionClient {
    /// notion-client のクライアント
    client: Client,
    /// 日報を保存するデータベース ID
    database_id: String,
}

impl NotionClient {
    /// 新しい NotionClient を作成する。
    pub fn new(token: impl Into<String>, database_id: impl Into<String>) -> Result<Self> {
        let client = Client::new(token.into(), None).context("Failed to create Notion client")?;
        Ok(Self {
            client,
            database_id: database_id.into(),
        })
    }

    /// 日報ページを作成し、ページ ID と URL を返す。
    pub async fn create_diary_page(&self, title: &str) -> Result<(String, String)> {
        let mut properties = BTreeMap::new();

        // タイトルプロパティを設定（データベースのタイトル列名に合わせる）
        properties.insert(
            "名前".to_string(),
            PageProperty::Title {
                id: None,
                title: vec![RichText::Text {
                    text: Text {
                        content: title.to_string(),
                        link: None,
                    },
                    annotations: None,
                    plain_text: None,
                    href: None,
                }],
            },
        );

        let request = notion_client::endpoints::pages::create::request::CreateAPageRequest {
            parent: Parent::DatabaseId {
                database_id: self.database_id.clone(),
            },
            properties,
            ..Default::default()
        };

        let page = self
            .client
            .pages
            .create_a_page(request)
            .await
            .context("Failed to create Notion page")?;

        Ok((page.id, page.url))
    }

    /// ページにテキストブロックを追加する。
    pub async fn append_text_block(&self, page_id: &str, text: &str) -> Result<()> {
        let block = Block {
            block_type: BlockType::Paragraph {
                paragraph: ParagraphValue {
                    rich_text: vec![RichText::Text {
                        text: Text {
                            content: text.to_string(),
                            link: None,
                        },
                        annotations: None,
                        plain_text: None,
                        href: None,
                    }],
                    color: None,
                    children: None,
                },
            },
            ..Default::default()
        };

        let request = AppendBlockChildrenRequest {
            children: vec![block],
            after: None,
        };

        self.client
            .blocks
            .append_block_children(page_id, request)
            .await
            .context("Failed to append block")?;

        Ok(())
    }

    /// ページに画像ブロックを追加する（外部URL）。
    pub async fn append_image_block(&self, page_id: &str, image_url: &str) -> Result<()> {
        let block = Block {
            block_type: BlockType::Image {
                image: ImageValue {
                    file_type: File::External {
                        external: ExternalFile {
                            url: image_url.to_string(),
                        },
                    },
                },
            },
            ..Default::default()
        };

        let request = AppendBlockChildrenRequest {
            children: vec![block],
            after: None,
        };

        self.client
            .blocks
            .append_block_children(page_id, request)
            .await
            .context("Failed to append image block")?;

        Ok(())
    }
}
