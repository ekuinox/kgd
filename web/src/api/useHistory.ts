import { useEffect, useState } from 'react';
import type { DateRange } from '../range.ts';
import { fetchHistory } from './client.ts';
import type { HistoryResponse } from './schema.gen.ts';

/**
 * 期間の位置ログの取得状態。読み込み中も直前の結果を残し、地図のちらつきを抑える。
 *
 * 取得に失敗したら `data` は null にし、新しい期間の下に前の期間の結果を出さない。
 */
export type HistoryState = {
  data: HistoryResponse | null;
  loading: boolean;
  error: string | null;
};

/**
 * 期間の位置ログを 1 回取得し、状態を `update` で書き換える。
 *
 * 読み込み中は直前の結果を残し、失敗したら結果を消してエラーを出す。
 * `signal` が取り消されたあとに届いた結果と失敗は、新しい期間の表示を上書きしないよう捨てる。
 */
export async function loadHistory(
  range: DateRange,
  signal: AbortSignal,
  update: (next: (previous: HistoryState) => HistoryState) => void,
  fetcher: (range: DateRange, signal: AbortSignal) => Promise<HistoryResponse> = fetchHistory,
): Promise<void> {
  update((previous) => ({ ...previous, loading: true, error: null }));
  try {
    const data = await fetcher(range, signal);
    if (signal.aborted) {
      return;
    }
    update(() => ({ data, loading: false, error: null }));
  } catch (error: unknown) {
    if (signal.aborted) {
      return;
    }
    console.error('Failed to load location history', error);
    const message = error instanceof Error ? error.message : String(error);
    update(() => ({ data: null, loading: false, error: message }));
  }
}

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
    void loadHistory({ from, to }, controller.signal, setState);
    return () => controller.abort();
  }, [from, to]);

  return state;
}
