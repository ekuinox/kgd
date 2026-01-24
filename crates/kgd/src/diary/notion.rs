//! Notion API との連携機能を提供する。

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use notion_client::{
    endpoints::{Client, blocks::append::request::AppendBlockChildrenRequest},
    objects::{
        block::{Block, BlockType, ImageValue, ParagraphValue},
        file::{ExternalFile, File},
        page::{PageProperty, SelectPropertyValue},
        parent::Parent,
        rich_text::{RichText, Text},
    },
};

use crate::config::NotionTagConfig;

/// Notion API クライアントのラッパー。
pub struct NotionClient {
    /// notion-client のクライアント
    client: Client,
    /// 日報を保存するデータベース ID
    database_id: String,
    /// タイトルプロパティ名
    title_property: String,
    /// ページ作成時に設定するタグ
    tags: Vec<NotionTagConfig>,
}

impl NotionClient {
    /// 新しい NotionClient を作成する。
    pub fn new(
        token: impl Into<String>,
        database_id: impl Into<String>,
        title_property: impl Into<String>,
        tags: Vec<NotionTagConfig>,
    ) -> Result<Self> {
        let client = Client::new(token.into(), None).context("Failed to create Notion client")?;
        Ok(Self {
            client,
            database_id: database_id.into(),
            title_property: title_property.into(),
            tags,
        })
    }

    /// 日報ページを作成し、ページ ID と URL を返す。
    pub async fn create_diary_page(&self, title: &str) -> Result<(String, String)> {
        let mut properties = BTreeMap::new();

        // タイトルプロパティを設定
        properties.insert(
            self.title_property.clone(),
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

        // タグ（セレクト/マルチセレクトプロパティ）を設定
        for tag in &self.tags {
            let select_value = SelectPropertyValue {
                id: None,
                name: Some(tag.value.clone()),
                color: None,
            };
            let property = if tag.multi_select {
                PageProperty::MultiSelect {
                    id: None,
                    multi_select: vec![select_value],
                }
            } else {
                PageProperty::Select {
                    id: None,
                    select: Some(select_value),
                }
            };
            properties.insert(tag.property.clone(), property);
        }

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
