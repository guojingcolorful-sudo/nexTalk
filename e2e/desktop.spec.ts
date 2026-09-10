import { test, expect, type Locator, type Page } from '@playwright/test';

/**
 * desktop.spec.ts — the desktop surface end-to-end (DSK-01/02/04, UI-01/02).
 *
 * The desktop app talks to Rust over Tauri IPC, which does not exist in a
 * browser. installTauriMock() stands in for the IPC bridge before the app
 * boots: it records every command call (so the spec can assert that a click
 * really reached the command surface), answers get_pairing_info and the window
 * APIs, and exposes __tauriEmit() so the spec can push Rust events into the
 * webview through the same path the real event plugin uses.
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

interface SessionHandler {
  (event: { event: string; id: number; payload: unknown }): void;
}

/** Serialized into the page — must stay self-contained (no outer references). */
function installTauriMock(): void {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  const callbacks = new Map<number, SessionHandler>();
  const listeners = new Map<string, number[]>();
  const eventIds = new Map<string, number>();
  let callbackSeq = 0;
  let eventSeq = 0;

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
    transformCallback(callback: SessionHandler) {
      const id = ++callbackSeq;
      callbacks.set(id, callback);
      return id;
    },
    async invoke(cmd: string, args: Record<string, unknown> = {}) {
      calls.push({ cmd, args });
      switch (cmd) {
        case 'get_pairing_info':
          if (window.location.hash.includes('failPairing')) {
            throw new Error('lan server unavailable');
          }
          return { url: 'http://192.168.1.10:8787/?token=e2e00token', port: 8787 };
        case 'plugin:event|listen': {
          const name = String(args.event);
          const handlerId = Number(args.handler);
          const token = `evt-${++eventSeq}`;
          eventIds.set(token, handlerId);
          listeners.set(name, [...(listeners.get(name) ?? []), handlerId]);
          return token;
        }
        case 'plugin:event|unlisten': {
          const handlerId = eventIds.get(String(args.eventId));
          if (handlerId !== undefined) {
            const name = String(args.event);
            listeners.set(name, (listeners.get(name) ?? []).filter((id) => id !== handlerId));
          }
          return null;
        }
        case 'plugin:window|get_all_windows':
          return ['console', 'dual'];
        case 'start_session':
        case 'stop_session':
        case 'plugin:window|show':
        case 'plugin:window|minimize':
        case 'plugin:window|close':
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

const CONSOLE_VIEWPORT = { width: 340, height: 680 };

const HUB_ROUTES = [
  { control: '模拟简历.pdf', path: '/resume', title: '简历导入' },
  { control: '术语表', path: '/glossary', title: '术语表' },
  { control: '音色注册', path: '/voice', title: '音色注册' },
  { control: '录音资产', path: '/recordings', title: '录音资产' },
  { control: '复盘报告', path: '/review', title: '复盘报告' },
  { control: '设置', path: '/setup', title: '引导向导' },
];

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(() => window.__tauriCalls ?? []);
}

/** Pushes a Rust event through the same path the event plugin uses. */
async function emit(page: Page, event: string, payload: unknown): Promise<void> {
  await page.evaluate(
    ([name, data]) => window.__tauriEmit?.(name, data),
    [event, payload] as [string, unknown],
  );
}

/** The webview only receives events once the event plugin has registered the
 *  listener; wait for that invoke before emitting, so a test never races the
 *  app's mount effect. */
async function waitForListeners(page: Page): Promise<void> {
  await expect
    .poll(async () => (await calls(page)).filter((call) => call.cmd === 'plugin:event|listen').length)
    .toBeGreaterThanOrEqual(3);
}

/** Vertical position, used to prove the widget's fixed card order. */
async function yOf(locator: Locator): Promise<number> {
  const box = await locator.boundingBox();
  expect(box, 'element must be laid out').not.toBeNull();
  return box?.y ?? -1;
}

test.describe('console hub', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('renders the hub cards in the locked order', async ({ page }) => {
    await page.goto('/#/console');

    await expect(page.getByText('NEXTALK')).toBeVisible();
    await expect(page.getByText('极言')).toBeVisible();

    const stealth = page.getByRole('button', { name: /隐形模式/ });
    const qr = page.getByRole('region', { name: '同步手机' });
    const resumeRow = page.getByRole('button', { name: /模拟简历\.pdf/ });
    const assetSection = page.getByRole('region', { name: '本地资产' });

    await expect(stealth).toBeVisible();
    await expect(stealth).toContainText('Cmd + Shift + H');
    await expect(qr).toBeVisible();
    await expect(page.getByText('扫码开启手机跨端辅助展示')).toBeVisible();
    await expect(resumeRow).toBeVisible();
    await expect(page.getByRole('button', { name: /术语表/ })).toContainText('3 个术语');
    await expect(page.getByRole('button', { name: /音色注册/ })).toContainText('未注册');
    await expect(assetSection).toBeVisible();

    expect(await yOf(stealth)).toBeLessThan(await yOf(qr));
    expect(await yOf(qr)).toBeLessThan(await yOf(resumeRow));
    expect(await yOf(resumeRow)).toBeLessThan(await yOf(assetSection));
  });

  test('renders the live pairing QR from get_pairing_info', async ({ page }) => {
    await page.goto('/#/console');

    const qrImage = page.getByAltText('手机配对二维码');
    await expect(qrImage).toBeVisible();
    expect(await qrImage.getAttribute('src')).toMatch(/^data:image\/png/);
    // No phone_count event has arrived yet (01-05 emits it) — the card says so
    // instead of inventing a connection count.
    await expect(page.getByText('等待扫码')).toBeVisible();
    expect((await calls(page)).map((call) => call.cmd)).toContain('get_pairing_info');
  });

  test('routes every hub control to its page and back', async ({ page }) => {
    await page.goto('/#/console');

    for (const entry of HUB_ROUTES) {
      await page.getByRole('button', { name: new RegExp(entry.control) }).click();
      await expect(page).toHaveURL(new RegExp(`#${entry.path}$`));
      await expect(page.getByTestId('page-stub')).toContainText(entry.title);

      await page.getByRole('button', { name: '返回控制台' }).click();
      await expect(page).toHaveURL(/#\/console$/);
      await expect(page.getByRole('button', { name: '开始模拟会话' })).toBeVisible();
    }
  });

  test('开始模拟会话 reaches start_session and locks the CTA', async ({ page }) => {
    await page.goto('/#/console');

    await page.getByRole('button', { name: '开始模拟会话' }).click();

    const running = page.getByRole('button', { name: '会话进行中' });
    await expect(running).toBeVisible();
    await expect(running).toBeDisabled();
    expect((await calls(page)).map((call) => call.cmd)).toContain('start_session');
  });

  test('扩展视图 asks the hidden dual window to show', async ({ page }) => {
    await page.goto('/#/console');

    await page.getByRole('button', { name: '扩展视图' }).click();

    await expect
      .poll(async () => (await calls(page)).filter((call) => call.cmd === 'plugin:window|show'))
      .toEqual([{ cmd: 'plugin:window|show', args: { label: 'dual' } }]);
  });

  test('StealthCard flashes the window transparent', async ({ page }) => {
    await page.goto('/#/console');

    const stealth = page.getByRole('button', { name: /隐形模式/ });
    await expect(stealth).toHaveCSS('opacity', '1');

    await stealth.click();
    await expect(stealth).toHaveCSS('opacity', '0');
    // Transitional feedback only — the window comes back on its own because
    // real orderOut hiding is Phase 4.
    await expect(stealth).toHaveCSS('opacity', '1', { timeout: 5000 });
  });

  test('shows the locked pairing failure copy when pairing info fails', async ({ page }) => {
    await page.goto('/#/console?failPairing=1');

    const banner = page.getByRole('alert');
    await expect(banner).toContainText('配对失败');
    await expect(banner).toContainText('请确认手机与电脑连接同一 Wi-Fi，然后重新扫码');

    const before = (await calls(page)).filter((call) => call.cmd === 'get_pairing_info').length;
    await page.getByRole('button', { name: '重试' }).click();
    await expect
      .poll(async () => (await calls(page)).filter((call) => call.cmd === 'get_pairing_info').length)
      .toBeGreaterThan(before);
  });
});

/** The locked sim script (src-tauri/src/sim/script.rs) — the dual pane must
 *  render exactly this content. */
const QUESTION_EN =
  'Could you walk me through the specific steps you took to optimize the database?';
const QUESTION_ZH = '你能详细说一下你优化数据库的具体步骤吗？';
const ANSWER_ZH = '首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。';
const STRATEGY_BULLETS = ['慢查询日志定位', '拆连表查询', 'Redis 缓存层'];

const QUESTION_EVENT = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: QUESTION_ZH,
  en: QUESTION_EN,
  final: true,
};
const ANSWER_EVENT = { t: 'subtitle', id: 'r1-a', speaker: 'user', seq: 2, zh: ANSWER_ZH, final: true };
const STRATEGY_EVENT = {
  t: 'strategy',
  id: 's-r1',
  roundId: 'r1',
  title: '数据库优化',
  bullets: STRATEGY_BULLETS,
};

test.describe('dual pane extended view', () => {
  test.use({ viewport: { width: 860, height: 680 } });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('renders both panes with their locked empty states', async ({ page }) => {
    await page.goto('/#/dual');

    const subtitles = page.getByRole('region', { name: '实时字幕' });
    const ai = page.getByRole('region', { name: 'AI 辅助' });

    await expect(page.getByRole('heading', { name: '扩展视图' })).toBeVisible();
    await expect(subtitles.getByRole('heading', { name: '实时字幕' })).toBeVisible();
    await expect(ai.getByRole('heading', { name: 'AI 辅助' })).toBeVisible();
    await expect(subtitles).toContainText('等待语音输入');
    await expect(subtitles).toContainText('模拟会话开始后，双语字幕将显示在这里');
    await expect(ai).toContainText('AI 策略将自动生成');
    await expect(ai).toContainText('提问结束后，策略卡片会出现在这里');
    // Idle session: no capture pill claims the mic is live.
    await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
  });

  test('streams the simulated question, answer, and strategy into place', async ({ page }) => {
    await page.goto('/#/dual');
    await waitForListeners(page);

    await emit(page, 'session', QUESTION_EVENT);
    const subtitles = page.getByRole('region', { name: '实时字幕' });
    await expect(subtitles).toContainText(QUESTION_EN);
    await expect(subtitles).toContainText(QUESTION_ZH);
    await expect(subtitles).toContainText('面试官');

    await emit(page, 'session', ANSWER_EVENT);
    await expect(subtitles).toContainText(ANSWER_ZH);
    await expect(subtitles).toContainText('用户');
    await expect(subtitles).not.toContainText('等待语音输入');

    await emit(page, 'session', STRATEGY_EVENT);
    const ai = page.getByRole('region', { name: 'AI 辅助' });
    await expect(ai).toContainText('数据库优化');
    for (const bullet of STRATEGY_BULLETS) {
      await expect(ai.getByRole('listitem').filter({ hasText: bullet })).toBeVisible();
    }
    await expect(ai).not.toContainText('AI 策略将自动生成');
  });

  test('keeps each bubble language choice independent', async ({ page }) => {
    await page.goto('/#/dual');
    await waitForListeners(page);
    await emit(page, 'session', QUESTION_EVENT);
    await emit(page, 'session', ANSWER_EVENT);

    const interviewerToggle = page.getByRole('group', { name: '面试官语言' });
    const userToggle = page.getByRole('group', { name: '用户语言' });
    await expect(interviewerToggle.getByRole('button', { name: 'EN+中', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(userToggle.getByRole('button', { name: '中', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    await interviewerToggle.getByRole('button', { name: '中', exact: true }).click();

    // Only the interviewer bubble reacted — its English line is gone from the
    // stream (the AI pane's context card keeps the original wording) …
    const subtitles = page.getByRole('region', { name: '实时字幕' });
    await expect(subtitles.getByText(QUESTION_EN)).toHaveCount(0);
    await expect(subtitles).toContainText(QUESTION_ZH);
    // … the user bubble kept its own preference and its text.
    await expect(page.getByText(ANSWER_ZH)).toBeVisible();
    await expect(userToggle.getByRole('button', { name: '中', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  test('drops a malformed payload before it can render', async ({ page }) => {
    await page.goto('/#/dual');
    await waitForListeners(page);
    await emit(page, 'session', QUESTION_EVENT);
    await expect(page.getByRole('region', { name: '实时字幕' })).toContainText(QUESTION_EN);

    const bubblesBefore = await page.locator('[data-testid="subtitle-stream"] > div').count();

    // seq must be a number — the narrowing guard rejects the whole payload.
    await emit(page, 'session', {
      t: 'subtitle',
      id: 'bad-1',
      speaker: 'interviewer',
      seq: 'not-a-number',
      en: 'MALFORMED-SHOULD-NOT-RENDER',
      final: true,
    });
    // A payload that is not a ServerEvent at all is dropped too.
    await emit(page, 'session', { t: 'subtitle-hostile', en: 'MALFORMED-SHOULD-NOT-RENDER' });

    await expect(page.getByText('MALFORMED-SHOULD-NOT-RENDER')).toHaveCount(0);
    await expect(page.locator('[data-testid="subtitle-stream"] > div')).toHaveCount(bubblesBefore);
  });

  test('reflects listening, generating, and ended session states', async ({ page }) => {
    await page.goto('/#/dual');
    await waitForListeners(page);

    await emit(page, 'session_status', { session: 'listening' });
    await expect(page.getByText('麦克风开启-监听中')).toBeVisible();

    await emit(page, 'session_status', { session: 'generating' });
    await expect(page.getByRole('status', { name: '正在生成' })).toBeVisible();

    await emit(page, 'session_status', { session: 'ended' });
    await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
    await expect(page.getByRole('status', { name: '正在生成' })).toHaveCount(0);
  });
});
