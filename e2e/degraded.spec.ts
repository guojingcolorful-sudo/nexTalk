import { expect, test, type Page } from '@playwright/test';
import { WebSocketServer, type WebSocket } from 'ws';

/**
 * degraded.spec.ts — 02-03 T3.5 e2e: the degraded (translator-down) form on
 * BOTH surfaces (GOV-14 / D-12).
 *
 * The degraded state is not a new event type: it is a `subtitle` whose
 * `trace.errorCode` is set — the original Chinese with NO English, because the
 * translation was never produced. The bubble must show the red badge, the
 * locked copy 翻译服务暂时不可用, the original, and 正在重试; the English side
 * stays empty (a translation is never fabricated). A healthy follow-up event
 * for the same segment id replaces the degraded form — zero residue, no
 * second bubble.
 *
 * Project-gated: the desktop section runs under the `desktop` project (Tauri
 * IPC mock, port 1420), the phone section under `teleprompter` (real WS mock
 * server, port 8791) — each surface is asserted against its own app origin.
 */

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: {
      registerListener: () => void;
      unregisterListener: () => void;
    };
    __tauriCalls?: { cmd: string; args: Record<string, unknown> }[];
    __tauriEmit?: (event: string, payload: unknown) => void;
  }
}

interface BridgeHandler {
  (event: { event: string; id: number; payload: unknown }): void;
}

/** Serialized into the page — must stay self-contained (same mechanism as
 *  desktop.spec.ts, trimmed to what the dual pane needs). */
function installTauriMock(): void {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  const callbacks = new Map<number, BridgeHandler>();
  const listeners = new Map<string, number[]>();
  let callbackSeq = 0;

  window.__tauriCalls = calls;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    registerListener: () => undefined,
    unregisterListener: () => undefined,
  };

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'console' },
      windows: [{ label: 'console' }, { label: 'dual' }],
    },
    transformCallback(callback: BridgeHandler) {
      const id = ++callbackSeq;
      callbacks.set(id, callback);
      return id;
    },
    async invoke(cmd: string, args: Record<string, unknown> = {}) {
      calls.push({ cmd, args });
      switch (cmd) {
        case 'plugin:event|listen': {
          const name = String(args.event);
          const handlerId = Number(args.handler);
          listeners.set(name, [...(listeners.get(name) ?? []), handlerId]);
          return `evt-${handlerId}`;
        }
        case 'plugin:window|get_all_windows':
          return ['console', 'dual'];
        case 'plugin:event|unlisten':
        case 'plugin:window|show':
        case 'plugin:window|minimize':
        case 'plugin:window|close':
        case 'start_session':
        case 'stop_session':
          return null;
        default:
          return null;
      }
    },
  };

  window.__tauriEmit = (event, payload) => {
    for (const handlerId of listeners.get(event) ?? []) {
      const handler = callbacks.get(handlerId);
      if (handler) handler({ event, id: handlerId, payload });
    }
  };
}

/** Pushes a Rust event through the same path the event plugin uses. */
async function emit(page: Page, payload: unknown): Promise<void> {
  await page.evaluate((data) => window.__tauriEmit?.('session', data), payload);
}

/** The webview only receives events once the event plugin registered its
 *  listeners; wait for that before emitting so the test never races mount. */
async function waitForListeners(page: Page): Promise<void> {
  await expect
    .poll(async () =>
      (await page.evaluate(() => window.__tauriCalls ?? [])).filter(
        (call) => call.cmd === 'plugin:event|listen',
      ).length,
    )
    .toBeGreaterThanOrEqual(3);
}

// --------------------------------------------------------------- the phone ---

const HOST = '127.0.0.1';
const TOKEN = 'e2e-token';

interface MockServer {
  url: string;
  /** Resolves with each accepted client, in connection order. */
  nextSocket: () => Promise<WebSocket>;
  close: () => Promise<void>;
}

/** The desktop's LAN server stand-in: speaks @nextalk/protocol over real WS. */
async function startMockServer(): Promise<MockServer> {
  const wss = new WebSocketServer({ host: HOST, port: 0 });
  await new Promise<void>((resolve, reject) => {
    wss.once('listening', resolve);
    wss.once('error', reject);
  });

  const address = wss.address();
  if (typeof address !== 'object' || address === null) {
    throw new Error('mock WS server did not bind a TCP port');
  }

  const arrived: WebSocket[] = [];
  const waiters: Array<(socket: WebSocket) => void> = [];
  wss.on('connection', (socket) => {
    const waiter = waiters.shift();
    if (waiter) waiter(socket);
    else arrived.push(socket);
  });

  return {
    url: `ws://${HOST}:${address.port}`,
    nextSocket: () => {
      const queued = arrived.shift();
      if (queued) return Promise.resolve(queued);
      return new Promise<WebSocket>((resolve) => waiters.push(resolve));
    },
    close: () =>
      new Promise<void>((resolve) => {
        for (const client of wss.clients) client.terminate();
        wss.close(() => resolve());
      }),
  };
}

