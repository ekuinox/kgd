import { useEffect, useState } from 'react';
import type { DateRange } from '../range.ts';
import { fetchHistory } from './client.ts';
import type { HistoryResponse } from './schema.gen.ts';

/** 期間の位置ログの取得状態。読み込み中も直前の結果を残し、地図のちらつきを抑える。 */
export type HistoryState = {
  data: HistoryResponse | null;
  loading: boolean;
  error: string | null;
};

/**
 * 期間が変わるたびに位置ログを取得する。
 *
 * 期間を変えたら前のリクエストを取り消し、古い応答が新しい期間の表示を上書きしないようにする。
 */
export function useHistory(range: DateRange): HistoryState {
  const [state, setState] = useState<HistoryState>({ data: null, loading: true, error: null });
  const { from, to } = range;

  useEffect(() => {
    const controller = new AbortController();
    setState((previous) => ({ ...previous, loading: true, error: null }));
    fetchHistory({ from, to }, controller.signal).then(
      (data) => setState({ data, loading: false, error: null }),
      (error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        console.error('Failed to load location history', error);
        const message = error instanceof Error ? error.message : String(error);
        setState((previous) => ({ ...previous, loading: false, error: message }));
      },
    );
    return () => controller.abort();
  }, [from, to]);

  return state;
}
