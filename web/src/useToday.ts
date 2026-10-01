import { useEffect, useState } from 'react';
import { todayIn } from './range.ts';

/** 日付が変わったかを確かめる間隔 (ミリ秒)。 */
const CHECK_INTERVAL_MS = 60_000;

/**
 * `timeZone` の暦での今日を `YYYY-MM-DD` で返す。
 *
 * 画面を開いたまま 0 時を過ぎても「今日」が前の日を指したままにならないよう、
 * 定期的に、またタブが表に戻ったときに確かめ直す (裏のタブではタイマーが間引かれるため)。
 */
export function useToday(timeZone: string): string {
  const [today, setToday] = useState(() => todayIn(timeZone, new Date()));

  useEffect(() => {
    const check = () => setToday(todayIn(timeZone, new Date()));
    check();
    const timer = window.setInterval(check, CHECK_INTERVAL_MS);
    document.addEventListener('visibilitychange', check);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', check);
    };
  }, [timeZone]);

  return today;
}
