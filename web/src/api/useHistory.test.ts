import * as v from 'valibot';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { historyFixture } from './fixture.ts';
import { type HistoryResponse, HistoryResponseSchema } from './schema.gen.ts';
import { type HistoryState, loadHistory } from './useHistory.ts';

const range = { from: '2026-09-02', to: '2026-09-02' };
const previousData: HistoryResponse = v.parse(HistoryResponseSchema, historyFixture);
const nextData: HistoryResponse = {
  ...previousData,
  range: { ...previousData.range, from: '2026-09-02', to: '2026-09-02' },
};

/** 直前の期間を表示している状態から始め、`loadHistory` が書き換えた状態を返す。 */
function harness() {
  let state: HistoryState = { data: previousData, loading: false, error: null };
  return {
    update: (next: (previous: HistoryState) => HistoryState) => {
      state = next(state);
    },
    current: () => state,
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('loadHistory', () => {
  it('keeps the previous data while loading', async () => {
    const { update, current } = harness();
    let resolve: (data: HistoryResponse) => void = () => {};
    const pending = loadHistory(
      range,
      new AbortController().signal,
      update,
      () => new Promise((r) => (resolve = r)),
    );

    expect(current()).toEqual({ data: previousData, loading: true, error: null });

    resolve(nextData);
    await pending;
    expect(current()).toEqual({ data: nextData, loading: false, error: null });
  });

  it('drops the previous data when the request fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const { update, current } = harness();

    await loadHistory(range, new AbortController().signal, update, async () => {
      throw new Error('internal error');
    });

    expect(current()).toEqual({ data: null, loading: false, error: 'internal error' });
  });

  it('ignores a response that arrives after the request was aborted', async () => {
    const { update, current } = harness();
    const controller = new AbortController();

    await loadHistory(range, controller.signal, update, async () => {
      controller.abort();
      return nextData;
    });

    expect(current()).toEqual({ data: previousData, loading: true, error: null });
  });

  it('ignores a failure caused by aborting the request', async () => {
    const { update, current } = harness();
    const controller = new AbortController();

    await loadHistory(range, controller.signal, update, async () => {
      controller.abort();
      throw new DOMException('aborted', 'AbortError');
    });

    expect(current()).toEqual({ data: previousData, loading: true, error: null });
  });
});
