import { ACTIVITY_LABELS, DISTANCE_ACTIVITIES } from '../activity.ts';
import type { HistorySummary } from '../api/schema.gen.ts';
import { formatCount, formatDateTime, formatDuration, formatKm } from '../format.ts';
import styles from './SummaryPanel.module.css';

type Props = {
  /** 期間全体の集計 */
  summary: HistorySummary;
  /** 時刻を表示するタイムゾーン */
  timezone: string;
};

/** 期間全体の合計。文言は日次レポートに揃える。 */
export function SummaryPanel({ summary, timezone }: Props) {
  const breakdown = DISTANCE_ACTIVITIES.filter(
    (activity) => summary.distance_by_activity[activity] > 0,
  )
    .map(
      (activity) =>
        `${ACTIVITY_LABELS[activity]} ${formatKm(summary.distance_by_activity[activity])}`,
    )
    .join(' / ');

  return (
    <dl className={styles.summary}>
      <dt>移動距離</dt>
      <dd>
        {formatKm(summary.distance_m)}
        {breakdown && ` (${breakdown})`}
      </dd>
      <dt>移動 / 静止</dt>
      <dd>
        {formatDuration(summary.moving_s)} / {formatDuration(summary.stationary_s)}
      </dd>
      <dt>記録</dt>
      <dd>
        {formatCount(summary.point_count)} 点
        {summary.excluded_count > 0 &&
          ` (精度不足で ${formatCount(summary.excluded_count)} 点を除外)`}
      </dd>
      <dt>最初 / 最後</dt>
      <dd>
        {formatDateTime(summary.first_at, timezone)} / {formatDateTime(summary.last_at, timezone)}
      </dd>
    </dl>
  );
}
