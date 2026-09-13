import { test, expect, type Page } from '@playwright/test';

/**
 * demo.spec.ts — the one-button demo run (SYNC-01 / SYNC-03, DSK-04, UI-02).
 *
 * The desktop app talks to Rust over Tauri IPC, which does not exist in a
 * browser. installTauriMock() stands in for the IPC bridge before the app
 * boots: it records every command call (so the spec can prove a click reached
 * the command surface), answers get_pairing_info and the window APIs, and
 * exposes __tauriEmit() so the spec can push the exact payloads Rust publishes
 * (`session` / `session_status` / `phone_count`) into the webview.
 *
 * The event payloads below are copied from src-tauri/src/sim/script.rs, so the
 * mocked stream and the simulated one carry the same script.
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
        case 'interrupt':
        case 'repeat':
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
const DUAL_VIEWPORT = { width: 860, height: 680 };

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
    .toBeGreaterThanOrEqual(1);
}

async function emittedCommands(page: Page): Promise<string[]> {
  return (await calls(page)).map((call) => call.cmd);
}

// The locked sim script (src-tauri/src/sim/script.rs).
const QUESTION_EN =
  'Could you walk me through the specific steps you took to optimize the database?';
const QUESTION_ZH = '你能详细说一下你优化数据库的具体步骤吗？';
const ANSWER_ZH = '首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。';
const ANSWER_EN =
  'First we analysed the slow query log and found the bottleneck was a multi-table join on the product detail page.';
const STRATEGY_BULLETS = ['慢查询日志定位', '拆连表查询', 'Redis 缓存层'];
const R2_EN = 'What would you do when a production service degrades at 2 AM?';
const R2_ZH = '如果凌晨两点线上服务出现性能退化，你会怎么处理？';

const QUESTION_EVENT = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: QUESTION_ZH,
  en: QUESTION_EN,
  final: true,
};
const ANSWER_EVENT = {
  t: 'subtitle',
  id: 'r1-a',
  speaker: 'user',
  seq: 2,
  zh: ANSWER_ZH,
  en: ANSWER_EN,
  final: true,
};
const STRATEGY_EVENT = {
  t: 'strategy',
  id: 's-r1',
  roundId: 'r1',
  title: '数据库优化',
  bullets: STRATEGY_BULLETS,
};
const R2_QUESTION_EVENT = {
  t: 'subtitle',
  id: 'r2-q',
  speaker: 'interviewer',
  seq: 3,
  zh: R2_ZH,
  en: R2_EN,
  final: true,
};
/** 重听 re-emits the round with a fresh id and seq (the engine's `-r1` suffix). */
const R2_REPLAY_EVENT = { ...R2_QUESTION_EVENT, id: 'r2-q-r1', seq: 5 };

