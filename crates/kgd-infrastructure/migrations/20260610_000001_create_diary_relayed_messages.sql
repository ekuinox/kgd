-- 書き込み用チャンネルの元メッセージと、日報スレッドへ転記したメッセージの対応を管理するテーブル
CREATE TABLE IF NOT EXISTS diary_relayed_messages (
    id SERIAL PRIMARY KEY,
    -- 書き込み用チャンネルの元メッセージ ID
    source_message_id BIGINT NOT NULL,
    -- 転記先の日報スレッド ID
    thread_id BIGINT NOT NULL,
    -- 転記先スレッドに作成された転記メッセージ ID
    relayed_message_id BIGINT NOT NULL,
    -- 作成日時
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 元メッセージ ID は一意（編集・削除の追従で 1:1 対応を保つ）
CREATE UNIQUE INDEX IF NOT EXISTS idx_diary_relayed_messages_source
    ON diary_relayed_messages(source_message_id);
