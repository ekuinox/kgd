import * as v from 'valibot';
import { type HistoryQuery, HistoryQuerySchema } from './api/schema.gen.ts';

/** 選んだ期間。開始日と終了日 (どちらも含む) を `YYYY-MM-DD` で持つ。 */
export type DateRange = HistoryQuery;

/** よく使う範囲のボタン。 */
export type RangePreset = { label: string; range: DateRange };

/**
 * `timeZone` の暦で、`now` が何日かを `YYYY-MM-DD` で返す。
 *
 * サーバーは設定したタイムゾーンで日を区切るため、ブラウザの現地ではなくこの暦で「今日」を決める。
 */
export function todayIn(timeZone: string, now: Date): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(now);
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((item) => item.type === type)?.value ?? '';
  return `${part('year').padStart(4, '0')}-${part('month')}-${part('day')}`;
}

/** ブラウザの現地の日付を `YYYY-MM-DD` にする。カレンダーで選んだ日に使う。 */
export function toIsoDate(date: Date): string {
  const year = String(date.getFullYear()).padStart(4, '0');
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/** `YYYY-MM-DD` を、ブラウザの現地の 0 時の Date にする。カレンダーに渡す日に使う。 */
export function fromIsoDate(value: string): Date {
  const [year = 1970, month = 1, day = 1] = value.split('-').map(Number);
  return new Date(year, month - 1, day);
}

/** `YYYY-MM-DD` の日付を `days` 日ずらす。 */
export function addDays(date: string, days: number): string {
  const [year = 1970, month = 1, day = 1] = date.split('-').map(Number);
  return new Date(Date.UTC(year, month - 1, day + days)).toISOString().slice(0, 10);
}

/** 今日 (`YYYY-MM-DD`) だけの期間。 */
export function todayRange(today: string): DateRange {
  return { from: today, to: today };
}

/** 今日 (`YYYY-MM-DD`) から見た、よく使う範囲 (今日、昨日、直近 7 日、今月)。 */
export function presetRanges(today: string): RangePreset[] {
  const yesterday = addDays(today, -1);
  return [
    { label: '今日', range: { from: today, to: today } },
    { label: '昨日', range: { from: yesterday, to: yesterday } },
    { label: '直近 7 日', range: { from: addDays(today, -6), to: today } },
    { label: '今月', range: { from: `${today.slice(0, 8)}01`, to: today } },
  ];
}

/** URL のクエリから期間を読む。無い、形式が違う、前後が逆のときは今日 (`YYYY-MM-DD`) にする。 */
export function rangeFromSearch(search: string, today: string): DateRange {
  const params = new URLSearchParams(search);
  const parsed = v.safeParse(HistoryQuerySchema, {
    from: params.get('from'),
    to: params.get('to'),
  });
  if (!parsed.success || parsed.output.from > parsed.output.to) {
    return todayRange(today);
  }
  return parsed.output;
}

/** 期間を URL のクエリにする。 */
export function rangeToSearch(range: DateRange): string {
  return `?${new URLSearchParams({ from: range.from, to: range.to })}`;
}
