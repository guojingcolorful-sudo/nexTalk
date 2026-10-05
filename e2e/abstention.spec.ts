import { expect, test, type Page } from '@playwright/test';
import { WebSocketServer, type WebSocket } from 'ws';

/**
 * abstention.spec.ts — 02-03 T3.6 e2e: silent abstention on BOTH surfaces
 * (D-03), and the GOV-01/02 witness that abstention is NOT a confidence badge.
 *
 * An abstained segment is its own event (`t: 'abstained'`): the audio held
 * nothing translatable, so there is no subtitle text and no fabricated line —
 * the bubble shows the locked 「待翻译」 state instead. Low confidence never
 * produces this event and never renders a badge on the subtitle (2026-09-30
 * revision: confidence is provenance, not UI). A background subtitle that
 * arrives afterwards joins the stream as its own bubble — the abstained state
 * stays visible as history.
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
 *  degraded.spec.ts, trimmed to what the dual pane needs). */
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

const ZH_NEXT = '第二句有内容';
const EN_NEXT = 'The second line has content.';

/** D-03: the segment produced no valid text — its own event, no subtitle. */
const ABSTAINED_FRAME = {
  t: 'abstained',
  id: 'u1',
  speaker: 'user',
  seq: 1,
  reason: 'silent_audio',
  segmentStartMs: 620,
};

/** The next segment translates normally and joins the stream after it. */
const NEXT_FRAME = {
  t: 'subtitle',
  id: 'u2',
  speaker: 'user',
  seq: 2,
  zh: ZH_NEXT,
  en: EN_NEXT,
  final: true,
  trace: {
    segmentStartMs: 900,
    termHits: [],
    provider: 'volc',
    modelVersion: 'icl-2.0',
    confidenceSource: 'proxy',
  },
};

test.describe('desktop abstained bubble', () => {
  test.use({ viewport: { width: 860, height: 680 } });

  test('renders 待翻译 with no confidence badge, then keeps it as history', async ({
    page,
  }) => {
    test.skip(test.info().project.name !== 'desktop', '经桌面项目运行：需要 1420 的 Tauri IPC mock');
    await page.addInitScript(installTauriMock);
    await page.goto('/#/dual');
    await waitForListeners(page);

    const subtitles = page.getByRole('region', { name: '实时字幕' });
    await emit(page, ABSTAINED_FRAME);

    await expect(subtitles.getByText('待翻译')).toBeVisible();
    // GOV-01/02 (2026-09-30): abstention is a state, not a confidence mark.
    await expect(subtitles.getByText('低置信')).toHaveCount(0);
    // …and it is not a failure either: no degraded form for a silent segment.
    await expect(subtitles.getByText('翻译失败')).toHaveCount(0);

    await emit(page, NEXT_FRAME);

    await expect(subtitles.getByText(ZH_NEXT)).toBeVisible();
    await expect(subtitles.getByText('待翻译')).toHaveCount(1);
    await expect(subtitles.getByText('低置信')).toHaveCount(0);
    await expect(page.locator('[data-testid="subtitle-stream"] > div')).toHaveCount(2);
  });
});

test.describe('phone abstained bubble', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('renders the identical 待翻译 state, then keeps it as history', async ({
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

      socket.send(JSON.stringify(ABSTAINED_FRAME));

      const subs = page.locator('#panel-subs');
      await expect(subs.getByText('待翻译')).toBeVisible();
      await expect(subs.getByText('低置信')).toHaveCount(0);
      await expect(subs.getByText('翻译失败')).toHaveCount(0);

      socket.send(JSON.stringify(NEXT_FRAME));

      await expect(subs.getByText(ZH_NEXT)).toBeVisible();
      await expect(subs.getByText('待翻译')).toHaveCount(1);
      await expect(subs.locator('article')).toHaveCount(2);
    } finally {
      await mock.close();
    }
  });
});
