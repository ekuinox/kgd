//! スレッドと Notion ページの紐付け情報を永続化するストア。

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 日報エントリの情報。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiaryEntry {
    /// Discord スレッド ID
    pub thread_id: u64,
    /// Notion ページ ID
    pub page_id: String,
    /// Notion ページ URL
    pub page_url: String,
    /// 日付 (YYYY-MM-DD 形式)
    pub date: String,
    /// 作成日時
    pub created_at: DateTime<Utc>,
}

/// スレッドと Notion ページの紐付け情報を管理するストア。
pub struct DiaryStore {
    /// 永続化ファイルのパス
    path: PathBuf,
    /// スレッドID -> エントリ のマッピング
    entries: HashMap<u64, DiaryEntry>,
}

impl DiaryStore {
    /// ストアを読み込む。ファイルが存在しない場合は空のストアを作成する。
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let entries = if path.exists() {
            let content = fs::read_to_string(&path).context("Failed to read diary store")?;
            serde_json::from_str(&content).context("Failed to parse diary store")?
        } else {
            HashMap::new()
        };
        Ok(Self { path, entries })
    }

    /// ストアをファイルに保存する。
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.entries)
            .context("Failed to serialize diary store")?;
        fs::write(&self.path, content).context("Failed to write diary store")?;
        Ok(())
    }

    /// エントリを追加する。
    pub fn insert(&mut self, entry: DiaryEntry) -> Result<()> {
        self.entries.insert(entry.thread_id, entry);
        self.save()
    }

    /// スレッド ID からエントリを取得する。
    pub fn get_by_thread(&self, thread_id: u64) -> Option<&DiaryEntry> {
        self.entries.get(&thread_id)
    }

    /// 日付からエントリを取得する。
    pub fn get_by_date(&self, date: &str) -> Option<&DiaryEntry> {
        self.entries.values().find(|e| e.date == date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    #[test]
    fn load_empty_store() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path();
        std::fs::remove_file(path).ok();

        let store = DiaryStore::load(path).unwrap();
        assert!(store.entries.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        std::fs::remove_file(&path).ok();

        let mut store = DiaryStore::load(&path).unwrap();

        let entry = DiaryEntry {
            thread_id: 123,
            page_id: "page-123".to_string(),
            page_url: "https://notion.so/page-123".to_string(),
            date: "2024-01-01".to_string(),
            created_at: Utc::now(),
        };

        store.insert(entry.clone()).unwrap();

        assert!(store.get_by_thread(123).is_some());
        assert!(store.get_by_date("2024-01-01").is_some());
        assert!(store.get_by_thread(999).is_none());
    }

    #[test]
    fn load_existing_store() {
        let mut temp = NamedTempFile::new().unwrap();
        let json = r#"{"123":{"thread_id":123,"page_id":"p1","page_url":"https://notion.so/p1","date":"2024-01-01","created_at":"2024-01-01T00:00:00Z"}}"#;
        temp.write_all(json.as_bytes()).unwrap();

        let store = DiaryStore::load(temp.path()).unwrap();
        assert_eq!(store.entries.len(), 1);
        assert!(store.get_by_thread(123).is_some());
    }
}
