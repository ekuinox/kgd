import { describe, expect, it } from 'vitest';
import { ACTIVITY_COLORS, ACTIVITY_LABELS, lineColorExpression } from './activity.ts';

describe('activity', () => {
  it('uses the same colors and labels as the daily report', () => {
    expect(ACTIVITY_COLORS).toEqual({
      walking: '#2e9e44',
      cycling: '#f08c1a',
      automotive: '#1f6fd1',
      stationary: '#c0392b',
      unknown: '#808080',
    });
    expect(ACTIVITY_LABELS).toEqual({
      walking: '徒歩',
      cycling: '自転車',
      automotive: '車',
      stationary: '静止',
      unknown: '不明',
    });
  });

  it('builds a match expression over every activity with gray as the fallback', () => {
    expect(lineColorExpression()).toEqual([
      'match',
      ['get', 'activity'],
      'walking',
      '#2e9e44',
      'cycling',
      '#f08c1a',
      'automotive',
      '#1f6fd1',
      'stationary',
      '#c0392b',
      'unknown',
      '#808080',
      '#808080',
    ]);
  });
});
