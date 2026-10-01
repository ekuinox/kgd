import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, calendarUrl, fetchCalendar, fetchHistory, historyUrl } from './client.ts';
import { historyFixture } from './fixture.ts';

const range = { from: '2026-09-01', to: '2026-09-01' };

function respondWith(body: unknown, status = 200) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify(body), { status })),
  );
}

beforeEach(() => {
  // vite.config.ts の base ('/viewer/') を模す。Vitest (node 環境) では
  // import.meta.env.BASE_URL が既定で '/' になるため、ここで明示的に揃える。
  vi.stubEnv('BASE_URL', '/viewer/');
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe('fetchHistory', () => {
  it('requests the history API under the viewer base path', () => {
    expect(historyUrl(range)).toBe('/viewer/api/history?from=2026-09-01&to=2026-09-01');
  });

  it('returns a response that matches the generated schema', async () => {
    respondWith(historyFixture);

    const history = await fetchHistory(range);

    expect(history.total.point_count).toBe(3);
    expect(history.track.features[0]?.properties.activity).toBe('walking');
  });

  it('throws ApiError with the server message on error responses', async () => {
    respondWith({ error: 'from must not be after to' }, 400);

    await expect(fetchHistory(range)).rejects.toEqual(
      expect.objectContaining({
        name: 'ApiError',
        status: 400,
        message: 'from must not be after to',
      }),
    );
  });

  it('rejects responses that do not match the schema', async () => {
    const { track_meta: _, ...broken } = historyFixture;
    respondWith(broken);

    await expect(fetchHistory(range)).rejects.not.toBeInstanceOf(ApiError);
    await expect(fetchHistory(range)).rejects.toThrow();
  });

  it('rejects when the request is aborted', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        (_url: string, init?: RequestInit) =>
          new Promise((_resolve, reject) => {
            init?.signal?.addEventListener('abort', () =>
              reject(new DOMException('aborted', 'AbortError')),
            );
          }),
      ),
    );
    const controller = new AbortController();

    const pending = fetchHistory(range, controller.signal);
    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
  });
});

describe('fetchCalendar', () => {
  it('requests the calendar API under the viewer base path', () => {
    expect(calendarUrl()).toBe('/viewer/api/calendar');
  });

  it('returns the time zone the server uses to split days', async () => {
    respondWith({ timezone: 'Asia/Tokyo' });

    await expect(fetchCalendar()).resolves.toEqual({ timezone: 'Asia/Tokyo' });
  });

  it('throws ApiError with the server message on error responses', async () => {
    respondWith({ error: 'not found' }, 404);

    await expect(fetchCalendar()).rejects.toEqual(
      expect.objectContaining({ name: 'ApiError', status: 404, message: 'not found' }),
    );
  });
});
