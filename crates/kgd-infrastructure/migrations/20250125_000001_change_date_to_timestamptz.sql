-- date カラムを TEXT から TIMESTAMPTZ に変更
-- 既存データは 'YYYY-MM-DD' 形式の文字列なので、00:00:00 UTC として変換

ALTER TABLE diary_entries
    ALTER COLUMN date TYPE TIMESTAMPTZ
    USING (date || 'T00:00:00Z')::TIMESTAMPTZ;
