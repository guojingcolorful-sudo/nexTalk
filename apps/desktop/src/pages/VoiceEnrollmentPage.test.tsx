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
