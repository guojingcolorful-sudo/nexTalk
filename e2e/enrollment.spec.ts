import { test, expect, type Page } from '@playwright/test';

/**
 * enrollment.spec.ts — the four-step voice enrollment wizard (02-04 T4.4):
 * 采集 → 训练 → 试听 → 重训, driven entirely on the mock backend. No real
 * microphone and no vendor: the Tauri IPC bridge is stubbed before the app
 * boots and every command call is recorded, so the spec proves the flow
 * reached the right commands with the right arguments (the Rust half — the
 * recorder, the guard, the 火山 upload — is covered by `cargo test`).
 *
 * The copy assertions are the locked Chinese strings the vitest suite also
 * pins; a wording drift breaks both.
 */

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: {
      registerListener: () => void;
      unregisterListener: () => void;
    };
    __tauriCalls?: { cmd: string; args: Record<string, unknown> }[];
  }
}

/** The locked sentence the wizard asks the user to read
 *  (apps/desktop/src/data/mock-data.ts). */
const READING_TEXT =
  '在过去三年里，我主要负责后端服务的性能优化与稳定性建设，把核心接口的 P99 延迟从 800 毫秒降到了 200 毫秒以内。';

const TAKE = {
  path: '/Users/demo/Library/Application Support/com.nextalk.desktop/enroll/take-1.wav',
  durationS: 61.2,
  silenceRatio: 0.18,
  bytes: 1_958_444,
};

/** Serialized into the page — must stay self-contained (no outer references).
 *  Stateful like the real commands: training replaces the profile, and the
 *  preview synthesizes with whatever speaker is current. */
function installTauriMock(): void {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  let speaker: string | null = null;
  let trained = 0;

  const take = {
    path: '/Users/demo/Library/Application Support/com.nextalk.desktop/enroll/take-1.wav',
    durationS: 61.2,
    silenceRatio: 0.18,
    bytes: 1_958_444,
  };
  const cloneStatus = (speakerId: string, previous: unknown[]) => ({
    profile: {
      speakerId,
      resourceId: 'seed-icl-2.0',
      createdAt: '2026-10-05T09:00:00Z',
      samplePath: take.path,
      durationS: take.durationS,
      status: 'ready',
      previous,
    },
    voice: { kind: 'clone', name: speakerId },
    warning: null,
  });
  const presetStatus = {
    profile: null,
    voice: { kind: 'preset', name: 'zh_female_vv_uranus_bigtts' },
    warning: null,
  };

  window.__tauriCalls = calls;

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    registerListener: () => undefined,
    unregisterListener: () => undefined,
  };

  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'console' },
      windows: [{ label: 'console' }, { label: 'dual' }],
    },
    transformCallback: () => 1,
    async invoke(cmd: string, args: Record<string, unknown> = {}) {
      calls.push({ cmd, args });
      switch (cmd) {
        case 'get_voice_profile':
          return speaker === null ? presetStatus : cloneStatus(speaker, []);
        case 'start_enrollment_recording':
          return null;
        case 'enrollment_level':
          return 0.35;
        case 'stop_enrollment_recording':
          return take;
        case 'train_voice_clone': {
          trained += 1;
          const previous = speaker === null ? [] : [speaker];
          speaker = trained === 1 ? 'S_demo' : 'S_demo_2';
          await sleep(250); // long enough for the 正在训练音色… state
          return cloneStatus(speaker, previous);
        }
        case 'preview_voice':
          await sleep(150); // long enough for the 正在播放… state
          return {
            voice: { kind: speaker === null ? 'preset' : 'clone', name: speaker ?? 'preset' },
            bytes: 76_800,
            durationMs: 1_600,
          };
        case 'plugin:event|listen':
          return `evt-${calls.length}`;
        case 'plugin:event|unlisten':
        case 'plugin:window|get_all_windows':
          return null;
        default:
          return null;
      }
    },
  };
}

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(() => window.__tauriCalls ?? []);
}

test.describe('voice enrollment wizard', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(installTauriMock);
  });

  test('walks 采集 → 训练 → 试听 → 重训 on the mock backend', async ({ page }) => {
    await page.goto('/#/voice');

    // Step 1 准备: the arrival badge says preset — nothing is enrolled yet.
    await expect(page.getByText('当前音色：预置')).toBeVisible();
    await expect(page.getByRole('heading', { name: '准备录音' })).toBeVisible();
    await page.getByRole('button', { name: '下一步' }).click();

    // Step 2 录音: the Rust recorder owns the mic.
    await expect(page.getByRole('heading', { name: '录音 1-3 分钟' })).toBeVisible();
    await page.getByRole('button', { name: '开始录音' }).click();
    await expect(page.getByRole('button', { name: '停止录音' })).toBeVisible();
    await page.getByRole('button', { name: '停止录音' }).click();

    // Step 3 训练: the take summary comes from the guard's result.
    await expect(page.getByRole('heading', { name: '训练音色' })).toBeVisible();
    await expect(page.getByText('本次录音 61 秒 · 静音占比 18%')).toBeVisible();
    await page.getByRole('button', { name: '开始训练' }).click();
    await expect(page.getByRole('button', { name: '正在训练音色…' })).toBeVisible();

    // Step 4 试听: the badge flips to the clone the training produced.
    await expect(page.getByRole('heading', { name: '试听' })).toBeVisible();
    await expect(page.getByText('当前音色：我的克隆')).toBeVisible();

    // 试听中文 exercises the real command surface — fresh synthesis per click.
    await page.getByRole('button', { name: '试听中文' }).click();
    await expect(page.getByRole('button', { name: '正在播放…' })).toBeVisible();
    await expect(page.getByRole('button', { name: '试听中文' })).toBeVisible();

    // 重训 reuses the stored sample — the user records nothing new.
    await page.getByRole('button', { name: '重新训练' }).click();
    await expect(page.getByRole('button', { name: '正在训练音色…' })).toBeVisible();
    await expect(page.getByRole('button', { name: '重新训练' })).toBeVisible();
    await expect(page.getByText('当前音色：我的克隆')).toBeVisible();

    // The retrained voice previews without a re-record: no new take started.
    await page.getByRole('button', { name: '试听英文' }).click();
    await expect(page.getByRole('button', { name: '试听英文' })).toBeVisible();

    const recorded = await calls(page);
    const commands = recorded.map((call) => call.cmd);
    expect(commands.filter((cmd) => cmd === 'start_enrollment_recording')).toHaveLength(1);
    expect(commands.filter((cmd) => cmd === 'stop_enrollment_recording')).toHaveLength(1);

    const trains = recorded.filter((call) => call.cmd === 'train_voice_clone');
    expect(trains).toHaveLength(2);
    for (const train of trains) {
      expect(train.args).toEqual({ samplePath: TAKE.path, transcript: READING_TEXT });
    }

    const previews = recorded.filter((call) => call.cmd === 'preview_voice');
    expect(previews.map((preview) => preview.args)).toEqual([{ kind: 'zh' }, { kind: 'en' }]);
  });
});
