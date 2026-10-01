import type { ReactNode } from 'react';
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
import { type ChartRow, toChartRows } from '../charts.ts';
import styles from './DailyCharts.module.css';

type Props = {
  /** 日ごとの集計 */
  days: HistoryResponse['days'];
  /** 棒をクリックした日 (`YYYY-MM-DD`) を受け取る */
  onSelectDay: (date: string) => void;
};

/** 日ごとの移動距離、移動と静止の時間、記録点数のグラフ。 */
export function DailyCharts({ days, onSelectDay }: Props) {
  const rows = toChartRows(days);
  const select = (item: { payload?: ChartRow }) => {
    if (item.payload) {
      onSelectDay(item.payload.date);
    }
  };

  return (
    <div className={styles.charts}>
      <p className={styles.hint}>棒をクリックすると、その日だけを表示します</p>
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
}

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
