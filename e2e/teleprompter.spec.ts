import { expect, test, type Page } from '@playwright/test';
import { WebSocketServer, type WebSocket } from 'ws';

/**
 * teleprompter.spec.ts — 01-04 Task 3 e2e (the complete phone surface).
 *
 * Five sections, all against a real WS mock server on an ephemeral port and
 * the `ws=` query override kept from 01-02 (the product port 8787 belongs to
 * the desktop LAN server and is squatted by an unrelated local tool here):
 *
 * 1. token mount — a pairing token renders the gate, a missing one the 重扫码 error
 * 2. full flow — the locked r1 answer types out (deterministic clock checkpoint)
 *    and 开始提词 engages the stay-awake fallback on a non-secure LAN origin
 * 3. toggle — tab switching stays local; the mode control emits the EXACT
 *    protocol frame (`language`, never `language_pref`)
 * 4. reconnect — a dropped transport shows 正在自动重连, resumes from the last
 *    seen seq, and the replay tail does not duplicate bubbles (SYNC-05)
 * 5. malformed frames are dropped by the isServerEvent gate (threat T-01-02)
 */

const HOST = '127.0.0.1';
const TOKEN = 'e2e-token';

/** Locked r1 payload — apps/desktop/src-tauri/src/sim/script.rs. */
const ANSWER_ZH = '首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。';
const QUESTION_ZH = '你能详细说一下你优化数据库的具体步骤吗？';
const QUESTION_EN =
  'Could you walk me through the specific steps you took to optimize the database?';

/** useTypewriter reveals one character per 40ms tick. */
const TYPEWRITER_INTERVAL_MS = 40;
const TYPEWRITER_CHECKPOINT_CHARS = 20;

const ANSWER_FRAME = {
  t: 'subtitle',
  id: 'r1-a',
  speaker: 'user',
  seq: 2,
  zh: ANSWER_ZH,
  final: true,
};

const QUESTION_FRAME = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: QUESTION_ZH,
  en: QUESTION_EN,
  final: true,
};

const STRATEGY_FRAME = {
  t: 'strategy',
  id: 's-r1',
  roundId: 'r1',
  title: '数据库优化',
  bullets: ['慢查询日志定位', '拆连表查询'],
};

const LINE_5_ZH = '第五行字幕内容';
const LINE_6_ZH = '第六行字幕内容';

const SUBTITLE_5 = { t: 'subtitle', id: 'u5', speaker: 'user', seq: 5, zh: LINE_5_ZH, final: true };
const SUBTITLE_6 = { t: 'subtitle', id: 'u6', speaker: 'user', seq: 6, zh: LINE_6_ZH, final: true };

const START_BUTTON = '开始提词';

interface MockServer {
  url: string;
  /** Resolves with each accepted client, in connection order. */
  nextSocket: () => Promise<WebSocket>;
  /** Frames the client sent on this socket, parsed, in arrival order. */
  framesOf: (socket: WebSocket) => Array<Record<string, unknown>>;
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

  const framesBySocket = new Map<WebSocket, Array<Record<string, unknown>>>();
  const arrived: WebSocket[] = [];
  const waiters: Array<(socket: WebSocket) => void> = [];

  wss.on('connection', (socket) => {
    const frames: Array<Record<string, unknown>> = [];
    framesBySocket.set(socket, frames);
    socket.on('message', (raw) => {
      try {
        frames.push(JSON.parse(String(raw)) as Record<string, unknown>);
      } catch {
        // The H5 only ever sends JSON; ignore anything else.
      }
    });
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
    framesOf: (socket) => framesBySocket.get(socket) ?? [],
    close: () =>
      new Promise<void>((resolve) => {
        // Force-terminate leftovers so the next spec binds a fresh port cleanly.
        for (const client of wss.clients) client.terminate();
        wss.close(() => resolve());
      }),
  };
}

/**
 * Node-side poll. Playwright's own auto-waiting polls with requestAnimationFrame
 * *inside the page*, so it stalls while the page clock is frozen (section 2) —
 * this reads the DOM through evaluate() and sleeps in Node instead.
 */
async function waitUntil(page: Page, condition: () => boolean, what: string): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (await page.evaluate(condition)) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`timed out waiting for ${what}`);
}

test.use({ viewport: { width: 390, height: 844 } });

