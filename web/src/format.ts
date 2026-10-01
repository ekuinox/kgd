/** メートルを小数 1 桁のキロメートル表記にする (日次レポートと同じ形)。 */
export function formatKm(meters: number): string {
  return `${(meters / 1000).toFixed(1)} km`;
}

/** 秒を「N 時間 M 分」または「M 分」にする (日次レポートと同じ形)。 */
export function formatDuration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  return hours > 0 ? `${hours} 時間 ${minutes % 60} 分` : `${minutes} 分`;
}

/** 3 桁ごとにカンマで区切る。 */
export function formatCount(value: number): string {
  return value.toLocaleString('en-US');
}

/** ISO 8601 の時刻を、指定したタイムゾーンの「M/D HH:MM」にする。無ければ「-」。 */
export function formatDateTime(iso: string | null | undefined, timeZone: string): string {
  if (!iso) {
    return '-';
  }
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(new Date(iso));
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((item) => item.type === type)?.value ?? '';
  return `${part('month')}/${part('day')} ${part('hour')}:${part('minute')}`;
}

/** `YYYY-MM-DD` をグラフの目盛り用の「M/D」にする。 */
export function formatShortDate(isoDate: string): string {
  const [, month = '0', day = '0'] = isoDate.split('-');
  return `${Number(month)}/${Number(day)}`;
}
