/** API の応答の例。Rust の DTO が出す JSON と同じ形にする (テストで使う)。 */
export const historyFixture = {
  range: { from: '2026-09-01', to: '2026-09-01', timezone: 'Asia/Tokyo' },
  total: {
    distance_m: 1500,
    distance_by_activity: { walking: 500, cycling: 0, automotive: 1000, unknown: 0 },
    moving_s: 1200,
    stationary_s: 90,
    point_count: 3,
    excluded_count: 1,
    first_at: '2026-09-01T00:03:12Z',
    last_at: '2026-09-01T09:30:00Z',
  },
  days: [
    {
      date: '2026-09-01',
      summary: {
        distance_m: 1500,
        distance_by_activity: { walking: 500, cycling: 0, automotive: 1000, unknown: 0 },
        moving_s: 1200,
        stationary_s: 90,
        point_count: 3,
        excluded_count: 1,
        first_at: '2026-09-01T00:03:12Z',
        last_at: '2026-09-01T09:30:00Z',
      },
    },
  ],
  track: {
    type: 'FeatureCollection',
    features: [
      {
        type: 'Feature',
        properties: { activity: 'walking' },
        geometry: {
          type: 'LineString',
          coordinates: [
            [139.7, 35.6],
            [139.71, 35.61],
          ],
        },
      },
    ],
  },
  track_meta: { original_points: 3, returned_points: 3, simplified: false },
};
