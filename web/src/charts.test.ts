import { describe, expect, it } from 'vitest';
import { historyFixture } from './api/fixture.ts';
import type { HistoryResponse } from './api/schema.gen.ts';
import { chartGranularity, toChartRows } from './charts.ts';
import { addDays } from './range.ts';

type Day = HistoryResponse['days'][number];

/** `date` の日に、徒歩で `walkingM` メートル、移動 `movingS` 秒、`points` 点を記録した集計。 */
function day(date: string, walkingM: number, movingS: number, points: number): Day {
  return {
    date,
    summary: {
      distance_m: walkingM,
      distance_by_activity: { walking: walkingM, cycling: 0, automotive: 0, unknown: 0 },
      moving_s: movingS,
      stationary_s: 0,
      point_count: points,
      excluded_count: 0,
      first_at: null,
      last_at: null,
    },
  };
}

/** `from` から `count` 日ぶん、毎日 1 km と 1 時間と 10 点の集計を並べる。 */
function days(from: string, count: number): Day[] {
  return Array.from({ length: count }, (_, index) => day(addDays(from, index), 1000, 3600, 10));
}

describe('chartGranularity', () => {
  it('draws a bar per day up to about two months, then per week, then per month', () => {
    expect(chartGranularity(1)).toBe('day');
    expect(chartGranularity(62)).toBe('day');
    expect(chartGranularity(63)).toBe('week');
    expect(chartGranularity(366)).toBe('week');
    expect(chartGranularity(367)).toBe('month');
    expect(chartGranularity(3660)).toBe('month');
  });
});

describe('toChartRows', () => {
  it('converts daily summaries into chart rows in km, hours and counts', () => {
    expect(toChartRows(historyFixture.days, 'day')).toEqual([
      {
        from: '2026-09-01',
        to: '2026-09-01',
        label: '9/1',
        walking: 0.5,
        cycling: 0,
        automotive: 1,
        unknown: 0,
        movingHours: 0.3,
        stationaryHours: 0,
        points: 3,
        excluded: 1,
      },
    ]);
  });

  it('groups days into weeks starting on Monday and keeps partial weeks at the edges', () => {
    // 2026-09-02 は水曜日。9/2〜9/6、9/7〜9/13、9/14〜9/15 に分かれる
    const rows = toChartRows(days('2026-09-02', 14), 'week');

    expect(rows.map((row) => [row.from, row.to, row.label, row.points])).toEqual([
      ['2026-09-02', '2026-09-06', '9/2', 50],
      ['2026-09-07', '2026-09-13', '9/7', 70],
      ['2026-09-14', '2026-09-15', '9/14', 20],
    ]);
    expect(rows[1]?.walking).toBe(7);
    expect(rows[1]?.movingHours).toBe(7);
  });

  it('groups days into calendar months', () => {
    const rows = toChartRows(days('2026-09-29', 35), 'month');

    expect(rows.map((row) => [row.from, row.to, row.label, row.points])).toEqual([
      ['2026-09-29', '2026-09-30', '2026/9', 20],
      ['2026-10-01', '2026-10-31', '2026/10', 310],
      ['2026-11-01', '2026-11-02', '2026/11', 20],
    ]);
  });

  it('adds meters and seconds before rounding', () => {
    // 1 日 40 m (0.04 km) は日ごとに丸めると 0 だが、10 日で 0.4 km になる
    const rows = toChartRows(
      days('2026-09-07', 10).map((item) => day(item.date, 40, 0, 1)),
      'month',
    );

    expect(rows[0]?.walking).toBe(0.4);
  });

  it('reduces ten years of days to about 120 bars', () => {
    expect(toChartRows(days('2016-01-01', 3660), chartGranularity(3660))).toHaveLength(121);
  });
});
