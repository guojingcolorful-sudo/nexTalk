import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import VoiceEnrollmentPage from './VoiceEnrollmentPage';
import { MOCK_VOICE_READING_TEXT } from '../data/mock-data';

/**
 * T4.3 四步注册向导 — the page is the real console for enrollment now: the
 * Rust recorder owns the microphone (`start/stop_enrollment_recording`), the
 * level meter polls `enrollment_level`, training goes through
 * `train_voice_clone`, and the header badge mirrors the resolved voice from
 * `get_voice_profile` — the same `resolve_voice()` the cascade reads (T-02-20:
 * the badge must never say 克隆 while the preset is what speaks).
 *
 * Copy is Chinese-locked here: 开始录音 / 重新录制 / 正在训练音色… / 训练失败 /
 * 当前音色：预置 / 当前音色：我的克隆.
 */

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const PRESET_STATUS = {
  profile: null,
  voice: { kind: 'preset', name: 'zh_female_vv_uranus_bigtts' },
  warning: null,
};

const CLONE_STATUS = {
  profile: {
    speakerId: 'S_demo',
    resourceId: 'seed-icl-2.0',
    createdAt: '2026-10-05T09:00:00Z',
    samplePath: '/app/enroll/take-1.wav',
    durationS: 61.2,
    status: 'ready',
    previous: [],
  },
  voice: { kind: 'clone', name: 'S_demo' },
  warning: null,
};

const TAKE = {
  path: '/app/enroll/take-1.wav',
  durationS: 61.2,
  silenceRatio: 0.18,
  bytes: 1_958_444,
};

/** The profile a successful 重新训练 leaves behind (T4.4): a new speaker, the
 *  old one kept in `previous` as the rollback anchor. */
const RETRAINED_CLONE_STATUS = {
  profile: {
    speakerId: 'S_demo_2',
    resourceId: 'seed-icl-2.0',
    createdAt: '2026-10-05T10:00:00Z',
    samplePath: TAKE.path,
    durationS: 61.2,
    status: 'ready',
    previous: [CLONE_STATUS.profile],
  },
  voice: { kind: 'clone', name: 'S_demo_2' },
  warning: null,
};

const previewResult = (speakerId: string) => ({
  voice: { kind: 'clone', name: speakerId },
  bytes: 76_800,
  durationMs: 1_600,
});

type CommandHandler = (args?: unknown) => Promise<unknown> | unknown;

/** Routes each command; overrides win over the happy-path default. */
function mockBackend(overrides: Record<string, CommandHandler> = {}) {
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    const override = overrides[command];
    if (override !== undefined) return Promise.resolve(override(args));
    switch (command) {
      case 'get_voice_profile':
        return Promise.resolve(PRESET_STATUS);
      case 'start_enrollment_recording':
        return Promise.resolve(null);
      case 'enrollment_level':
        return Promise.resolve(0.35);
      case 'stop_enrollment_recording':
        return Promise.resolve(TAKE);
      case 'train_voice_clone':
        return Promise.resolve(CLONE_STATUS);
      default:
        return Promise.resolve(null);
    }
  });
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/voice']}>
      <VoiceEnrollmentPage />
    </MemoryRouter>,
  );
}

const flush = async () => {
  await act(async () => {});
};

const stopCalls = () =>
  invokeMock.mock.calls.filter(([command]) => command === 'stop_enrollment_recording');

/** Walks to the take screen and starts the Rust recorder. */
async function startTake() {
  fireEvent.click(screen.getByRole('button', { name: '下一步' }));
  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: '开始录音' }));
  });
}

