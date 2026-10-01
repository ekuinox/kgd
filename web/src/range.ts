import * as v from 'valibot';
import { type HistoryQuery, HistoryQuerySchema } from './api/schema.gen.ts';

/** 選んだ期間。開始日と終了日 (どちらも含む) を `YYYY-MM-DD` で持つ。 */
export type DateRange = HistoryQuery;

/** よく使う範囲のボタン。 */
export type RangePreset = { label: string; range: DateRange };

/** ブラウザの現地の日付を `YYYY-MM-DD` にする。 */
export function toIsoDate(date: Date): string {
  const year = String(date.getFullYear()).padStart(4, '0');
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/** `YYYY-MM-DD` を、ブラウザの現地の 0 時の Date にする。 */
export function fromIsoDate(value: string): Date {
  const [year = 1970, month = 1, day = 1] = value.split('-').map(Number);
  return new Date(year, month - 1, day);
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

/** 今日だけの期間。 */
export function todayRange(now: Date): DateRange {
  const today = toIsoDate(now);
  return { from: today, to: today };
}

/** よく使う範囲 (今日、昨日、直近 7 日、今月)。 */
export function presetRanges(now: Date): RangePreset[] {
  const today = toIsoDate(now);
  const yesterday = toIsoDate(addDays(now, -1));
  return [
    { label: '今日', range: { from: today, to: today } },
    { label: '昨日', range: { from: yesterday, to: yesterday } },
    { label: '直近 7 日', range: { from: toIsoDate(addDays(now, -6)), to: today } },
    {
      label: '今月',
      range: { from: toIsoDate(new Date(now.getFullYear(), now.getMonth(), 1)), to: today },
    },
  ];
}

/** URL のクエリから期間を読む。無い、形式が違う、前後が逆のときは今日にする。 */
export function rangeFromSearch(search: string, now: Date): DateRange {
  const params = new URLSearchParams(search);
  const parsed = v.safeParse(HistoryQuerySchema, {
    from: params.get('from'),
    to: params.get('to'),
  });
  if (!parsed.success || parsed.output.from > parsed.output.to) {
    return todayRange(now);
  }
  return parsed.output;
}

/** 期間を URL のクエリにする。 */
export function rangeToSearch(range: DateRange): string {
  return `?${new URLSearchParams({ from: range.from, to: range.to })}`;
}
