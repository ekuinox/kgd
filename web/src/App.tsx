import { useEffect, useState } from 'react';
import styles from './App.module.css';
import { useHistory } from './api/useHistory.ts';
import { DailyCharts } from './components/DailyCharts.tsx';
import { RangePicker } from './components/RangePicker.tsx';
import { SummaryPanel } from './components/SummaryPanel.tsx';
import { TrackMap } from './components/TrackMap.tsx';
import { type DateRange, rangeFromSearch, rangeToSearch } from './range.ts';

/** 位置ログのビューア。期間は URL のクエリに持ち、再読み込みやブックマークでも同じ期間を開く。 */
export function App() {
  const [today] = useState(() => new Date());
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
              <DailyCharts
                days={data.days}
                onSelectDay={(date) => setRange({ from: date, to: date })}
              />
            </>
          )}
        </aside>
      </main>
    </div>
  );
}
