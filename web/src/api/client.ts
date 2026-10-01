import * as v from 'valibot';
import type { DateRange } from '../range.ts';
import {
  type CalendarResponse,
  CalendarResponseSchema,
  ErrorResponseSchema,
  type HistoryResponse,
  HistoryResponseSchema,
} from './schema.gen.ts';

/** API がエラーの応答を返したことを表す。 */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

/** サーバーの暦 (日を区切るタイムゾーン) を返す API の URL。 */
export function calendarUrl(): string {
  return `${import.meta.env.BASE_URL}api/calendar`;
}

/** 期間の位置ログを返す API の URL。 */
export function historyUrl(range: DateRange): string {
  const query = new URLSearchParams({ from: range.from, to: range.to });
  return `${import.meta.env.BASE_URL}api/history?${query}`;
}

/**
 * サーバーの暦を取得する。画面はこの暦で「今日」を決める。
 *
 * エラーの扱いは `fetchHistory` と同じ。
 */
export function fetchCalendar(signal?: AbortSignal): Promise<CalendarResponse> {
  return fetchJson(calendarUrl(), CalendarResponseSchema, signal);
}

/**
 * 期間の位置ログを取得する。応答は生成したスキーマで検証してから返す。
 *
 * エラーの応答なら ApiError を、スキーマに合わない応答なら valibot の ValiError を投げる。
 */
export function fetchHistory(range: DateRange, signal?: AbortSignal): Promise<HistoryResponse> {
  return fetchJson(historyUrl(range), HistoryResponseSchema, signal);
}

/** JSON を取得し、エラーの応答なら ApiError を投げ、成功ならスキーマで検証して返す。 */
async function fetchJson<TSchema extends v.GenericSchema>(
  url: string,
  schema: TSchema,
  signal?: AbortSignal,
): Promise<v.InferOutput<TSchema>> {
  const response = await fetch(url, { signal });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = v.safeParse(ErrorResponseSchema, body);
    throw new ApiError(
      error.success ? error.output.error : `HTTP ${response.status}`,
      response.status,
    );
  }
  return v.parse(schema, body);
}
