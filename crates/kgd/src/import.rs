//! JSONL に記録された OwnTracks メッセージの取り込み。

use std::{
    fs::{self, File},
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use serde_json::Value;
use tracing::info;

use kgd_application::{RecordLocationUseCase, ports::LocationRepository};
use kgd_domain::sanitize_identifier;
use kgd_infrastructure::{LocationStore, connect_pool};

use crate::config::Config;

/// 取り込み対象の JSONL ファイルを集める。
///
/// ディレクトリは再帰的に辿り、拡張子が `jsonl` のものだけを対象にする。
pub fn collect_jsonl_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        collect_into(path, &mut files)
            .with_context(|| format!("Failed to scan {}", path.display()))?;
    }
    files.sort();
    Ok(files)
}

fn collect_into(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_into(&entry?.path(), files)?;
        }
    } else if path.extension().is_some_and(|ext| ext == "jsonl") {
        files.push(path.to_path_buf());
    }
    Ok(())
}

/// JSONL の 1 行を JSON として解釈する。空行や壊れた行は `None` を返す。
pub fn parse_jsonl_line(line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// JSONL を読み込んでデータベースへ取り込む。
pub async fn run_import(config: &Config, paths: &[PathBuf]) -> Result<()> {
    let files = collect_jsonl_files(paths)?;
    info!(files = files.len(), "Importing OwnTracks JSONL");

    let pool = connect_pool(&config.diary.database_url)
        .await
        .context("Failed to connect to database")?;
    let store: Arc<dyn LocationRepository> = Arc::new(LocationStore::new(pool));
    let use_case = RecordLocationUseCase::new(store);

    let mut parsed_total = 0usize;
    let mut stored_total = 0usize;
    let mut skipped_total = 0usize;

    for file in files {
        // ディレクトリ名 <user>-<device> から端末を復元する。
        // 受信時は X-Limit-U / X-Limit-D から取るが、JSONL には残っていない。
        let (user_id, device_id) = device_from_path(&file);

        let reader = BufReader::new(
            File::open(&file).with_context(|| format!("Failed to open {}", file.display()))?,
        );

        let mut payloads = Vec::new();
        for line in reader.lines() {
            let line = line.with_context(|| format!("Failed to read {}", file.display()))?;
            match parse_jsonl_line(&line) {
                Some(value) => payloads.push(value),
                None => skipped_total += 1,
            }
        }

        let outcome = use_case.record(&user_id, &device_id, payloads).await?;
        parsed_total += outcome.parsed;
        stored_total += outcome.stored;
        skipped_total += outcome.skipped;

        info!(
            file = %file.display(),
            user_id,
            device_id,
            parsed = outcome.parsed,
            stored = outcome.stored,
            "Imported file"
        );
    }

    info!(
        parsed = parsed_total,
        stored = stored_total,
        skipped = skipped_total,
        "Import finished"
    );

    Ok(())
}

/// `<user>-<device>/<date>.jsonl` の親ディレクトリ名から端末を復元する。
fn device_from_path(path: &Path) -> (String, String) {
    let directory = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    match directory.split_once('-') {
        Some((user, device)) => (
            sanitize_identifier(Some(user), "unknown"),
            sanitize_identifier(Some(device), "device"),
        ),
        None => ("unknown".to_string(), "device".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// ディレクトリを渡すと配下の .jsonl を再帰的に集め、
    /// それ以外の拡張子を無視することを確認する。
    #[test]
    fn collect_jsonl_files_walks_directories() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("ekuinox-ohtori");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("2026-09-13.jsonl"), "").unwrap();
        fs::write(nested.join("README.md"), "").unwrap();

        let files = collect_jsonl_files(&[dir.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("2026-09-13.jsonl"));
    }

    /// ファイルを直接渡した場合はそのまま対象になることを確認する。
    #[test]
    fn collect_jsonl_files_accepts_direct_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.jsonl");
        fs::write(&file, "").unwrap();

        let files = collect_jsonl_files(std::slice::from_ref(&file)).unwrap();

        assert_eq!(files, vec![file]);
    }

    /// 空行と壊れた行を読み飛ばし、正しい行だけを返すことを確認する。
    ///
    /// 途中で壊れた行があっても取り込みを止めないため。
    #[test]
    fn parse_jsonl_line_skips_blank_and_broken_lines() {
        assert!(parse_jsonl_line("").is_none());
        assert!(parse_jsonl_line("   ").is_none());
        assert!(parse_jsonl_line("{not json").is_none());
        assert!(parse_jsonl_line(r#"{"_type":"location"}"#).is_some());
    }

    /// ディレクトリ名 <user>-<device> から端末を復元することを確認する。
    ///
    /// JSONL には X-Limit-U / X-Limit-D が残っていないため、
    /// 受け口が付けたディレクトリ名が唯一の手がかりになる。
    #[test]
    fn device_from_path_splits_directory_name() {
        let path = PathBuf::from("data/ekuinox-ohtori/2026-09-13.jsonl");
        assert_eq!(
            device_from_path(&path),
            ("ekuinox".to_string(), "ohtori".to_string())
        );
    }

    /// 区切りが無いディレクトリ名では既定値を返すことを確認する。
    #[test]
    fn device_from_path_falls_back_without_separator() {
        let path = PathBuf::from("data/plain/2026-09-13.jsonl");
        assert_eq!(
            device_from_path(&path),
            ("unknown".to_string(), "device".to_string())
        );
    }
}
