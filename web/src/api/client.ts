import * as v from 'valibot';
import type { DateRange } from '../range.ts';
import { ErrorResponseSchema, type HistoryResponse, HistoryResponseSchema } from './schema.gen.ts';

/** API がエラーの応答を返したことを表す。 */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

// Vite の base ('/viewer/'、vite.config.ts 参照) を固定で使う。
// import.meta.env.BASE_URL は SSR/Vitest (node 環境) では '/' になり、
// ブラウザでの実配信パスと食い違うため使わない。
const BASE_URL = '/viewer/';

/** 期間の位置ログを返す API の URL。 */
export function historyUrl(range: DateRange): string {
  const query = new URLSearchParams({ from: range.from, to: range.to });
  return `${BASE_URL}api/history?${query}`;
}

/**
 * 期間の位置ログを取得する。応答は生成したスキーマで検証してから返す。
 *
 * エラーの応答なら ApiError を、スキーマに合わない応答なら valibot の ValiError を投げる。
 */
export async function fetchHistory(
  range: DateRange,
  signal?: AbortSignal,
): Promise<HistoryResponse> {
  const response = await fetch(historyUrl(range), { signal });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = v.safeParse(ErrorResponseSchema, body);
    throw new ApiError(
      error.success ? error.output.error : `HTTP ${response.status}`,
      response.status,
    );
  }
  return v.parse(HistoryResponseSchema, body);
}
