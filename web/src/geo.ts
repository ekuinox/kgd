import type { HistoryResponse } from './api/schema.gen.ts';

/** 南西の角と北東の角 (`[経度, 緯度]`)。MapLibre の fitBounds に渡す形。 */
export type Bounds = [[number, number], [number, number]];

/** 軌跡の全体が収まる範囲を返す。点が無ければ null。 */
export function trackBounds(track: HistoryResponse['track']): Bounds | null {
  let west = Number.POSITIVE_INFINITY;
  let south = Number.POSITIVE_INFINITY;
  let east = Number.NEGATIVE_INFINITY;
  let north = Number.NEGATIVE_INFINITY;
  for (const feature of track.features) {
    for (const position of feature.geometry.coordinates) {
      const lon = position[0];
      const lat = position[1];
      if (lon === undefined || lat === undefined) {
        continue;
      }
      west = Math.min(west, lon);
      east = Math.max(east, lon);
      south = Math.min(south, lat);
      north = Math.max(north, lat);
    }
  }
  if (!Number.isFinite(west)) {
    return null;
  }
  return [
    [west, south],
    [east, north],
  ];
}
