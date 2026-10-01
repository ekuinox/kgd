import type { HistoryResponse } from './api/schema.gen.ts';
import { formatShortDate } from './format.ts';

/** グラフの 1 本の棒にまとめる長さ。 */
export type ChartGranularity = 'day' | 'week' | 'month';

/**
 * グラフの 1 行 (1 本の棒)。`from` から `to` まで (どちらも含む) の日を足し合わせる。
 * 距離は km、時間は時間で持つ (どちらも小数 1 桁)。
 */
export type ChartRow = {
  from: string;
  to: string;
  label: string;
  walking: number;
  cycling: number;
  automotive: number;
  unknown: number;
  movingHours: number;
  stationaryHours: number;
  points: number;
  excluded: number;
};

/** 日ごとに棒を描く日数の上限。これを超えたら週ごとにまとめる (約 2 か月)。 */
const MAX_DAILY_BARS = 62;
/** 週ごとに棒を描く日数の上限。これを超えたら月ごとにまとめる (約 1 年)。 */
const MAX_WEEKLY_DAYS = 366;

/**
 * 日数から、棒にまとめる長さを決める。
 *
 * 長い期間を 1 日 1 本で描くと数千本の棒になり、スマートフォンで描画が重くなるため。
 */
export function chartGranularity(dayCount: number): ChartGranularity {
  if (dayCount <= MAX_DAILY_BARS) {
    return 'day';
  }
  return dayCount <= MAX_WEEKLY_DAYS ? 'week' : 'month';
}

/**
 * 日ごとの集計を、`granularity` の長さごとに足し合わせたグラフの行にする。
 *
 * 週は月曜日から始める。期間の端の週や月は、期間に入る日だけを足す。
 * 丸めの誤差がたまらないよう、メートルと秒のまま足してから km と時間に直す。
 */
export function toChartRows(
  days: HistoryResponse['days'],
  granularity: ChartGranularity,
): ChartRow[] {
  const buckets: Bucket[] = [];
  for (const { date, summary } of days) {
    const key = bucketKey(date, granularity);
    let bucket = buckets.at(-1);
    if (bucket?.key !== key) {
      bucket = {
        key,
        from: date,
        to: date,
        walking: 0,
        cycling: 0,
        automotive: 0,
        unknown: 0,
        moving: 0,
        stationary: 0,
        points: 0,
        excluded: 0,
      };
      buckets.push(bucket);
    }
    bucket.to = date;
    bucket.walking += summary.distance_by_activity.walking;
    bucket.cycling += summary.distance_by_activity.cycling;
    bucket.automotive += summary.distance_by_activity.automotive;
    bucket.unknown += summary.distance_by_activity.unknown;
    bucket.moving += summary.moving_s;
    bucket.stationary += summary.stationary_s;
    bucket.points += summary.point_count;
    bucket.excluded += summary.excluded_count;
  }
  return buckets.map((bucket) => ({
    from: bucket.from,
    to: bucket.to,
    label: bucketLabel(bucket.from, granularity),
    walking: toKm(bucket.walking),
    cycling: toKm(bucket.cycling),
    automotive: toKm(bucket.automotive),
    unknown: toKm(bucket.unknown),
    movingHours: toHours(bucket.moving),
    stationaryHours: toHours(bucket.stationary),
    points: bucket.points,
    excluded: bucket.excluded,
  }));
}

/** 足し合わせている途中の 1 本の棒。距離はメートル、時間は秒で持つ。 */
type Bucket = {
  key: string;
  from: string;
  to: string;
  walking: number;
  cycling: number;
  automotive: number;
  unknown: number;
  moving: number;
  stationary: number;
  points: number;
  excluded: number;
};

const toKm = (meters: number) => Math.round(meters / 100) / 10;
const toHours = (seconds: number) => Math.round(seconds / 360) / 10;

/** `YYYY-MM-DD` が入る棒の目印。同じ棒に入る日は同じ値になる。 */
function bucketKey(date: string, granularity: ChartGranularity): string {
  switch (granularity) {
    case 'day':
      return date;
    case 'week':
      return mondayOf(date);
    case 'month':
      return date.slice(0, 7);
  }
}

/** 棒の目盛りの文字。日と週は最初の日の「M/D」、月は「YYYY/M」にする。 */
function bucketLabel(from: string, granularity: ChartGranularity): string {
  if (granularity === 'month') {
    const [year = '', month = '0'] = from.split('-');
    return `${year}/${Number(month)}`;
  }
  return formatShortDate(from);
}

/** `YYYY-MM-DD` を含む週の月曜日を `YYYY-MM-DD` で返す。 */
function mondayOf(date: string): string {
  const [year = 1970, month = 1, day = 1] = date.split('-').map(Number);
  const utc = new Date(Date.UTC(year, month - 1, day));
  // getUTCDay は日曜日が 0 なので、月曜日からの日数に直す
  const sinceMonday = (utc.getUTCDay() + 6) % 7;
  utc.setUTCDate(utc.getUTCDate() - sinceMonday);
  return utc.toISOString().slice(0, 10);
}
