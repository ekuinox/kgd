-- 書き込み用チャンネルの元メッセージと、日報スレッドへ転送したメッセージの対応を管理するテーブル
CREATE TABLE IF NOT EXISTS diary_forwarded_messages (
    id SERIAL PRIMARY KEY,
    -- 書き込み用チャンネルの元メッセージ ID
    source_message_id BIGINT NOT NULL,
    -- 転送先の日報スレッド ID
    thread_id BIGINT NOT NULL,
    -- 転送先スレッドに作成された転送メッセージ ID
    forwarded_message_id BIGINT NOT NULL,
    -- 作成日時
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 元メッセージ ID は一意（編集・削除の追従で 1:1 対応を保つ）
CREATE UNIQUE INDEX IF NOT EXISTS idx_diary_forwarded_messages_source
    ON diary_forwarded_messages(source_message_id);
