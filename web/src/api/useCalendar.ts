import { useEffect, useState } from 'react';
import { fetchCalendar } from './client.ts';

/** サーバーの暦の取得状態。取得できるまで `timezone` は null。 */
export type CalendarState = {
  timezone: string | null;
  error: string | null;
};

/** サーバーの暦 (日を区切るタイムゾーン) を 1 回取得する。 */
export function useCalendar(): CalendarState {
  const [state, setState] = useState<CalendarState>({ timezone: null, error: null });

  useEffect(() => {
    const controller = new AbortController();
    fetchCalendar(controller.signal).then(
      ({ timezone }) => {
        if (!controller.signal.aborted) {
          setState({ timezone, error: null });
        }
      },
      (error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        console.error('Failed to load the calendar', error);
        const message = error instanceof Error ? error.message : String(error);
        setState({ timezone: null, error: message });
      },
    );
    return () => controller.abort();
  }, []);

  return state;
}
