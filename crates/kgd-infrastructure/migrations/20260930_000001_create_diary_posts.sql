-- bot が日報日のスレッドと Notion ページへ載せた投稿の進み具合
CREATE TABLE IF NOT EXISTS diary_posts (
    -- 投稿を一意に識別するキー (例: location-report:2026-09-28)
    post_key TEXT PRIMARY KEY,
    -- 載せる先の日報日
    diary_date DATE NOT NULL,
    -- Notion ページへ載せた時刻
    notion_posted_at TIMESTAMPTZ,
    -- スレッドへ投稿したメッセージ ID
    thread_message_id BIGINT,
    -- スレッドへ投稿した時刻
    thread_posted_at TIMESTAMPTZ,
    -- 日報が無いためスキップした時刻
    skipped_at TIMESTAMPTZ
);