describe('VoiceEnrollmentPage 四步注册向导', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    mockBackend();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  test('the badge says 预置 while no clone is ready', async () => {
    renderPage();
    await flush();
    expect(screen.getByText('当前音色：预置')).toBeTruthy();
  });

  test('the badge says 我的克隆 when the profile resolved to a clone', async () => {
    mockBackend({ get_voice_profile: () => CLONE_STATUS });
    renderPage();
    await flush();
    expect(screen.getByText('当前音色：我的克隆')).toBeTruthy();
  });

  test('a corrupt profile surfaces its warning instead of failing silently', async () => {
    mockBackend({
      get_voice_profile: () => ({
        profile: null,
        voice: { kind: 'preset', name: 'zh_female_vv_uranus_bigtts' },
        warning: '音色档案已损坏，已回退到预置音色，可重新训练修复',
      }),
    });
    renderPage();
    await flush();
    expect(screen.getByText('音色档案已损坏，已回退到预置音色，可重新训练修复')).toBeTruthy();
    expect(screen.getByText('当前音色：预置')).toBeTruthy();
  });

  test('a recorder that cannot open names the failure and stays ready', async () => {
    mockBackend({
      start_enrollment_recording: () =>
        Promise.reject({ code: 'device', message: '麦克风不可用：没有可用的输入设备' }),
    });
    renderPage();
    await flush();

    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '开始录音' }));
    });

    expect(screen.getByText('麦克风不可用：没有可用的输入设备')).toBeTruthy();
    expect(screen.getByRole('button', { name: '开始录音' })).toBeTruthy();
  });

  test('开始录音 starts the Rust take; the meter polls and the timer counts', async () => {
    renderPage();
    await flush();
    await startTake();

    expect(invokeMock).toHaveBeenCalledWith('start_enrollment_recording');
    expect(screen.getByRole('button', { name: '停止录音' })).toBeTruthy();

    await act(async () => {
      vi.advanceTimersByTime(1100);
    });

    const levelCalls = invokeMock.mock.calls.filter(([command]) => command === 'enrollment_level');
    expect(levelCalls.length).toBeGreaterThanOrEqual(10);
    expect(screen.getByTestId('recording-timer').textContent).toContain('00:01');
    expect(screen.getByTestId('level-meter')).toBeTruthy();
  });

  test('停止录音 advances to training, and training swaps in the clone badge', async () => {
    let settleTraining: ((status: unknown) => void) | undefined;
    mockBackend({
      train_voice_clone: () =>
        new Promise((resolve) => {
          settleTraining = resolve;
        }),
    });
    renderPage();
    await flush();
    await startTake();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '停止录音' }));
    });
    expect(invokeMock).toHaveBeenCalledWith('stop_enrollment_recording');
    expect(screen.getByRole('button', { name: '开始训练' })).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '开始训练' }));
    });
    expect(invokeMock).toHaveBeenCalledWith('train_voice_clone', {
      samplePath: TAKE.path,
      transcript: MOCK_VOICE_READING_TEXT,
    });
    expect(screen.getByRole('button', { name: '正在训练音色…' })).toBeTruthy();

    await act(async () => {
      settleTraining?.(CLONE_STATUS);
    });
    expect(screen.getByRole('button', { name: '完成' })).toBeTruthy();
    expect(screen.getByText('当前音色：我的克隆')).toBeTruthy();
  });

  test('a take that fails the guard shows the Chinese reason and offers 重新录制', async () => {
    mockBackend({
      stop_enrollment_recording: () =>
        Promise.reject({ code: 'too_short', message: '录音太短，请至少朗读 1 分钟（当前 15 秒）' }),
    });
    renderPage();
    await flush();
    await startTake();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '停止录音' }));
    });

    expect(screen.getByText('录音太短，请至少朗读 1 分钟（当前 15 秒）')).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新录制' }));
    });
    expect(screen.getByRole('button', { name: '停止录音' })).toBeTruthy();
  });

  test('训练失败 keeps the old voice and names the reason', async () => {
    mockBackend({
      train_voice_clone: () =>
        Promise.reject({ code: 'transcript_mismatch', message: '录音与文本不匹配，请重录' }),
    });
    renderPage();
    await flush();
    await startTake();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '停止录音' }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '开始训练' }));
    });

    expect(screen.getByText('训练失败')).toBeTruthy();
    expect(screen.getByText('录音与文本不匹配，请重录')).toBeTruthy();
    expect(screen.getByText('当前音色：预置')).toBeTruthy();
    expect(screen.getByRole('button', { name: '重试' })).toBeTruthy();
  });

  test('上一步 during a take stops the recorder once and never force-advances', async () => {
    renderPage();
    await flush();
    await startTake();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '上一步' }));
    });

    expect(stopCalls()).toHaveLength(1);
    expect(screen.getByRole('button', { name: '下一步' })).toBeTruthy();

    // The interval is gone: driving the clock past the take length must not
    // jump the wizard forward from wherever the user navigated to (WR-05).
    act(() => {
      vi.advanceTimersByTime(200_000);
    });
    expect(screen.getByRole('button', { name: '下一步' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '完成' })).toBeNull();
    expect(stopCalls()).toHaveLength(1);
  });

  test('the take stops itself at three minutes and lands in training', async () => {
    renderPage();
    await flush();
    await startTake();

    await act(async () => {
      vi.advanceTimersByTime(181_000);
    });

    expect(invokeMock).toHaveBeenCalledWith('stop_enrollment_recording');
    expect(screen.getByRole('button', { name: '开始训练' })).toBeTruthy();
  });
});

/**
 * T4.4 试听与重训 — step 4 synthesizes the two fixed preview lines with the
 * currently resolved voice (nothing cached: every click re-synthesizes, so a
 * retrain is audible immediately), 重新训练 reuses the stored sample, and a
 * failed retrain leaves the old clone in place (Test 4: never strand the user
 * with neither the old nor the new voice).
 */
