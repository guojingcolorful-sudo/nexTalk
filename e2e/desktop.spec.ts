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

/** Every hub control, its route, and the heading its real page renders. */
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
      await expect(page.getByRole('heading', { name: entry.title })).toBeVisible();

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
/** Serialized into the page: the mic request is refused (unavailable path). */
function denyMicrophone(): void {
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: { getUserMedia: () => Promise.reject(new Error('NotAllowedError')) },
  });
}

/** Serialized into the page: a silent fake stream so recording can start. */
function grantMicrophone(): void {
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: {
      getUserMedia: () => Promise.resolve({ getTracks: () => [{ stop: () => undefined }] }),
    },
  });
}

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

const VOICE_READING_TEXT =
  '在过去三年里，我主要负责后端服务的性能优化与稳定性建设，把核心接口的 P99 延迟从 800 毫秒降到了 200 毫秒以内。';

test.describe('setup wizard', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('walks the four steps and finishes back at the console', async ({ page }) => {
    await page.goto('/#/setup');

    await expect(page.getByRole('heading', { name: '引导向导' })).toBeVisible();
    await expect(page.getByText('模拟模式').first()).toBeVisible();
    await expect(page.getByRole('heading', { name: '欢迎使用极言' })).toBeVisible();

    await page.getByRole('button', { name: '下一步' }).click();
    await expect(page.getByRole('heading', { name: '安装 BlackHole' })).toBeVisible();
    await expect(page.getByText('打开随应用附带的 BlackHole 2ch.pkg 安装包。')).toBeVisible();

    await page.getByRole('button', { name: '下一步' }).click();
    await expect(page.getByRole('heading', { name: '检测与权限' })).toBeVisible();
    const checks = page.getByRole('region', { name: '环境检测' });
    await expect(checks).toContainText('未检测');
    await expect(checks).toContainText('模拟数据');

    await page.getByRole('button', { name: '重新检测' }).click();
    await expect(checks).toContainText('已就绪');

    await page.getByRole('button', { name: '下一步' }).click();
    await expect(page.getByRole('heading', { name: '配置完成' })).toBeVisible();

    await page.getByRole('button', { name: '完成' }).click();
    await expect(page).toHaveURL(/#\/console$/);
  });
});

test.describe('voice enrollment', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
    await page.addInitScript(denyMicrophone);
  });

  test('shows the locked mic-unavailable banner when the mic is refused', async ({ page }) => {
    await page.goto('/#/voice');

    await expect(page.getByRole('heading', { name: '音色注册' })).toBeVisible();
    await expect(page.getByText(VOICE_READING_TEXT)).toBeVisible();

    await page.getByRole('button', { name: '下一步' }).click();
    await expect(page.getByRole('heading', { name: '录音 1-3 分钟' })).toBeVisible();
    await expect(page.getByTestId('recording-countdown')).toHaveText('03:00');

    await page.getByRole('button', { name: '开始录音' }).click();

    const banner = page.getByRole('alert');
    await expect(banner).toContainText('麦克风不可用');
    await expect(banner).toContainText('请在 系统设置 → 隐私与安全性 → 麦克风 中允许访问');
    // Nothing was captured — the wizard stays on the recording step.
    await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
    await expect(page.getByTestId('recording-countdown')).toHaveText('03:00');
  });

  test('records with a countdown, then finishes on the sample step', async ({ page }) => {
    await page.addInitScript(grantMicrophone);
    await page.goto('/#/voice');

    await page.getByRole('button', { name: '下一步' }).click();
    await page.getByRole('button', { name: '开始录音' }).click();

    await expect(page.getByText('麦克风开启-监听中')).toBeVisible();
    await expect(page.getByTestId('recording-countdown')).toHaveText(/^0[23]:\d{2}$/);

    await page.getByRole('button', { name: '停止录音' }).click();
    await expect(page.getByRole('heading', { name: '试听与完成' })).toBeVisible();
    await expect(page.getByText('音色样本占位')).toBeVisible();
    await expect(page.getByText('模拟数据')).toBeVisible();
    await expect(page.getByRole('button', { name: '播放' })).toBeDisabled();

    await page.getByRole('button', { name: '完成' }).click();
    await expect(page).toHaveURL(/#\/console$/);
  });
});

