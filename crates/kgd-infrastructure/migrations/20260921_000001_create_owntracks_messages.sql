-- OwnTracks から受信した位置情報メッセージを保存するテーブル
CREATE TABLE IF NOT EXISTS owntracks_messages (
    id BIGSERIAL PRIMARY KEY,
    -- 端末が名乗るユーザー識別子 (X-Limit-U)
    user_id TEXT NOT NULL,
    -- 端末が名乗るデバイス識別子 (X-Limit-D)
    device_id TEXT NOT NULL,
    -- メッセージ種別 (_type)
    msg_type TEXT NOT NULL,
    -- 端末が位置を取得した時刻 (tst)
    tst TIMESTAMPTZ,
    -- 受信した時刻
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 緯度
    lat DOUBLE PRECISION,
    -- 経度
    lon DOUBLE PRECISION,
    -- 水平精度 (メートル)
    acc INTEGER,
    -- 高度 (メートル)
    alt INTEGER,
    -- 速度 (km/h)
    vel INTEGER,
    -- バッテリー残量 (%)
    batt SMALLINT,
    -- 送信のきっかけ (t)
    trigger_type TEXT,
    -- モーション判定の先頭要素 (motionactivities[0])
    motion TEXT,
    -- 受け取った JSON 全体
    payload JSONB NOT NULL
);

-- 同じメッセージの再送を弾く（端末は圏外復帰時に同じ点を送り直す）
CREATE UNIQUE INDEX IF NOT EXISTS idx_owntracks_messages_identity
    ON owntracks_messages(user_id, device_id, msg_type, tst);

-- 日次レポートは端末と日付で範囲を絞って取り出す
CREATE INDEX IF NOT EXISTS idx_owntracks_messages_device_tst
    ON owntracks_messages(user_id, device_id, tst);
