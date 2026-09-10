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
