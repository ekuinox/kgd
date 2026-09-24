//! OwnTracks の受信メッセージを永続化するストア。

use anyhow::{Context as _, Result};
use sqlx::{PgPool, Postgres, QueryBuilder, types::Json};

use kgd_application::ports::LocationRepository;
use kgd_domain::OwnTracksMessage;

/// 1 回の INSERT に含めるメッセージ件数の上限。
///
/// Postgres の拡張プロトコルは 1 クエリあたり最大 65535 個のバインド
/// パラメータまでしか受け付けない。1 行 14 列なので、この値を超える
/// バッチは複数の INSERT 文に分けて実行する。HTTP 受信・JSONL 取り込み
/// のどちらも大きなバッチを送りうるため、呼び出し側ではなくここで守る。
const INSERT_CHUNK_SIZE: usize = 1000;

/// OwnTracks の受信メッセージを管理するストア。
#[derive(Clone)]
pub struct LocationStore {
    pool: PgPool,
}

impl LocationStore {
    /// 既存のプールからストアを作る。
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// バインドパラメータ上限に収まる 1 チャンクぶんを INSERT する。
    async fn insert_chunk(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }

        // 端末はバッチで送ってくるため、1 文へまとめて往復を減らす。
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO owntracks_messages \
             (user_id, device_id, msg_type, tst, received_at, lat, lon, acc, alt, vel, batt, trigger_type, motion, payload) ",
        );

        builder.push_values(messages, |mut row, message| {
            row.push_bind(&message.user_id)
                .push_bind(&message.device_id)
                .push_bind(&message.msg_type)
                .push_bind(message.tst)
                // received_at は tst と同様に欠けうる (例: waypoints の JSONL 取り込み)。
                // 列は NOT NULL なので、無ければ DB 側で受信 (実行) 時刻を採る。
                .push("COALESCE(")
                .push_bind_unseparated(message.received_at)
                .push_unseparated(", NOW())")
                .push_bind(message.lat)
                .push_bind(message.lon)
                .push_bind(message.acc)
                .push_bind(message.alt)
                .push_bind(message.vel)
                .push_bind(message.batt)
                .push_bind(&message.trigger_type)
                .push_bind(&message.motion)
                .push_bind(Json(&message.payload));
        });

        builder.push(" ON CONFLICT (user_id, device_id, msg_type, tst) DO NOTHING");

        let result = builder
            .build()
            .execute(&self.pool)
            .await
            .context("Failed to insert owntracks messages")?;

        Ok(result.rows_affected() as usize)
    }
}

#[async_trait::async_trait]
impl LocationRepository for LocationStore {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        let mut affected = 0usize;
        for chunk in chunk_messages(messages) {
            affected += self.insert_chunk(chunk).await?;
        }
        Ok(affected)
    }
}

/// バインドパラメータ上限に収まるチャンクへ分割する。
///
/// DB を伴わない純粋な分割ロジックとして切り出し、境界値をユニットテストで
/// 確認できるようにしている。
fn chunk_messages(messages: &[OwnTracksMessage]) -> impl Iterator<Item = &[OwnTracksMessage]> {
    messages.chunks(INSERT_CHUNK_SIZE)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// テスト用の最小限の `OwnTracksMessage` を作る。
    fn dummy_message(tst_seconds: i64) -> OwnTracksMessage {
        use chrono::{TimeZone as _, Utc};

        OwnTracksMessage {
            user_id: "u".to_string(),
            device_id: "d".to_string(),
            msg_type: "location".to_string(),
            tst: Utc.timestamp_opt(tst_seconds, 0).single(),
            received_at: Utc.timestamp_opt(tst_seconds, 0).single(),
            lat: None,
            lon: None,
            acc: None,
            alt: None,
            vel: None,
            batt: None,
            trigger_type: None,
            motion: None,
            payload: json!({ "_type": "location", "tst": tst_seconds }),
        }
    }

    /// 上限を超える件数は複数チャンクに分割され、最後のチャンクだけ端数になることを確認する。
    ///
    /// バインドパラメータ上限を超えて 1 文で INSERT しようとすると DB エラーになるため、
    /// チャンク化が確実に効いていることを分割件数で検証する。
    #[test]
    fn chunk_messages_splits_batches_larger_than_the_limit() {
        let messages: Vec<OwnTracksMessage> = (0..(INSERT_CHUNK_SIZE * 2 + 500) as i64)
            .map(dummy_message)
            .collect();

        let sizes: Vec<usize> = chunk_messages(&messages).map(<[_]>::len).collect();

        assert_eq!(sizes, vec![INSERT_CHUNK_SIZE, INSERT_CHUNK_SIZE, 500]);
    }

    /// 上限未満の件数は 1 チャンクのままであることを確認する。
    #[test]
    fn chunk_messages_keeps_small_batches_in_a_single_chunk() {
        let messages: Vec<OwnTracksMessage> = (0..3).map(dummy_message).collect();

        let sizes: Vec<usize> = chunk_messages(&messages).map(<[_]>::len).collect();

        assert_eq!(sizes, vec![3]);
    }

    /// 空のバッチではチャンクが 1 つも生成されないことを確認する。
    #[test]
    fn chunk_messages_yields_nothing_for_empty_input() {
        let messages: Vec<OwnTracksMessage> = Vec::new();

        assert_eq!(chunk_messages(&messages).count(), 0);
    }
}