test.describe('console demo run', () => {
  test.use({ viewport: CONSOLE_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('starts the session and walks the CTA through 停止 / 打断 / 重听', async ({ page }) => {
    await page.goto('/#/console');
    await waitForListeners(page);

    // 扩展视图 needs a live session: idle offers nothing to extend.
    const dualButton = page.getByRole('button', { name: '扩展视图' });
    await expect(dualButton).toBeDisabled();

    await page.getByRole('button', { name: '开始模拟会话' }).click();
    expect(await emittedCommands(page)).toContain('start_session');
    // UAT-5 bidirectional: the desktop's own start reveals 扩展视图 too —
    // exactly like a phone-initiated start does.
    await expect
      .poll(async () => (await calls(page)).filter((call) => call.cmd === 'plugin:window|show'))
      .toEqual([{ cmd: 'plugin:window|show', args: { label: 'dual' } }]);

    // Rust answers with the session status; the action bar becomes the live run.
    await emit(page, 'session_status', { session: 'listening' });
    await expect(page.getByRole('button', { name: '会话进行中' })).toBeDisabled();
    await expect(dualButton).toBeEnabled();
    // UAT-7 final: a live conversation has no 打断/重听 — the controls are
    // gone from the product (the sim engine keeps its internal replay
    // machinery for the 复盘 surface in a later phase).
    await expect(page.getByRole('button', { name: '打断' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '重听' })).toHaveCount(0);

    await dualButton.click();
    await expect
      .poll(async () => (await calls(page)).filter((call) => call.cmd === 'plugin:window|show'))
      .toEqual([
        { cmd: 'plugin:window|show', args: { label: 'dual' } },
        { cmd: 'plugin:window|show', args: { label: 'dual' } },
      ]);

    // The answer is generating: the mic pill state moves, no extra controls.
    await emit(page, 'session_status', { session: 'generating' });
    await expect(page.getByRole('button', { name: '回答生成中' })).toBeDisabled();
    const afterControls = await emittedCommands(page);
    expect(afterControls).not.toContain('interrupt');
    expect(afterControls).not.toContain('repeat');

    // 停止 is destructive: it goes through the locked confirmation first.
    await page.getByRole('button', { name: '停止', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('停止会话？');
    await expect(dialog).toContainText('当前字幕与策略将清空');
    await dialog.getByRole('button', { name: '取消' }).click();
    expect(await emittedCommands(page)).not.toContain('stop_session');

    await page.getByRole('button', { name: '停止', exact: true }).click();
    await dialog.getByRole('button', { name: '停止' }).click();
    expect(await emittedCommands(page)).toContain('stop_session');

    // Ended: the action bar returns to the start CTA, ready for a re-run.
    await emit(page, 'session_status', { session: 'ended' });
    await expect(page.getByRole('button', { name: '开始模拟会话' })).toBeEnabled();
  });

  test('recreates the extended view after its window was closed (WR-08)', async ({ page }) => {
    await page.goto('/#/console');
    await waitForListeners(page);

    // The dual window's own close control destroys it: the label is gone from
    // the live window list, which is what getByLabel consults.
    await page.evaluate(() => {
      const internals = window.__TAURI_INTERNALS__ as {
        invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
      };
      const original = internals.invoke.bind(internals);
      internals.invoke = (cmd, args = {}) =>
        cmd === 'plugin:window|get_all_windows'
          ? Promise.resolve(['console'])
          : original(cmd, args);
    });

    await emit(page, 'session_status', { session: 'listening' });
    await page.getByRole('button', { name: '扩展视图' }).click();

    // 扩展视图 must bring it back, not silently do nothing.
    await expect
      .poll(async () => (await calls(page)).filter((call) => call.cmd === 'plugin:webview|create_webview_window'))
      .toHaveLength(1);
    const commands = await emittedCommands(page);
    expect(commands).not.toContain('plugin:window|show'); // nothing to show
    await expect(page.getByText('扩展视图打开失败')).toHaveCount(0);
  });

  test('surfaces a failed extended-view open instead of swallowing it (WR-08)', async ({ page }) => {
    await page.goto('/#/console');
    await waitForListeners(page);

    await page.evaluate(() => {
      const internals = window.__TAURI_INTERNALS__ as {
        invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
      };
      const original = internals.invoke.bind(internals);
      internals.invoke = (cmd, args = {}) => {
        if (cmd === 'plugin:window|get_all_windows') return Promise.resolve(['console']);
        if (cmd === 'plugin:webview|create_webview_window') {
          return Promise.reject(new Error('window creation failed'));
        }
        return original(cmd, args);
      };
    });

    await emit(page, 'session_status', { session: 'listening' });
    await page.getByRole('button', { name: '扩展视图' }).click();

    await expect(page.getByText('扩展视图打开失败')).toBeVisible();
    await expect(page.getByRole('button', { name: '重试' })).toBeVisible();
  });

  test('shows the live phone count the desktop publishes', async ({ page }) => {
    await page.goto('/#/console');
    await waitForListeners(page);

    await expect(page.getByText('等待扫码')).toBeVisible();

    await emit(page, 'phone_count', { count: 1 });
    await expect(page.getByText('已连接 1 台设备')).toBeVisible();

    await emit(page, 'phone_count', { count: 0 });
    await expect(page.getByText('等待扫码')).toBeVisible();
  });
});

test.describe('dual pane demo run', () => {
  test.use({ viewport: DUAL_VIEWPORT });

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('streams the interview, cuts a round, replays it, and follows the phone mode', async ({
    page,
  }) => {
    await page.goto('/#/dual');
    await waitForListeners(page);

    const subtitles = page.getByRole('region', { name: '实时字幕' });
    const ai = page.getByRole('region', { name: 'AI 辅助' });

    // The interviewer asks: both languages land in the stream.
    await emit(page, 'session_status', { session: 'listening' });
    await expect(page.getByText('麦克风开启-监听中')).toBeVisible();
    await emit(page, 'session', QUESTION_EVENT);
    await expect(subtitles).toContainText(QUESTION_EN);
    await expect(subtitles).toContainText(QUESTION_ZH);

    // The answer streams and the copilot card lands.
    await emit(page, 'session', ANSWER_EVENT);
    await emit(page, 'session', STRATEGY_EVENT);
    await emit(page, 'session_status', { session: 'generating' });
    await expect(subtitles).toContainText(ANSWER_ZH);
    // The user's bubble opens Chinese-only (the English line is what the cloned
    // voice speaks) — the phone's mode is what brings it out.
    await expect(subtitles.getByText(ANSWER_EN)).toHaveCount(0);
    await expect(ai).toContainText('数据库优化');
    await expect(page.getByRole('status', { name: '正在生成' })).toBeVisible();

    // UAT-7 final: a live conversation has no 打断/重听 controls — the
    // buttons are gone from the extended view too.
    await expect(page.getByRole('button', { name: '打断' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '重听' })).toHaveCount(0);

    // The next round opens on the same stream.
    await emit(page, 'session', R2_QUESTION_EVENT);
    await expect(subtitles).toContainText(R2_EN);

    // A replayed round carries a fresh id (`-r{n}`) — a new bubble, never a
    // duplicate; the renderer keys by id so both lines coexist.
    await emit(page, 'session', R2_REPLAY_EVENT);
    await expect(subtitles.getByText(R2_EN)).toHaveCount(2);

    // SYNC-03: the phone applies all-en; the desktop follows on the same
    // stream and every untouched bubble drops its Chinese line — including the
    // user's, whose cloned English was previously hidden.
    await emit(page, 'session', { t: 'language', language: 'all-en' });
    await expect(subtitles.getByText(QUESTION_ZH)).toHaveCount(0);
    await expect(subtitles.getByText(R2_ZH)).toHaveCount(0);
    await expect(subtitles.getByText(ANSWER_ZH)).toHaveCount(0);
    await expect(subtitles).toContainText(QUESTION_EN);
    await expect(subtitles).toContainText(ANSWER_EN);

    // Ended: the mic pill and the generating indicator both stand down.
    await emit(page, 'session_status', { session: 'ended' });
    await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
    await expect(page.getByRole('status', { name: '正在生成' })).toHaveCount(0);
  });
});
