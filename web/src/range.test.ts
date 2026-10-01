import { describe, expect, it } from 'vitest';
import {
  addDays,
  fromIsoDate,
  presetRanges,
  rangeFromSearch,
  rangeToSearch,
  todayIn,
  todayRange,
  toIsoDate,
} from './range.ts';

const today = '2026-09-30';

describe('todayIn', () => {
  it('uses the calendar of the given time zone, not the browser', () => {
    // 2026-09-30 15:30 UTC は Asia/Tokyo では 10/1、America/Los_Angeles では 9/30
    const now = new Date(Date.UTC(2026, 8, 30, 15, 30));
    expect(todayIn('Asia/Tokyo', now)).toBe('2026-10-01');
    expect(todayIn('America/Los_Angeles', now)).toBe('2026-09-30');
    expect(todayIn('UTC', now)).toBe('2026-09-30');
  });

  it('switches at midnight of the given time zone', () => {
    // Asia/Tokyo の 0 時は UTC の前日 15 時
    expect(todayIn('Asia/Tokyo', new Date(Date.UTC(2026, 8, 30, 14, 59)))).toBe('2026-09-30');
    expect(todayIn('Asia/Tokyo', new Date(Date.UTC(2026, 8, 30, 15, 0)))).toBe('2026-10-01');
  });
});

describe('toIsoDate / fromIsoDate', () => {
  it('formats the local date with zero padding and parses it back', () => {
    expect(toIsoDate(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
    expect(toIsoDate(fromIsoDate('2026-09-01'))).toBe('2026-09-01');
  });
});

describe('addDays', () => {
  it('moves across month and year boundaries', () => {
    expect(addDays('2026-09-30', 1)).toBe('2026-10-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
    expect(addDays('2027-01-01', -1)).toBe('2026-12-31');
  });
});

describe('presetRanges', () => {
  it('offers today, yesterday, the last 7 days and this month', () => {
    expect(presetRanges(today)).toEqual([
      { label: '今日', range: { from: '2026-09-30', to: '2026-09-30' } },
      { label: '昨日', range: { from: '2026-09-29', to: '2026-09-29' } },
      { label: '直近 7 日', range: { from: '2026-09-24', to: '2026-09-30' } },
      { label: '今月', range: { from: '2026-09-01', to: '2026-09-30' } },
    ]);
  });

  it('follows the given day on the first day of a month', () => {
    expect(presetRanges('2026-10-01')).toEqual([
      { label: '今日', range: { from: '2026-10-01', to: '2026-10-01' } },
      { label: '昨日', range: { from: '2026-09-30', to: '2026-09-30' } },
      { label: '直近 7 日', range: { from: '2026-09-25', to: '2026-10-01' } },
      { label: '今月', range: { from: '2026-10-01', to: '2026-10-01' } },
    ]);
  });
});

describe('rangeFromSearch', () => {
  it('reads from and to from the query string', () => {
    expect(rangeFromSearch('?from=2026-09-01&to=2026-09-10', today)).toEqual({
      from: '2026-09-01',
      to: '2026-09-10',
    });
  });

  it('falls back to today for missing, malformed or reversed ranges', () => {
    const expected = todayRange(today);
    expect(rangeFromSearch('', today)).toEqual(expected);
    expect(rangeFromSearch('?from=2026-9-1&to=2026-09-10', today)).toEqual(expected);
    expect(rangeFromSearch('?from=2026-09-10&to=2026-09-01', today)).toEqual(expected);
  });

  it('round-trips through rangeToSearch', () => {
    const range = { from: '2026-09-01', to: '2026-09-10' };
    expect(rangeFromSearch(rangeToSearch(range), today)).toEqual(range);
  });
});
