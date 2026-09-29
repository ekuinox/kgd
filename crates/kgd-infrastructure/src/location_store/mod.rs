//! OwnTracks の受信メッセージを永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder, types::Json};

use kgd_application::ports::LocationRepository;
use kgd_domain::{Activity, OwnTracksMessage, TrackPoint};

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

    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>> {
        let rows: Vec<LocationRow> = sqlx::query_as(
            r#"
            SELECT tst, lat, lon, acc, motion
            FROM owntracks_messages
            WHERE msg_type = 'location'
              AND tst >= $1
              AND tst < $2
              AND lat IS NOT NULL
              AND lon IS NOT NULL
            ORDER BY tst
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch owntracks locations")?;

        Ok(rows.into_iter().map(to_track_point).collect())
    }
}

/// 位置の点として読み出す行 (tst, lat, lon, acc, motion)。
///
/// tst/lat/lon は列としては NULL 許容だが、上の WHERE 句 (tst >= $1 AND tst < $2 は
/// NULL を暗黙に、lat/lon IS NOT NULL は明示的に除外する) がそれらを常に非 NULL に
/// 絞り込んでいるため、ここでは Option を挟まず直接デコードできる。WHERE 句の条件を
/// 外すとデコードに失敗するようになる。
type LocationRow = (DateTime<Utc>, f64, f64, Option<i32>, Option<String>);

/// 読み出した行を軌跡の点へ変換する。
fn to_track_point((at, lat, lon, accuracy_m, motion): LocationRow) -> TrackPoint {
    TrackPoint {
        at,
        lat,
        lon,
        accuracy_m,
        activity: motion.as_deref().and_then(Activity::from_motion),
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

    /// 行の motion 文字列が移動種別へ変換され、精度がそのまま残ることを確認する。
    #[test]
    fn to_track_point_maps_motion_and_accuracy() {
        use chrono::{TimeZone as _, Utc};
        use kgd_domain::Activity;

        let at = Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap();

        let walking = to_track_point((at, 35.0, 139.0, Some(12), Some("walking".to_string())));
        let missing = to_track_point((at, 35.0, 139.0, None, None));

        assert_eq!(walking.activity, Some(Activity::Walking));
        assert_eq!(walking.accuracy_m, Some(12));
        assert_eq!(missing.activity, None);
        assert_eq!(missing.accuracy_m, None);
    }
}
