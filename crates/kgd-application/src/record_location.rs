//! OwnTracks のメッセージを受け取って永続化するユースケース。

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tracing::warn;

use kgd_domain::{OwnTracksMessage, parse_owntracks_message};

use super::ports::LocationRepository;

/// 受信 1 回ぶんの処理結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOutcome {
    /// 解釈できたメッセージ数
    pub parsed: usize,
    /// 実際に保存された件数 (重複を除く)
    pub stored: usize,
    /// 解釈できず捨てた件数
    pub skipped: usize,
}

/// OwnTracks のメッセージを受け取って永続化するユースケース。
pub struct RecordLocationUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
}

impl RecordLocationUseCase {
    /// 新しい RecordLocationUseCase を作成する。
    pub fn new(repo: Arc<dyn LocationRepository>) -> Self {
        Self { repo }
    }

    /// 受け取った JSON 群を解釈して保存する。
    ///
    /// 解釈できない要素は捨てて処理を続ける。端末は複数メッセージを 1 度に
    /// 送ることがあり、1 件の異常でバッチ全体を失わないようにするため。
    pub async fn record(
        &self,
        user_id: &str,
        device_id: &str,
        payloads: Vec<Value>,
    ) -> Result<RecordOutcome> {
        let total = payloads.len();
        let messages: Vec<OwnTracksMessage> = payloads
            .into_iter()
            .filter_map(|payload| parse_owntracks_message(user_id, device_id, payload))
            .collect();

        let parsed = messages.len();
        let skipped = total - parsed;
        if skipped > 0 {
            warn!(
                skipped,
                user_id, device_id, "Skipped unparsable OwnTracks payloads"
            );
        }

        if messages.is_empty() {
            return Ok(RecordOutcome {
                parsed: 0,
                stored: 0,
                skipped,
            });
        }

        let stored = self.repo.insert_messages(&messages).await?;

        Ok(RecordOutcome {
            parsed,
            stored,
            skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::ports::MockLocationRepository;

    use super::*;

    /// 複数のメッセージをまとめて渡すと 1 回の呼び出しで保存され、
    /// 保存件数が結果に反映されることを確認する。
    #[tokio::test]
    async fn record_stores_all_parsed_messages_in_one_call() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 2)
            .times(1)
            .returning(|messages| Ok(messages.len()));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1, "lat": 1.0, "lon": 2.0 }),
            json!({ "_type": "location", "tst": 2, "lat": 1.5, "lon": 2.5 }),
        ];

        let outcome = use_case
            .record("ekuinox", "ohtori", payloads)
            .await
            .unwrap();

        assert_eq!(outcome.parsed, 2);
        assert_eq!(outcome.stored, 2);
        assert_eq!(outcome.skipped, 0);
    }

    /// 解釈できない JSON が混ざっていても、解釈できたぶんだけ保存し、
    /// スキップ件数を返すことを確認する。
    ///
    /// 1 件の異常で端末のバッチ送信全体を落とさないため。
    #[tokio::test]
    async fn record_skips_unparsable_payloads() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 1)
            .times(1)
            .returning(|messages| Ok(messages.len()));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1 }),
            json!({ "lat": 1.0 }),
            json!("plain string"),
        ];

        let outcome = use_case
            .record("ekuinox", "ohtori", payloads)
            .await
            .unwrap();

        assert_eq!(outcome.parsed, 1);
        assert_eq!(outcome.stored, 1);
        assert_eq!(outcome.skipped, 2);
    }

    /// すべて解釈できなかった場合はリポジトリを呼ばないことを確認する。
    #[tokio::test]
    async fn record_does_not_touch_repository_when_nothing_parsed() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages().times(0);

        let use_case = RecordLocationUseCase::new(Arc::new(repo));

        let outcome = use_case
            .record("ekuinox", "ohtori", vec![json!({ "lat": 1.0 })])
            .await
            .unwrap();

        assert_eq!(outcome.parsed, 0);
        assert_eq!(outcome.stored, 0);
        assert_eq!(outcome.skipped, 1);
    }

    /// 重複で挿入されなかった件数が stored に含まれないことを確認する。
    ///
    /// 端末は圏外復帰時に同じ点を送り直すため、再送を「保存した」と数えない。
    #[tokio::test]
    async fn record_reports_stored_count_from_repository() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 2)
            .times(1)
            .returning(|_| Ok(1));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1 }),
            json!({ "_type": "location", "tst": 1 }),
        ];

        let outcome = use_case
            .record("ekuinox", "ohtori", payloads)
            .await
            .unwrap();

        assert_eq!(outcome.parsed, 2);
        assert_eq!(outcome.stored, 1);
    }
}