describe('VoiceEnrollmentPage 试听与重训', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    mockBackend();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  /** Walks 准备 → 录音 → 停止 → 训练到步骤 4（试听）. */
  async function enrollToPreview() {
    await startTake();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '停止录音' }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '开始训练' }));
    });
  }

  test('试听 synthesizes the clicked kind and shows the playing state', async () => {
    let settlePreview: ((value: unknown) => void) | undefined;
    mockBackend({
      preview_voice: () =>
        new Promise((resolve) => {
          settlePreview = resolve;
        }),
    });
    renderPage();
    await flush();
    await enrollToPreview();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });
    expect(invokeMock).toHaveBeenCalledWith('preview_voice', { kind: 'zh' });
    expect(screen.getByRole('button', { name: '正在播放…' })).toBeTruthy();

    await act(async () => {
      settlePreview?.(previewResult('S_demo'));
    });
    expect(screen.getByRole('button', { name: '试听中文' })).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听英文' }));
    });
    expect(invokeMock).toHaveBeenCalledWith('preview_voice', { kind: 'en' });
    await act(async () => {
      settlePreview?.(previewResult('S_demo'));
    });
  });

  test('每一次试听都重新合成，不缓存上一次的结果', async () => {
    renderPage();
    await flush();
    await enrollToPreview();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });

    const previews = invokeMock.mock.calls.filter(([command]) => command === 'preview_voice');
    expect(previews).toHaveLength(2);
  });

  test('重新训练 reuses the stored sample and the next 试听 reflects the new voice', async () => {
    let speaker = 'S_demo';
    const previews: Array<{ kind: string; speakerId: string }> = [];
    mockBackend({
      train_voice_clone: () => {
        speaker = 'S_demo_2';
        return RETRAINED_CLONE_STATUS;
      },
      preview_voice: (args) => {
        const { kind } = args as { kind: string };
        previews.push({ kind, speakerId: speaker });
        return previewResult(speaker);
      },
    });
    renderPage();
    await flush();
    await enrollToPreview();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新训练' }));
    });
    // The retrain uploads the sample the profile points at — the user is not
    // asked to record again (only one take was ever started).
    expect(invokeMock).toHaveBeenCalledWith('train_voice_clone', {
      samplePath: TAKE.path,
      transcript: MOCK_VOICE_READING_TEXT,
    });
    expect(invokeMock.mock.calls.filter(([c]) => c === 'start_enrollment_recording')).toHaveLength(
      1,
    );
    expect(screen.getByText('当前音色：我的克隆')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });
    expect(previews).toHaveLength(2);
    expect(previews[0].speakerId).not.toBe(previews[1].speakerId);
    expect(previews[1].speakerId).toBe('S_demo_2');
  });

  test('重训失败 keeps the old clone audible and names the reason', async () => {
    let attempts = 0;
    mockBackend({
      train_voice_clone: () => {
        attempts += 1;
        return attempts === 1
          ? CLONE_STATUS
          : Promise.reject({ code: 'retryable', message: '训练服务暂时不可用，请稍后重试' });
      },
    });
    renderPage();
    await flush();
    await enrollToPreview();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新训练' }));
    });

    expect(screen.getByText('训练失败')).toBeTruthy();
    expect(screen.getByText('训练服务暂时不可用，请稍后重试')).toBeTruthy();
    expect(screen.getByText('当前音色：我的克隆')).toBeTruthy();

    // The old voice still previews — the user is never stranded voiceless.
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '试听中文' }));
    });
    expect(invokeMock.mock.calls.filter(([c]) => c === 'preview_voice')).toHaveLength(1);
    expect(screen.queryByText('试听失败')).toBeNull();
  });

  test('重训进行时 shows 正在训练音色…', async () => {
    let settleRetrain: ((value: unknown) => void) | undefined;
    let attempts = 0;
    mockBackend({
      train_voice_clone: () => {
        attempts += 1;
        if (attempts === 1) return CLONE_STATUS;
        return new Promise((resolve) => {
          settleRetrain = resolve;
        });
      },
    });
    renderPage();
    await flush();
    await enrollToPreview();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '重新训练' }));
    });
    expect(screen.getByRole('button', { name: '正在训练音色…' })).toBeTruthy();

    await act(async () => {
      settleRetrain?.(RETRAINED_CLONE_STATUS);
    });
    expect(screen.getByRole('button', { name: '重新训练' })).toBeTruthy();
  });

  test('删除音色档案 confirms on the second click and flips the badge back to 预置', async () => {
    let deleted = false;
    mockBackend({
      get_voice_profile: () => (deleted ? PRESET_STATUS : CLONE_STATUS),
      delete_voice_profile: () => {
        deleted = true;
        return { profileRemoved: true, samples: [TAKE.path] };
      },
    });
    renderPage();
    await flush();
    await enrollToPreview();
    expect(screen.getByText('当前音色：我的克隆')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '删除音色档案' }));
    });
    // First click only arms the confirm — nothing destroyed yet.
    expect(invokeMock.mock.calls.some(([c]) => c === 'delete_voice_profile')).toBe(false);

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '确认删除？' }));
    });
    expect(invokeMock).toHaveBeenCalledWith('delete_voice_profile');
    expect(screen.getByText('当前音色：预置')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '重新训练' })).toBeNull();
  });
});
