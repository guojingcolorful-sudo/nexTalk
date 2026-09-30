import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import DualPanePage from './DualPanePage';

type SessionHandler = (event: { event: string; id: number; payload: unknown }) => void;

/** Listener registry shared with the mocked Tauri modules. */
const { handlers, invokeMock } = vi.hoisted(() => ({
  handlers: new Map<string, SessionHandler[]>(),
  invokeMock: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, handler: SessionHandler) => {
    handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    return () => undefined;
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

function emit(event: string, payload: unknown): void {
  for (const handler of handlers.get(event) ?? []) {
    handler({ event, id: 0, payload });
  }
}

/** Wait until the hook's listen() calls have registered on the mock. */
async function waitForListeners(event: string): Promise<void> {
  await waitFor(() => expect(handlers.get(event)?.length ?? 0).toBeGreaterThan(0));
}

/** Renders the page and waits for both Rust channels to be subscribed —
 *  anything emitted before that is dropped with the listener. */
async function renderDualPane(): Promise<void> {
  render(<DualPanePage />);
  await waitForListeners('session');
  await waitForListeners('session_status');
}

const STOP_LABEL = { name: '停止' };
const PILL_COPY = '麦克风开启-监听中';

const QUESTION_EVENT = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: '你能详细说一下你优化数据库的具体步骤吗？',
  en: 'Could you walk me through the specific steps you took to optimize the database?',
  final: true,
};

beforeEach(() => {
  handlers.clear();
  invokeMock.mockClear();
  // Reduced motion: the typewriter renders full text immediately (no timers to
  // advance) — this test targets the stopped state, not the typing cadence.
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('DualPanePage stop control', () => {
  it('shows neither the pill nor 停止 while the session is idle', async () => {
    await renderDualPane();

    expect(screen.queryByText(PILL_COPY)).toBeNull();
    expect(screen.queryByRole('button', STOP_LABEL)).toBeNull();
  });

  it('shows 停止 next to the pill for the listening and generating statuses', async () => {
    await renderDualPane();

    act(() => {
      emit('session_status', { session: 'listening' });
    });
    expect(screen.getByText(PILL_COPY)).toBeTruthy();
    expect(screen.getByRole('button', STOP_LABEL)).toBeTruthy();

    act(() => {
      emit('session_status', { session: 'generating' });
    });
    expect(screen.getByRole('button', STOP_LABEL)).toBeTruthy();
  });

  it('opens the locked confirm and 取消 leaves the session untouched', async () => {
    await renderDualPane();
    act(() => {
      emit('session_status', { session: 'listening' });
    });

    fireEvent.click(screen.getByRole('button', STOP_LABEL));

    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('停止会话？')).toBeTruthy();
    expect(within(dialog).getByText('当前字幕与策略将清空')).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: '取消' })).toBeTruthy();
    expect(within(dialog).getByRole('button', STOP_LABEL)).toBeTruthy();

    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }));

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(screen.getByRole('button', STOP_LABEL)).toBeTruthy();
  });

  it('stop_session runs once when 停止 is confirmed, then the dialog closes', async () => {
    await renderDualPane();
    act(() => {
      emit('session_status', { session: 'listening' });
    });

    fireEvent.click(screen.getByRole('button', STOP_LABEL));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', STOP_LABEL));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
    expect(invokeMock).toHaveBeenCalledWith('stop_session');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('empties both panes when Rust publishes the terminal status after 停止', async () => {
    await renderDualPane();

    act(() => {
      emit('session_status', { session: 'listening' });
      emit('session', QUESTION_EVENT);
    });
    const subtitles = screen.getByRole('region', { name: '实时字幕' });
    expect(within(subtitles).getByText(QUESTION_EVENT.en)).toBeTruthy();

    fireEvent.click(screen.getByRole('button', STOP_LABEL));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', STOP_LABEL));

    // Rust 停止 publishes the terminal state on both channels, in this order.
    act(() => {
      emit('session_status', { session: 'ended' });
      emit('session', { t: 'status', session: 'ended' });
    });

    expect(within(subtitles).getByText('等待语音输入')).toBeTruthy();
    expect(within(subtitles).queryByText(QUESTION_EVENT.en)).toBeNull();
    expect(screen.getByText('AI 策略将自动生成')).toBeTruthy();
    expect(screen.queryByText(PILL_COPY)).toBeNull();
    expect(screen.queryByRole('button', STOP_LABEL)).toBeNull();
  });
});
