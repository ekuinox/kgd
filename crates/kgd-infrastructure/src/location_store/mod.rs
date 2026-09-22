//! OwnTracks の受信メッセージを永続化するストア。

use anyhow::{Context as _, Result};
use sqlx::{PgPool, Postgres, QueryBuilder, types::Json};

use kgd_application::ports::LocationRepository;
use kgd_domain::OwnTracksMessage;

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
}

#[async_trait::async_trait]
impl LocationRepository for LocationStore {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }

        // 端末はバッチで送ってくるため、1 文へまとめて往復を減らす。
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO owntracks_messages \
             (user_id, device_id, msg_type, tst, lat, lon, acc, alt, vel, batt, trigger_type, motion, payload) ",
        );

        builder.push_values(messages, |mut row, message| {
            row.push_bind(&message.user_id)
                .push_bind(&message.device_id)
                .push_bind(&message.msg_type)
                .push_bind(message.tst)
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
