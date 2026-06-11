//! 日報ページの検索・作成。

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};
use notion_client::objects::{
    page::{PageProperty, SelectPropertyValue},
    parent::Parent,
    rich_text::{RichText, Text},
};

use super::{NOTION_API_VERSION, NotionClient, types::DatabaseQueryResponse};

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
            .http_client
            .post(format!(
                "https://api.notion.com/v1/databases/{}/query",
                self.database_id
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to query database")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to query database: {} - {}", status, body);
        }

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

        let page = self
            .client
            .pages
            .create_a_page(request)
            .await
            .context("Failed to create Notion page")?;

        Ok((page.id, page.url))
    }
}
