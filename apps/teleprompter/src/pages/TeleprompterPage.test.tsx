import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import App from '../App';
import { nextLanguagePref } from './TeleprompterPage';

/**
 * 01-04 Task 1 gate: the phone surface renders the locked copy, the tab
 * switch is URL state, and a subtitle frame that survived the isServerEvent
 * gate reaches the DOM. Typewriter timing is covered by useTypewriter's own
 * suite, so these specs stub reduced motion and assert committed text.
 */

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];

  readyState = 0;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = 3;
  }

  accept(): void {
    this.readyState = 1;
    this.onopen?.();
  }

  emit(payload: unknown): void {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }

  frames(): Array<Record<string, unknown>> {
    return this.sent.map((frame) => JSON.parse(frame) as Record<string, unknown>);
  }
}

function currentSocket(): FakeWebSocket {
  const socket = FakeWebSocket.instances.at(-1);
  if (!socket) throw new Error('no WebSocket opened');
  return socket;
}

/** Reduced motion makes the typewriter commit text instantly (deterministic). */
function stubReducedMotion(): void {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  );
}

const SUBTITLE = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: '你的数据库查询在负载下变慢了……',
  en: 'Your database queries are slowing down under load.',
  final: true,
};

beforeEach(() => {
  FakeWebSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeWebSocket);
  stubReducedMotion();
  Element.prototype.scrollIntoView = vi.fn();
  // jsdom has no media pipeline; the wake-lock fallback only needs play() to
  // resolve (the hook ignores the returned promise either way).
  Object.defineProperty(HTMLMediaElement.prototype, 'play', {
    value: vi.fn().mockResolvedValue(undefined),
    configurable: true,
    writable: true,
  });
  Object.defineProperty(HTMLMediaElement.prototype, 'pause', {
    value: vi.fn(),
    configurable: true,
    writable: true,
  });
  window.history.replaceState(null, '', '/?token=e2e-token&ws=ws://127.0.0.1:8788');
});

afterEach(() => {
  cleanup();
  // The fallback video is appended to <body>, outside the React root.
  document.body.querySelectorAll('video').forEach((video) => video.remove());
  vi.unstubAllGlobals();
});

