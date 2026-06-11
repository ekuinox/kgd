-- メッセージと Notion ブロックの対応を管理するテーブル
CREATE TABLE diary_message_blocks (
    id SERIAL PRIMARY KEY,
    -- Discord メッセージ ID
    message_id BIGINT NOT NULL,
    -- Notion ブロック ID
    block_id TEXT NOT NULL,
    -- ブロックの種類（text, image, link）
    block_type TEXT NOT NULL,
    -- 同一メッセージ内でのブロックの順序
    block_order INT NOT NULL DEFAULT 0,
    -- 作成日時
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- メッセージ ID でのインデックス（削除・更新時の検索用）
CREATE INDEX idx_diary_message_blocks_message_id ON diary_message_blocks(message_id);

-- ブロック ID でのユニーク制約
CREATE UNIQUE INDEX idx_diary_message_blocks_block_id ON diary_message_blocks(block_id);
