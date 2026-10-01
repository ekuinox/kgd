import { describe, expect, it } from 'vitest';
import { historyFixture } from './api/fixture.ts';
import { toChartRows } from './charts.ts';

describe('toChartRows', () => {
  it('converts daily summaries into chart rows in km, hours and counts', () => {
    expect(toChartRows(historyFixture.days)).toEqual([
      {
        date: '2026-09-01',
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
});