describe('TeleprompterPage', () => {
  test('renders the locked pre-session copy and the 开始提词 gate', () => {
    render(<App />);

    expect(screen.getByText('等待语音输入')).toBeTruthy();
    expect(screen.getByText('模拟会话开始后，双语字幕将显示在这里')).toBeTruthy();
    expect(screen.getByText('已配对桌面端')).toBeTruthy();
    expect(screen.getByRole('button', { name: '开始提词' })).toBeTruthy();
  });

  test('starts the session on tap and flips the gate to 暂停提词', () => {
    render(<App />);
    const socket = currentSocket();

    act(() => {
      socket.accept();
      screen.getByRole('button', { name: '开始提词' }).click();
    });

    expect(screen.getByRole('button', { name: '暂停提词' })).toBeTruthy();
    // UAT-5: 开始提词 is the same function as the desktop's 开始模拟会话 —
    // the tap pushes the session action over the wire.
    const actionFrames = socket.frames().filter((frame) => frame.action !== undefined);
    expect(actionFrames).toEqual([{ t: 'control', action: 'start_session' }]);
  });

  test('a live status from the desktop flips the gate to 暂停提词 (UAT-5 bidirectional)', () => {
    render(<App />);
    const socket = currentSocket();

    // The desktop starts the session on its own — the phone follows.
    act(() => {
      socket.accept();
      socket.emit({ t: 'status', session: 'listening' });
    });

    expect(screen.getByRole('button', { name: '暂停提词' })).toBeTruthy();
  });

  test('暂停提词 pushes stop_session to the desktop (UAT-5 bidirectional)', () => {
    render(<App />);
    const socket = currentSocket();

    act(() => {
      socket.accept();
      screen.getByRole('button', { name: '开始提词' }).click();
    });
    act(() => {
      screen.getByRole('button', { name: '暂停提词' }).click();
    });

    const actionFrames = socket.frames().filter((frame) => frame.action !== undefined);
    expect(actionFrames).toEqual([
      { t: 'control', action: 'start_session' },
      { t: 'control', action: 'stop_session' },
    ]);
  });

  test('switches to the AI 辅助 tab, persists it in the URL, and restores it on remount', () => {
    const first = render(<App />);

    act(() => {
      screen.getByRole('tab', { name: 'AI 辅助' }).click();
    });

    expect(screen.getByText('AI 策略将自动生成')).toBeTruthy();
    expect(screen.getByText('提问结束后，策略卡片会出现在这里')).toBeTruthy();
    expect(window.location.search).toContain('tab=ai');
    // The pairing token must survive the tab rewrite.
    expect(window.location.search).toContain('token=e2e-token');

    first.unmount();
    render(<App />);

    expect(screen.getByRole('tab', { name: 'AI 辅助' }).getAttribute('aria-selected')).toBe(
      'true',
    );
  });

  test('renders a validated subtitle frame and drops malformed ones', () => {
    render(<App />);
    const socket = currentSocket();

    act(() => {
      socket.emit(SUBTITLE);
      socket.emit({ t: 'subtitle', id: 'evil', speaker: 'user', zh: '不应渲染', final: true });
    });

    expect(screen.getByText(SUBTITLE.en)).toBeTruthy();
    expect(screen.getByText(SUBTITLE.zh)).toBeTruthy();
    expect(screen.queryByText('不应渲染')).toBeNull();
  });

  test('the bubble filters to the session language mode (UAT-4)', () => {
    render(<App />);
    const socket = currentSocket();

    act(() => {
      socket.accept();
      socket.emit(SUBTITLE); // interviewer: both languages present on the wire
      socket.emit({ t: 'language', language: 'all-zh' });
    });

    // 中: only the Chinese line renders — the English original is filtered
    // out, not reordered under it.
    expect(screen.getByText(SUBTITLE.zh)).toBeTruthy();
    expect(screen.queryByText(SUBTITLE.en)).toBeNull();

    act(() => {
      socket.emit({ t: 'language', language: 'all-en' });
    });

    // EN: only the English line renders.
    expect(screen.getByText(SUBTITLE.en)).toBeTruthy();
    expect(screen.queryByText(SUBTITLE.zh)).toBeNull();

    act(() => {
      socket.emit({ t: 'language', language: 'bilingual' });
    });

    // EN+中: both lines render.
    expect(screen.getByText(SUBTITLE.en)).toBeTruthy();
    expect(screen.getByText(SUBTITLE.zh)).toBeTruthy();
  });

  test('开始提词 engages the stay-awake fallback on a plain http LAN origin', () => {
    render(<App />);

    act(() => {
      screen.getByRole('button', { name: '开始提词' }).click();
    });

    // jsdom has no navigator.wakeLock — the same situation as http://192.168.x.x.
    const video = document.body.querySelector('video');
    expect(video).not.toBeNull();
    expect(video?.muted).toBe(true);
    expect(video?.loop).toBe(true);
    expect(screen.getByText('已启用防休眠回退模式')).toBeTruthy();
    expect(screen.getByText('屏幕常亮已开启')).toBeTruthy();
  });

  test('the mode control pushes the exact protocol control frame to the desktop', () => {
    render(<App />);
    const socket = currentSocket();

    act(() => {
      socket.accept();
    });
    act(() => {
      screen.getByRole('button', { name: /语言模式/ }).click();
    });

    const frames = socket.frames();
    expect(frames[0]).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 0 });
    expect(frames[1]).toEqual({ t: 'control', language: 'bilingual' }); // WR-03 re-assert
    expect(frames.at(-1)).toEqual({ t: 'control', language: 'all-zh' });
    expect(socket.sent.join('')).not.toContain('language_pref');
  });

  test('adopts the echoed language event as the source of truth (WR-03)', () => {
    render(<App />);
    const socket = currentSocket();
    act(() => {
      socket.accept();
    });

    // The desktop echoes the mode the SESSION is in (a second phone or a
    // reloaded page would otherwise render its own stale default).
    act(() => {
      socket.emit({ t: 'language', language: 'all-en' });
    });

    expect(screen.getByRole('button', { name: /语言模式 EN，/ })).toBeTruthy();

    // The next tap cycles from the echoed value, not from the local default.
    act(() => {
      screen.getByRole('button', { name: /语言模式 EN，/ }).click();
    });
    const frames = socket.frames();
    expect(frames.at(-1)).toEqual({ t: 'control', language: 'bilingual' });
  });

  test('without a token the page shows the pairing error instead of the teleprompter', () => {
    window.history.replaceState(null, '', '/');

    render(<App />);

    expect(screen.getByText('连接已失效，请重新扫码')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '开始提词' })).toBeNull();
  });
});

describe('nextLanguagePref', () => {
  test('cycles 中 → EN → EN+中 → 中 (the locked segments)', () => {
    expect(nextLanguagePref('all-zh')).toBe('all-en');
    expect(nextLanguagePref('all-en')).toBe('bilingual');
    expect(nextLanguagePref('bilingual')).toBe('all-zh');
  });
});
