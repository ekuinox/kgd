import { describe, expect, it } from 'vitest';
import { trackBounds } from './geo.ts';

function line(coordinates: [number, number][]) {
  return {
    type: 'Feature' as const,
    properties: { activity: 'walking' as const },
    geometry: { type: 'LineString' as const, coordinates },
  };
}

describe('trackBounds', () => {
  it('returns the south-west and north-east corners of every line', () => {
    const track = {
      type: 'FeatureCollection' as const,
      features: [
        line([
          [139.7, 35.6],
          [139.8, 35.7],
        ]),
        line([[139.5, 35.9]]),
      ],
    };

    expect(trackBounds(track)).toEqual([
      [139.5, 35.6],
      [139.8, 35.9],
    ]);
  });

  it('returns null for an empty track', () => {
    expect(trackBounds({ type: 'FeatureCollection', features: [] })).toBeNull();
  });
});