test.describe('glossary', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('adds a term, refuses duplicates, and deletes through the confirm modal', async ({
    page,
  }) => {
    await page.goto('/#/glossary');

    await expect(page.getByRole('heading', { name: '术语表' })).toBeVisible();
    await expect(page.getByText('模拟数据')).toBeVisible();
    const list = page.getByRole('region', { name: '术语列表' });
    await expect(list.getByText('K8s', { exact: true })).toBeVisible();
    await expect(list.getByText('幂等性', { exact: true })).toBeVisible();
    await expect(list.getByText('backpressure', { exact: true })).toBeVisible();

    // Duplicate guard renders the field error.
    await page.getByLabel('术语名称').fill('K8s');
    await page.getByRole('button', { name: '添加术语' }).click();
    await expect(page.getByText('该术语已在术语表中')).toBeVisible();

    await page.getByLabel('术语名称').fill('灰度发布');
    await page.getByRole('button', { name: '添加术语' }).click();
    await expect(list.getByText('灰度发布', { exact: true })).toBeVisible();

    // Cancel keeps the term …
    await list.getByRole('button', { name: '删除术语 K8s' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('删除术语「K8s」？');
    await expect(dialog).toContainText('该术语将不再受保护');
    await dialog.getByRole('button', { name: '取消' }).click();
    await expect(list.getByText('K8s', { exact: true })).toBeVisible();

    // … confirming removes it.
    await list.getByRole('button', { name: '删除术语 K8s' }).click();
    await dialog.getByRole('button', { name: '删除' }).click();
    await expect(list.getByText('K8s', { exact: true })).toHaveCount(0);

    for (const term of ['幂等性', 'backpressure', '灰度发布']) {
      await list.getByRole('button', { name: `删除术语 ${term}` }).click();
      await dialog.getByRole('button', { name: '删除' }).click();
    }

    await expect(list.getByText('术语表为空')).toBeVisible();
    await expect(list.getByText('添加专有名词（如 K8s、幂等性），翻译时将保持原样')).toBeVisible();
    await expect(list.getByRole('button', { name: '添加术语' })).toBeVisible();
  });
});

test.describe('resume import', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('imports by drop, then removes the resume through the confirm modal', async ({ page }) => {
    await page.goto('/#/resume');

    await expect(page.getByRole('heading', { name: '简历导入' })).toBeVisible();
    const zone = page.getByTestId('file-drop-zone');
    await expect(zone).toContainText('尚未导入简历');
    await expect(zone).toContainText('导入 PDF 或 Word 简历，AI 策略将基于真实经历生成');
    await expect(zone.getByRole('button', { name: '导入简历' })).toBeVisible();

    // Drag-over state fills the zone before anything is dropped.
    await zone.dispatchEvent('dragover');
    await expect(zone).toHaveClass(/bg-portalGreen/);
    await expect(zone).toContainText('松手即导入');

    // Dropping imports the mock resume: file row + portalGreen success state.
    await zone.dispatchEvent('drop');
    const fileRow = page.getByRole('region', { name: '已导入简历' });
    await expect(fileRow).toContainText('模拟简历.pdf');
    await expect(fileRow).toContainText('248 KB');
    await expect(fileRow).toContainText('已就绪');
    await expect(fileRow).toContainText('模拟数据');

    await fileRow.getByRole('button', { name: '移除' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('移除简历？');
    await expect(dialog).toContainText('AI 策略将不再参考该简历');
    await dialog.getByRole('button', { name: '移除' }).click();

    await expect(page.getByTestId('file-drop-zone')).toContainText('尚未导入简历');
  });
});

test.describe('recordings', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('shows the mock dual-track record and deletes it behind the confirm modal', async ({
    page,
  }) => {
    await page.goto('/#/recordings');

    await expect(page.getByRole('heading', { name: '录音资产' })).toBeVisible();
    const list = page.getByRole('region', { name: '录音列表' });
    await expect(list).toContainText('2026-08-27 14:05');
    await expect(list).toContainText('18 分 42 秒');
    await expect(list).toContainText('用户轨');
    await expect(list).toContainText('面试官轨');
    await expect(list).toContainText('模拟数据');
    for (const format of ['SRT', 'Markdown', 'Word']) {
      await expect(list.getByRole('button', { name: format })).toBeDisabled();
    }

    // Cancel keeps the recording …
    const deleteButton = list.getByRole('button', { name: '删除录音 2026-08-27 14:05' });
    await deleteButton.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('删除录音？');
    await expect(dialog).toContainText('该会话的录音将被永久删除，不可恢复');
    await dialog.getByRole('button', { name: '取消' }).click();
    await expect(list).toContainText('2026-08-27 14:05');

    // … confirming deletes it and reveals the empty state.
    await deleteButton.click();
    await dialog.getByRole('button', { name: '删除' }).click();
    await expect(list).toContainText('暂无录音');
    await expect(list).toContainText('会话结束后，双轨录音会出现在这里');
  });
});

test.describe('review report', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('generates the mock report from the empty state', async ({ page }) => {
    await page.goto('/#/review');

    await expect(page.getByRole('heading', { name: '复盘报告' })).toBeVisible();
    await expect(page.getByText('暂无复盘报告')).toBeVisible();
    await expect(page.getByText('生成报告后，可查看 Action Items 与关键关注点')).toBeVisible();

    await page.getByRole('button', { name: '生成报告' }).click();

    const report = page.getByRole('region', { name: '复盘报告内容' });
    await expect(report).toBeVisible();
    await expect(report).toContainText('整体表现良好');
    await expect(report).toContainText('模拟数据');
    await expect(report.getByRole('heading', { name: 'Action Items' })).toBeVisible();
    await expect(report.getByRole('heading', { name: '关键关注点' })).toBeVisible();
    await expect(report.getByRole('heading', { name: '逐题回放' })).toBeVisible();
    await expect(
      report.getByRole('listitem').filter({ hasText: '慢查询日志' }).first(),
    ).toBeVisible();
    await expect(report.getByRole('button', { name: /你能详细说一下/ })).toBeDisabled();
  });
});
