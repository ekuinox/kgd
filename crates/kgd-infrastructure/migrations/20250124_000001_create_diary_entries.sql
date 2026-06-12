-- 日報エントリテーブル
CREATE TABLE IF NOT EXISTS diary_entries (
    id SERIAL PRIMARY KEY,
    thread_id BIGINT NOT NULL UNIQUE,
    page_id TEXT NOT NULL,
    page_url TEXT NOT NULL,
    date TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 日付での検索用インデックス
CREATE INDEX IF NOT EXISTS idx_diary_entries_date ON diary_entries(date);
