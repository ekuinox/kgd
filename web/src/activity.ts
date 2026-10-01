import type { ExpressionSpecification } from '@maplibre/maplibre-gl-style-spec';
import type { HistoryResponse } from './api/schema.gen.ts';

/** 移動種別。 */
export type ActivityKind = HistoryResponse['track']['features'][number]['properties']['activity'];

/** 線の色。日次レポートの地図画像と揃える。 */
export const ACTIVITY_COLORS: Record<ActivityKind, string> = {
  walking: '#2e9e44',
  cycling: '#f08c1a',
  automotive: '#1f6fd1',
  stationary: '#c0392b',
  unknown: '#808080',
};

/** 表示名。日次レポートの文言と揃える。 */
export const ACTIVITY_LABELS: Record<ActivityKind, string> = {
  walking: '徒歩',
  cycling: '自転車',
  automotive: '車',
  stationary: '静止',
  unknown: '不明',
};

/** 距離を持つ移動種別 (静止は距離を積算しない)。グラフの積み上げの順序にも使う。 */
export const DISTANCE_ACTIVITIES = ['walking', 'cycling', 'automotive', 'unknown'] as const;

/** 区間の `activity` から線の色を選ぶ MapLibre の式。 */
export function lineColorExpression(): ExpressionSpecification {
  const pairs = (Object.keys(ACTIVITY_COLORS) as ActivityKind[]).flatMap((activity) => [
    activity,
    ACTIVITY_COLORS[activity],
  ]);
  return [
    'match',
    ['get', 'activity'],
    ...pairs,
    ACTIVITY_COLORS.unknown,
  ] as unknown as ExpressionSpecification;
}
