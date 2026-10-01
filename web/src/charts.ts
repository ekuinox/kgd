import type { HistoryResponse } from './api/schema.gen.ts';
import { formatShortDate } from './format.ts';

/** 日ごとのグラフの 1 行。距離は km、時間は時間で持つ (どちらも小数 1 桁)。 */
export type ChartRow = {
  date: string;
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

const toKm = (meters: number) => Math.round(meters / 100) / 10;
const toHours = (seconds: number) => Math.round(seconds / 360) / 10;

/** 日ごとの集計をグラフの行にする。 */
export function toChartRows(days: HistoryResponse['days']): ChartRow[] {
  return days.map(({ date, summary }) => ({
    date,
    label: formatShortDate(date),
    walking: toKm(summary.distance_by_activity.walking),
    cycling: toKm(summary.distance_by_activity.cycling),
    automotive: toKm(summary.distance_by_activity.automotive),
    unknown: toKm(summary.distance_by_activity.unknown),
    movingHours: toHours(summary.moving_s),
    stationaryHours: toHours(summary.stationary_s),
    points: summary.point_count,
    excluded: summary.excluded_count,
  }));
}
