-- 日次レポートは端末を問わず時刻の範囲で location を取り出す
CREATE INDEX IF NOT EXISTS idx_owntracks_messages_location_tst
    ON owntracks_messages(tst)
    WHERE msg_type = 'location';
