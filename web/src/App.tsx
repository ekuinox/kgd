import { useEffect, useState } from 'react';
import styles from './App.module.css';
import { useCalendar } from './api/useCalendar.ts';
import { useHistory } from './api/useHistory.ts';
import { DailyCharts } from './components/DailyCharts.tsx';
import { RangePicker } from './components/RangePicker.tsx';
import { SummaryPanel } from './components/SummaryPanel.tsx';
import { TrackMap } from './components/TrackMap.tsx';
import { type DateRange, rangeFromSearch, rangeToSearch } from './range.ts';
import { useToday } from './useToday.ts';

/**
 * 位置ログのビューア。
 *
 * 「今日」はサーバーと同じ暦で決めるため、先にサーバーの暦を取得してから画面を出す。
 */
export function App() {
  const calendar = useCalendar();

  if (calendar.timezone === null) {
    return (
      <div className={styles.app}>
        <header className={styles.header}>
          <h1>位置ログ</h1>
        </header>
        {calendar.error ? (
          <p className={styles.error}>読み込めませんでした: {calendar.error}</p>
        ) : (
          <div className={styles.status}>読み込み中…</div>
        )}
      </div>
    );
  }
  return <Viewer timezone={calendar.timezone} />;
}

/** 期間は URL のクエリに持ち、再読み込みやブックマークでも同じ期間を開く。 */
function Viewer({ timezone }: { timezone: string }) {
  const today = useToday(timezone);
  const [range, setRange] = useState<DateRange>(() =>
    rangeFromSearch(window.location.search, today),
  );
  const { data, loading, error } = useHistory(range);

  useEffect(() => {
    const search = rangeToSearch(range);
    if (search !== window.location.search) {
      window.history.replaceState(null, '', search);
    }
  }, [range]);

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <h1>位置ログ</h1>
        <RangePicker range={range} today={today} onChange={setRange} />
        {loading && <span>読み込み中…</span>}
      </header>
      {error && <p className={styles.error}>読み込めませんでした: {error}</p>}
      <main className={styles.main}>
        <div className={styles.map}>
          {data ? (
            <TrackMap track={data.track} meta={data.track_meta} />
          ) : (
            <div className={styles.status}>
              {loading ? '読み込み中…' : '表示できるデータがありません'}
            </div>
          )}
        </div>
        <aside className={styles.side}>
          {data && (
            <>
              <SummaryPanel summary={data.total} timezone={data.range.timezone} />
              <DailyCharts days={data.days} onSelectRange={setRange} />
            </>
          )}
        </aside>
      </main>
    </div>
  );
}
