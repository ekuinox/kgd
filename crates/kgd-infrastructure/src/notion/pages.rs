//! 日報ページの検索・作成。

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use notion_client::objects::{
    page::{PageProperty, SelectPropertyValue},
    parent::Parent,
    rich_text::{RichText, Text},
};
use reqwest::Method;

use super::{
    NotionClient,
    retry::ensure_success,
    types::{DatabaseQueryResponse, PageInfo},
};

impl NotionClient {
    /// 指定したタイトルの日報ページを検索し、存在すればページ ID と URL を返す。
    pub async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>> {
        let body = serde_json::json!({
            "filter": {
                "property": self.title_property,
                "title": {
                    "equals": title
                }
            },
            "page_size": 1
        });

        let response = self
            .request(
                Method::POST,
                format!("/databases/{}/query", self.database_id),
            )
            .json(&body)
            .send()
            .await
            .context("Failed to query database")?;

        let response = ensure_success(response, "Failed to query database").await?;

        let result: DatabaseQueryResponse = response
            .json()
            .await
            .context("Failed to parse database query response")?;

        Ok(result
            .results
            .first()
            .map(|page| (page.id.clone(), page.url.clone())))
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

        let response = self
            .request(Method::POST, "/pages")
            .json(&request)
            .send()
            .await
            .context("Failed to create Notion page")?;

        let response = ensure_success(response, "Failed to create Notion page").await?;

        let page: PageInfo = response
            .json()
            .await
            .context("Failed to parse create page response")?;

        Ok((page.id, page.url))
    }
}
