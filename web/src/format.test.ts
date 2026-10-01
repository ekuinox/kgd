import { describe, expect, it } from 'vitest';
import {
  formatCount,
  formatDateTime,
  formatDuration,
  formatKm,
  formatShortDate,
} from './format.ts';

describe('format', () => {
  it('formats distances like the daily report', () => {
    expect(formatKm(12345)).toBe('12.3 km');
    expect(formatKm(0)).toBe('0.0 km');
  });

  it('formats durations like the daily report', () => {
    expect(formatDuration(59)).toBe('0 分');
    expect(formatDuration(5 * 60)).toBe('5 分');
    expect(formatDuration(3 * 3600 + 5 * 60 + 30)).toBe('3 時間 5 分');
  });

  it('groups thousands', () => {
    expect(formatCount(1234567)).toBe('1,234,567');
  });

  it('shows date and time in the given time zone', () => {
    expect(formatDateTime('2026-09-01T00:03:12Z', 'Asia/Tokyo')).toBe('9/1 09:03');
    expect(formatDateTime(null, 'Asia/Tokyo')).toBe('-');
  });

  it('shortens ISO dates for chart labels', () => {
    expect(formatShortDate('2026-09-05')).toBe('9/5');
  });
});