test.describe('phone teleprompter H5', () => {
  test('token mount: a pairing token renders the gate, a missing one the 重扫码 error', async ({
    page,
  }) => {
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);

      await expect(page.getByRole('button', { name: START_BUTTON })).toBeVisible();
      await expect(page.getByText('已配对桌面端')).toBeVisible();

      // `?token=` (empty) is a pairing failure, not a connection failure.
      await page.goto('/?token=');

      await expect(page.getByText('连接已失效，请重新扫码')).toBeVisible();
      await expect(page.getByRole('button', { name: START_BUTTON })).toHaveCount(0);
    } finally {
      await mock.close();
    }
  });

  test('full flow: the r1 answer types out to the checkpoint and 开始提词 engages the wake fallback', async ({
    page,
  }) => {
    const mock = await startMockServer();
    try {
      // http://localhost IS a secure context, so the real Screen Wake Lock API
      // would answer and no fallback video would ever be created. The LAN demo
      // runs on plain http://192.168.x.x (not secure, no API) — force that path.
      await page.addInitScript(() => {
        Object.defineProperty(Navigator.prototype, 'wakeLock', {
          configurable: true,
          get: () => undefined,
        });
      });

      // Freeze time before load, then advance exactly 20 ticks: the typewriter
      // checkpoint becomes arithmetic instead of a race against real time.
      // `install()` alone still lets real time through, which leaks a tick or
      // two while the bubble mounts — `pauseAt` is what makes it exact.
      await page.clock.install();
      await page.clock.pauseAt(new Date());
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);

      const socket = await mock.nextSocket();
      socket.send(JSON.stringify(ANSWER_FRAME));

      const typedAnswer = page.locator('article[aria-label="我"] p').first();
      await waitUntil(
        page,
        () => document.querySelector('article[aria-label="我"] p') !== null,
        'the answer bubble to mount',
      );

      await page.clock.runFor(TYPEWRITER_CHECKPOINT_CHARS * TYPEWRITER_INTERVAL_MS);

      const shown = await typedAnswer.textContent();
      expect(shown).toBe(ANSWER_ZH.slice(0, TYPEWRITER_CHECKPOINT_CHARS));
      expect(shown).not.toBe(ANSWER_ZH); // mid-flight, not a completed render

      // Let time flow again for Playwright's own actionability polling.
      await page.clock.resume();

      await page.getByRole('button', { name: START_BUTTON }).click();

      await expect(page.getByRole('button', { name: '暂停提词' })).toBeVisible();
      await expect(page.getByText('屏幕常亮已开启')).toBeVisible();
      await expect(page.getByText('已启用防休眠回退模式')).toBeVisible();

      const fallbackVideo = page.locator('body > video');
      await expect(fallbackVideo).toHaveCount(1);
      await expect(fallbackVideo).toHaveAttribute('aria-hidden', 'true');
      await expect(fallbackVideo).toHaveJSProperty('muted', true);
      await expect(fallbackVideo).toHaveJSProperty('loop', true);
      await expect(fallbackVideo).toHaveJSProperty('playsInline', true);
      await expect(fallbackVideo).toHaveCSS('opacity', '0');
      await expect(fallbackVideo).toHaveCSS('pointer-events', 'none');
    } finally {
      await mock.close();
    }
  });

  test('tab switching stays local and the mode control emits the exact protocol frame', async ({
    page,
  }) => {
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);
      const socket = await mock.nextSocket();

      // Every (re)open opens with the resume cursor, then re-asserts the mode
      // the phone last chose (WR-03) so a tap during a drop is never lost.
      await expect.poll(() => mock.framesOf(socket).length).toBe(2);
      expect(mock.framesOf(socket)[0]).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 0 });
      expect(mock.framesOf(socket)[1]).toEqual({ t: 'control', language: 'bilingual' });

      await page.getByRole('tab', { name: 'AI 辅助' }).click();
      await expect(page.getByText('AI 策略将自动生成')).toBeVisible();
      expect(mock.framesOf(socket)).toHaveLength(2); // reading a tab never talks

      // The phone's session default is EN+中 (bilingual): two taps cycle
      // EN+中 → 中 → EN. Each tap waits for the committed label, so the cycle
      // can never read a stale languagePref.
      await page.getByRole('button', { name: /语言模式 EN\+中，/ }).click();
      await page.getByRole('button', { name: /语言模式 中，/ }).click();
      await expect(page.getByRole('button', { name: /语言模式 EN，/ })).toBeVisible();

      await expect.poll(() => mock.framesOf(socket).length).toBe(4);
      const control = mock.framesOf(socket).at(-1);
      expect(control).toEqual({ t: 'control', language: 'all-en' });
      expect(Object.keys(control ?? {})).toEqual(['t', 'language']);
      expect(JSON.stringify(mock.framesOf(socket))).not.toContain('language_pref');
    } finally {
      await mock.close();
    }
  });

  test('a dropped socket shows 正在自动重连 and resumes from the last seen seq', async ({ page }) => {
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);
      const first = await mock.nextSocket();

      await expect.poll(() => mock.framesOf(first).length).toBe(2);
      expect(mock.framesOf(first)[0]).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 0 });
      expect(mock.framesOf(first)[1]).toEqual({ t: 'control', language: 'bilingual' });

      first.send(JSON.stringify(SUBTITLE_5));
      await expect(page.locator('article[aria-label="我"] p').first()).toHaveText(LINE_5_ZH);

      // Wifi blip: the transport dies with no clean close frame.
      first.terminate();

      await expect(page.getByText('正在自动重连')).toBeVisible();

      // The 1s ladder reconnects and asks for everything after seq 5 — with
      // the session epoch and the re-asserted mode (WR-03) behind it.
      const second = await mock.nextSocket();
      await expect(page.getByText('实时同步中')).toBeVisible();
      await expect.poll(() => mock.framesOf(second).length).toBe(2);
      expect(mock.framesOf(second)[0]).toEqual({ t: 'resume', sinceSeq: 5, sinceEpoch: 0 });
      expect(mock.framesOf(second)[1]).toEqual({ t: 'control', language: 'bilingual' });

      // The replay tail repeats the already-seen seq 5: the phone must add the
      // new line and never duplicate the bubble it already rendered.
      second.send(JSON.stringify({ t: 'timeline', events: [SUBTITLE_5, SUBTITLE_6] }));

      await expect(page.locator('article[aria-label="我"]')).toHaveCount(2);
      await expect(page.getByText(LINE_5_ZH)).toHaveCount(1);
      await expect(page.getByText(LINE_6_ZH)).toHaveCount(1);
    } finally {
      await mock.close();
    }
  });

  test('a restarted session clears the phone and streams the new one (CR-01)', async ({ page }) => {
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);
      const socket = await mock.nextSocket();

      // Session 1 ends with the phone holding a cursor at seq 8 and a strategy
      // card already rendered.
      socket.send(JSON.stringify(SUBTITLE_5));
      socket.send(JSON.stringify(SUBTITLE_6));
      socket.send(JSON.stringify({ ...STRATEGY_FRAME, title: '旧策略' }));
      await expect(page.getByText(LINE_5_ZH)).toBeVisible();
      await expect(page.getByText(LINE_6_ZH)).toBeVisible();

      // 停止 → 开始模拟会话: the timeline restarts, so seq 1/2 and "s-r1" are
      // both "already seen" to a phone that never drops its cursors.
      socket.send(JSON.stringify({ t: 'session_started', epoch: 2 }));
      socket.send(JSON.stringify(QUESTION_FRAME));
      socket.send(JSON.stringify(ANSWER_FRAME));
      socket.send(JSON.stringify(STRATEGY_FRAME));

      // The previous session is gone, not stacked under the new one.
      await expect(page.getByText(LINE_5_ZH)).toHaveCount(0);
      await expect(page.getByText(LINE_6_ZH)).toHaveCount(0);
      await expect(page.getByText(QUESTION_ZH)).toBeVisible();
      await expect(page.getByText(ANSWER_ZH)).toBeVisible();

      await page.getByRole('tab', { name: 'AI 辅助' }).click();
      await expect(page.getByText('数据库优化')).toBeVisible();
      await expect(page.getByText('旧策略')).toHaveCount(0);
    } finally {
      await mock.close();
    }
  });

  test('malformed frames never render (isServerEvent gate)', async ({ page }) => {
    const mock = await startMockServer();
    try {
      await page.goto(`/?token=${TOKEN}&ws=${mock.url}`);
      const socket = await mock.nextSocket();

      // Ordered delivery: the poison frames are processed before the valid one,
      // so a rendered bubble proves they were dropped rather than merely queued.
      socket.send(JSON.stringify({ foo: 'bar' }));
      socket.send(
        JSON.stringify({ t: 'subtitle', id: 'evil', speaker: 'user', zh: '恶意载荷不应显示', final: true }),
      );
      socket.send(JSON.stringify(QUESTION_FRAME));

      await expect(page.locator('article')).toHaveCount(1);
      await expect(page.getByText('恶意载荷不应显示')).toHaveCount(0);
    } finally {
      await mock.close();
    }
  });
});
