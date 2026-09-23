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

use kgd_application::{RecordLocationUseCase, RecordOutcome, ports::LocationRepository};
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
///
/// 実体は薄いシェルで、実際の永続化先を組み立てて [`import_files`] に委ねる。
pub async fn run_import(config: &Config, paths: &[PathBuf]) -> Result<()> {
    let files = collect_jsonl_files(paths)?;
    info!(files = files.len(), "Importing OwnTracks JSONL");

    let pool = connect_pool(&config.diary.database_url)
        .await
        .context("Failed to connect to database")?;
    let store: Arc<dyn LocationRepository> = Arc::new(LocationStore::new(pool));
    let use_case = RecordLocationUseCase::new(store);

    let total = import_files(&use_case, &files).await?;

    info!(
        imported = total.stored,
        existing = total.parsed.saturating_sub(total.stored),
        skipped = total.skipped,
        "Import finished"
    );

    Ok(())
}

/// 集めたファイル一覧を順に取り込み、件数を合算して返す。
///
/// `use_case` を受け取る形にすることで、実データベースに繋がずに
/// 識別子の受け渡しと集計をテストできるようにしている。
async fn import_files(
    use_case: &RecordLocationUseCase,
    files: &[PathBuf],
) -> Result<RecordOutcome> {
    let mut total = RecordOutcome {
        parsed: 0,
        stored: 0,
        skipped: 0,
    };

    for file in files {
        let outcome = import_file(use_case, file).await?;
        total.parsed += outcome.parsed;
        total.stored += outcome.stored;
        total.skipped += outcome.skipped;
    }

    Ok(total)
}

/// 1 ファイルぶんを読み込み、取り込みユースケースへ渡す。
///
/// 行単位で壊れているぶんは `parse_jsonl_line` の時点でスキップし件数に加える。
/// ドメイン変換で弾かれたぶん (`RecordOutcome::skipped`) と合算して返すため、
/// 呼び出し側は「壊れた行」を種類によらず 1 つの数字として扱える。
async fn import_file(use_case: &RecordLocationUseCase, file: &Path) -> Result<RecordOutcome> {
    // ディレクトリ名 <user>-<device> から端末を復元する。
    // 受信時は X-Limit-U / X-Limit-D から取るが、JSONL には残っていない。
    let (user_id, device_id) = device_from_path(file);

    let reader = BufReader::new(
        File::open(file).with_context(|| format!("Failed to open {}", file.display()))?,
    );

    let mut payloads = Vec::new();
    let mut line_skipped = 0usize;
    for line in reader.lines() {
        let line = line.with_context(|| format!("Failed to read {}", file.display()))?;
        match parse_jsonl_line(&line) {
            Some(value) => payloads.push(value),
            None => line_skipped += 1,
        }
    }

    let mut outcome = use_case
        .record(&user_id, &device_id, payloads)
        .await
        .with_context(|| format!("Failed to record messages from {}", file.display()))?;
    outcome.skipped += line_skipped;

    info!(
        file = %file.display(),
        user_id,
        device_id,
        imported = outcome.stored,
        existing = outcome.parsed.saturating_sub(outcome.stored),
        skipped = outcome.skipped,
        "Imported file"
    );

    Ok(outcome)
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
    use std::{fs, sync::Mutex};

    use kgd_domain::OwnTracksMessage;

    use super::*;

    /// 呼び出しを記録するだけの偽リポジトリ。
    ///
    /// `insert_messages` に渡された先頭メッセージの `user_id` / `device_id` と
    /// 件数を記録する。実データベースなしで「識別子が正しい順で record() に
    /// 届くか」「複数ファイルの件数が正しく合算されるか」を確認するため。
    #[derive(Default)]
    struct RecordingRepository {
        calls: Mutex<Vec<(String, String, usize)>>,
    }

    #[async_trait::async_trait]
    impl LocationRepository for RecordingRepository {
        async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
            if let Some(first) = messages.first() {
                self.calls.lock().unwrap().push((
                    first.user_id.clone(),
                    first.device_id.clone(),
                    messages.len(),
                ));
            }
            // 重複が無いものとして扱う (このテストの関心は識別子と集計)。
            Ok(messages.len())
        }
    }

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

    /// ディレクトリ名から復元した user_id / device_id が、
    /// 入れ替わらずに `record()` へ届くことを確認する。
    ///
    /// 呼び出し順を取り違えると全テストが通ったまま端末の識別を破壊するため
    /// (レビュー指摘)、実データベース無しで検証できるようにしている。
    #[tokio::test]
    async fn import_file_passes_user_and_device_id_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("ekuinox-ohtori");
        fs::create_dir_all(&nested).unwrap();
        let file = nested.join("2026-09-13.jsonl");
        fs::write(&file, r#"{"_type":"location","tst":1}"#).unwrap();

        let repo = Arc::new(RecordingRepository::default());
        let use_case = RecordLocationUseCase::new(repo.clone());

        let outcome = import_file(&use_case, &file).await.unwrap();

        assert_eq!(outcome.parsed, 1);
        assert_eq!(outcome.stored, 1);
        assert_eq!(outcome.skipped, 0);

        let calls = repo.calls.lock().unwrap();
        assert_eq!(
            *calls,
            vec![("ekuinox".to_string(), "ohtori".to_string(), 1)]
        );
    }

    /// 複数ファイルを取り込んだとき、件数 (取り込み・既存相当・スキップ) が
    /// ファイルをまたいで正しく合算されることを確認する。
    ///
    /// 壊れた行 (JSON として読めない行) とドメイン変換で弾かれる行の両方を
    /// 混ぜ、どちらもスキップ件数へ合算されることも合わせて確認する。
    #[tokio::test]
    async fn import_files_aggregates_counts_across_multiple_files() {
        let dir = tempfile::tempdir().unwrap();

        let first_dir = dir.path().join("alice-phone1");
        fs::create_dir_all(&first_dir).unwrap();
        let first_file = first_dir.join("2026-09-13.jsonl");
        fs::write(
            &first_file,
            "{\"_type\":\"location\",\"tst\":1}\n{\"_type\":\"location\",\"tst\":2}\n",
        )
        .unwrap();

        let second_dir = dir.path().join("bob-phone2");
        fs::create_dir_all(&second_dir).unwrap();
        let second_file = second_dir.join("2026-09-13.jsonl");
        fs::write(
            &second_file,
            "{\"_type\":\"location\",\"tst\":3}\n{not json\n{\"lat\":1.0}\n",
        )
        .unwrap();

        let repo = Arc::new(RecordingRepository::default());
        let use_case = RecordLocationUseCase::new(repo.clone());

        let files = collect_jsonl_files(&[dir.path().to_path_buf()]).unwrap();
        let total = import_files(&use_case, &files).await.unwrap();

        // file1: 2 行とも location として解釈できる (parsed=2, skipped=0)
        // file2: 1 行は location、`{not json` は行レベルでスキップ、
        //        `{"lat":1.0}` は _type が無くドメイン変換で弾かれてスキップ
        assert_eq!(total.parsed, 3);
        assert_eq!(total.stored, 3);
        assert_eq!(total.skipped, 2);

        let calls = repo.calls.lock().unwrap();
        assert_eq!(
            *calls,
            vec![
                ("alice".to_string(), "phone1".to_string(), 2),
                ("bob".to_string(), "phone2".to_string(), 1),
            ]
        );
    }
}
