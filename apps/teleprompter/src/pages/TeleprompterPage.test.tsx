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

  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
  }

  close(): void {
    // no-op: the hook only needs the handle
  }

  emit(payload: unknown): void {
    this.onmessage?.({ data: JSON.stringify(payload) });
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
  window.history.replaceState(null, '', '/?token=e2e-token&ws=ws://127.0.0.1:8788');
});

afterEach(() => {
  cleanup();
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

    act(() => {
      screen.getByRole('button', { name: '开始提词' }).click();
    });

    expect(screen.getByRole('button', { name: '暂停提词' })).toBeTruthy();
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
