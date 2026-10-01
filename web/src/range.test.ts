import { describe, expect, it } from 'vitest';
import {
  fromIsoDate,
  presetRanges,
  rangeFromSearch,
  rangeToSearch,
  todayRange,
  toIsoDate,
} from './range.ts';

const now = new Date(2026, 8, 30, 10, 0);

describe('toIsoDate / fromIsoDate', () => {
  it('formats the local date with zero padding and parses it back', () => {
    expect(toIsoDate(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
    expect(toIsoDate(fromIsoDate('2026-09-01'))).toBe('2026-09-01');
  });
});

describe('presetRanges', () => {
  it('offers today, yesterday, the last 7 days and this month', () => {
    expect(presetRanges(now)).toEqual([
      { label: '今日', range: { from: '2026-09-30', to: '2026-09-30' } },
      { label: '昨日', range: { from: '2026-09-29', to: '2026-09-29' } },
      { label: '直近 7 日', range: { from: '2026-09-24', to: '2026-09-30' } },
      { label: '今月', range: { from: '2026-09-01', to: '2026-09-30' } },
    ]);
  });
});

describe('rangeFromSearch', () => {
  it('reads from and to from the query string', () => {
    expect(rangeFromSearch('?from=2026-09-01&to=2026-09-10', now)).toEqual({
      from: '2026-09-01',
      to: '2026-09-10',
    });
  });

  it('falls back to today for missing, malformed or reversed ranges', () => {
    const today = todayRange(now);
    expect(rangeFromSearch('', now)).toEqual(today);
    expect(rangeFromSearch('?from=2026-9-1&to=2026-09-10', now)).toEqual(today);
    expect(rangeFromSearch('?from=2026-09-10&to=2026-09-01', now)).toEqual(today);
  });

  it('round-trips through rangeToSearch', () => {
    const range = { from: '2026-09-01', to: '2026-09-10' };
    expect(rangeFromSearch(rangeToSearch(range), now)).toEqual(range);
  });
});