// --------------------------------------------------------------- the frames ---

const ERROR_CODE = 'retry_exhausted';
const ZH_ORIGINAL = '我们把查询拆成了两条 SQL，并且加了 Redis 缓存层。';
const EN_RECOVERED = 'We split the query into two statements and added a Redis cache layer.';

const TRACE = {
  segmentStartMs: 620,
  termHits: [],
  provider: 'volc',
  modelVersion: 'icl-2.0',
  confidenceSource: 'proxy',
};

/** GOV-10/D-07: the degradation travels as data — the original with the
 *  aggregatable error code and no English at all. */
const DEGRADED_FRAME = {
  t: 'subtitle',
  id: 'u1',
  speaker: 'user',
  seq: 1,
  zh: ZH_ORIGINAL,
  final: true,
  trace: { ...TRACE, errorCode: ERROR_CODE },
};

/** The same segment, recovered: English present, errorCode gone. */
const RECOVERED_FRAME = {
  t: 'subtitle',
  id: 'u1',
  speaker: 'user',
  seq: 2,
  zh: ZH_ORIGINAL,
  en: EN_RECOVERED,
  final: true,
  trace: TRACE,
};

test.describe('desktop degraded bubble', () => {
  test.use({ viewport: { width: 860, height: 680 } });

  test('renders the locked degraded form, then clears it when the segment recovers', async ({
    page,
  }) => {
    test.skip(test.info().project.name !== 'desktop', '经桌面项目运行：需要 1420 的 Tauri IPC mock');
    await page.addInitScript(installTauriMock);
    await page.goto('/#/dual');
    await waitForListeners(page);

    const subtitles = page.getByRole('region', { name: '实时字幕' });
    await emit(page, DEGRADED_FRAME);

    const badge = subtitles.getByText('翻译失败');
    await expect(badge).toBeVisible();
    await expect(badge).toHaveAttribute('data-error-code', ERROR_CODE);
    await expect(subtitles.getByText('翻译服务暂时不可用')).toBeVisible();
    await expect(subtitles.getByText(ZH_ORIGINAL)).toBeVisible();
    await expect(subtitles.getByText('正在重试')).toBeVisible();
    await expect(subtitles.getByText(/We split the query/)).toHaveCount(0);

    // Same segment id → the healthy event replaces the degraded form.
    await emit(page, RECOVERED_FRAME);

    await expect(subtitles.getByText('翻译失败')).toHaveCount(0);
    await expect(subtitles.getByText('翻译服务暂时不可用')).toHaveCount(0);
    await expect(subtitles.getByText('正在重试')).toHaveCount(0);
    await expect(subtitles.getByText(ZH_ORIGINAL)).toBeVisible();
    await expect(page.locator('[data-testid="subtitle-stream"] > div')).toHaveCount(1);
  });
});

test.describe('phone degraded bubble', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('renders the identical degraded form, then clears it when the segment recovers', async ({
    page,
  }) => {
    test.skip(
      test.info().project.name !== 'teleprompter',
      '经手机项目运行：需要 8791 的 H5 预览 + WS mock',
    );
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);
      const socket = await mock.nextSocket();

      socket.send(JSON.stringify(DEGRADED_FRAME));

      const subs = page.locator('#panel-subs');
      const badge = subs.getByText('翻译失败');
      await expect(badge).toBeVisible();
      await expect(badge).toHaveAttribute('data-error-code', ERROR_CODE);
      await expect(subs.getByText('翻译服务暂时不可用')).toBeVisible();
      await expect(subs.getByText(ZH_ORIGINAL)).toBeVisible();
      await expect(subs.getByText('正在重试')).toBeVisible();
      await expect(subs.getByText(/We split the query/)).toHaveCount(0);

      socket.send(JSON.stringify(RECOVERED_FRAME));

      await expect(subs.getByText('翻译失败')).toHaveCount(0);
      await expect(subs.getByText('翻译服务暂时不可用')).toHaveCount(0);
      await expect(subs.getByText('正在重试')).toHaveCount(0);
      await expect(subs.getByText(ZH_ORIGINAL)).toBeVisible();
      await expect(subs.locator('article')).toHaveCount(1);
    } finally {
      await mock.close();
    }
  });
});
