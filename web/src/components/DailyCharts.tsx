import { memo, type ReactNode, useMemo } from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ACTIVITY_COLORS, ACTIVITY_LABELS, DISTANCE_ACTIVITIES } from '../activity.ts';
import type { HistoryResponse } from '../api/schema.gen.ts';
import { type ChartGranularity, type ChartRow, chartGranularity, toChartRows } from '../charts.ts';
import type { DateRange } from '../range.ts';
import styles from './DailyCharts.module.css';

type Props = {
  /** 日ごとの集計 */
  days: HistoryResponse['days'];
  /** 棒をクリックしたとき、その棒にまとめた期間を受け取る */
  onSelectRange: (range: DateRange) => void;
};

/** 棒にまとめた長さごとの説明。 */
const HINTS: Record<ChartGranularity, string> = {
  day: '棒をクリックすると、その日だけを表示します',
  week: '期間が長いため週ごとにまとめています。棒をクリックすると、その週だけを表示します',
  month: '期間が長いため月ごとにまとめています。棒をクリックすると、その月だけを表示します',
};

/**
 * 移動距離、移動と静止の時間、記録点数のグラフ。
 *
 * 長い期間は週や月ごとにまとめて棒の数を抑える。行の計算はメモ化し、
 * 読み込み中の表示の切り替えなどで親が描き直されても作り直さない。
 */
export const DailyCharts = memo(function DailyCharts({ days, onSelectRange }: Props) {
  const granularity = chartGranularity(days.length);
  const rows = useMemo(() => toChartRows(days, granularity), [days, granularity]);
  const select = (item: { payload?: ChartRow }) => {
    if (item.payload) {
      onSelectRange({ from: item.payload.from, to: item.payload.to });
    }
  };

  return (
    <div className={styles.charts}>
      <p className={styles.hint}>{HINTS[granularity]}</p>
      <Chart title="移動距離 (km)" rows={rows}>
        {DISTANCE_ACTIVITIES.map((activity) => (
          <Bar
            key={activity}
            dataKey={activity}
            name={ACTIVITY_LABELS[activity]}
            stackId="distance"
            fill={ACTIVITY_COLORS[activity]}
            cursor="pointer"
            onClick={select}
          />
        ))}
      </Chart>
      <Chart title="移動と静止 (時間)" rows={rows}>
        <Bar
          dataKey="movingHours"
          name="移動"
          stackId="time"
          fill="#4c6ef5"
          cursor="pointer"
          onClick={select}
        />
        <Bar
          dataKey="stationaryHours"
          name="静止"
          stackId="time"
          fill={ACTIVITY_COLORS.stationary}
          cursor="pointer"
          onClick={select}
        />
      </Chart>
      <Chart title="記録点数" rows={rows}>
        <Bar
          dataKey="points"
          name="記録"
          stackId="points"
          fill="#495057"
          cursor="pointer"
          onClick={select}
        />
        <Bar
          dataKey="excluded"
          name="除外"
          stackId="points"
          fill="#adb5bd"
          cursor="pointer"
          onClick={select}
        />
      </Chart>
    </div>
  );
});

/** 1 つの棒グラフ。 */
function Chart({
  title,
  rows,
  children,
}: {
  title: string;
  rows: ChartRow[];
  children: ReactNode;
}) {
  return (
    <section>
      <h3>{title}</h3>
      <ResponsiveContainer width="100%" height={180}>
        <BarChart data={rows}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey="label" />
          <YAxis />
          <Tooltip />
          <Legend />
          {children}
        </BarChart>
      </ResponsiveContainer>
    </section>
  );
}
