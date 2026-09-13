import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { backoffDelay, useWs, type WsTicket } from './useWs';

/**
 * 01-04 Task 2 gate (SYNC-02 / SYNC-03 / SYNC-05):
 * - reconnect ladder is locked to 1s → 2s → 4s → 30s (cap, never a tight loop)
 * - resume replays from the last seen subtitle seq and never double-renders
 * - control frames carry the EXACT protocol field names the Rust server parses
 *   (deny_unknown_fields drops the connection on anything else)
 * - the isServerEvent gate + 64KB frame cap still hold (T-01-02 regression)
 */

class FakeSocket {
  static instances: FakeSocket[] = [];
  static OPEN = 1;

  readyState = 0;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = 3;
  }

  /** Server accepts the connection. */
  accept(): void {
    this.readyState = FakeSocket.OPEN;
    this.onopen?.();
  }

  /** Server pushes a frame. */
  push(payload: unknown): void {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }

  pushRaw(data: string): void {
    this.onmessage?.({ data });
  }

  /** Transport drops (wifi blip / server restart). */
  drop(): void {
    this.readyState = 3;
    this.onclose?.();
  }
}

const TICKET: WsTicket = { token: 'a'.repeat(32), url: 'ws://127.0.0.1:8799' };

function latest(): FakeSocket {
  const socket = FakeSocket.instances.at(-1);
  if (!socket) throw new Error('no socket opened');
  return socket;
}

function parsed(socket: FakeSocket, index: number): Record<string, unknown> {
  return JSON.parse(socket.sent[index]) as Record<string, unknown>;
}

function subtitle(seq: number, text = `line ${seq}`, final = true) {
  return { t: 'subtitle', id: `s${seq}`, speaker: 'interviewer', seq, en: text, final };
}

beforeEach(() => {
  FakeSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeSocket);
  vi.useFakeTimers();
});

afterEach(() => {
  // Auto-cleanup is off (vitest globals are off) — without this an earlier
  // hook keeps its reconnect timers and sockets alive into the next spec.
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('backoffDelay', () => {
  test('locks the ladder to 1s, 2s, 4s and caps at 30s', () => {
    expect(backoffDelay(1)).toBe(1000);
    expect(backoffDelay(2)).toBe(2000);
    expect(backoffDelay(3)).toBe(4000);
    expect(backoffDelay(4)).toBe(30_000);
    expect(backoffDelay(9)).toBe(30_000);
  });
});

describe('useWs reconnect', () => {
  test('reconnects on the ladder and never opens a second socket before the delay elapses', () => {
    const { result } = renderHook(() => useWs(TICKET));
    expect(FakeSocket.instances).toHaveLength(1);

    act(() => latest().accept());
    expect(result.current.state).toBe('connected');

    // Drop 1 → 1s
    act(() => latest().drop());
    expect(result.current.state).toBe('reconnecting');
    act(() => vi.advanceTimersByTime(999));
    expect(FakeSocket.instances).toHaveLength(1);
    act(() => vi.advanceTimersByTime(1));
    expect(FakeSocket.instances).toHaveLength(2);

    // Drop 2 → 2s
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1999));
    expect(FakeSocket.instances).toHaveLength(2);
    act(() => vi.advanceTimersByTime(1));
    expect(FakeSocket.instances).toHaveLength(3);

    // Drop 3 → 4s
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(4000));
    expect(FakeSocket.instances).toHaveLength(4);

    // Drop 4 and 5 → capped at 30s, not a tight loop
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(29_999));
    expect(FakeSocket.instances).toHaveLength(4);
    act(() => vi.advanceTimersByTime(1));
    expect(FakeSocket.instances).toHaveLength(5);

    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(30_000));
    expect(FakeSocket.instances).toHaveLength(6);
  });

  test('surfaces a stale flag after repeated failed reconnects (UAT-5 stale-token hint)', () => {
    const { result } = renderHook(() => useWs(TICKET));

    act(() => latest().accept());
    expect(result.current.stale).toBe(false);

    // Three failed reconnects (attempt 3 fires on the 4s rung).
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(2000));
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(4000));
    expect(result.current.stale).toBe(true);

    // A healthy open clears it.
    act(() => latest().accept());
    expect(result.current.stale).toBe(false);
  });

  test('a successful reconnect resets the ladder to 1s', () => {
    renderHook(() => useWs(TICKET));

    act(() => latest().accept());
    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    act(() => latest().accept());

    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    expect(FakeSocket.instances).toHaveLength(3);
  });

  test('does not reconnect after unmount and closes the socket', () => {
    const { unmount } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());
    const socket = latest();

    unmount();

    expect(socket.readyState).toBe(3);
    act(() => vi.advanceTimersByTime(120_000));
    expect(FakeSocket.instances).toHaveLength(1);
  });
});

describe('useWs resume', () => {
  test('sends {t:resume,sinceSeq} with the last seen seq on every open', () => {
    renderHook(() => useWs(TICKET));

    act(() => latest().accept());
    expect(parsed(latest(), 0)).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 0 });

    act(() => {
      latest().push(subtitle(4));
      latest().push(subtitle(5));
    });

    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    act(() => latest().accept());

    expect(parsed(latest(), 0)).toEqual({ t: 'resume', sinceSeq: 5, sinceEpoch: 0 });
  });

  test('replay tail re-renders without duplicating already-seen seqs', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => latest().push(subtitle(5, 'seen before')));

    act(() => {
      latest().push({ t: 'timeline', events: [subtitle(5, 'seen before'), subtitle(6, 'new line')] });
    });

    const rendered = result.current.events.filter((event) => event.t === 'subtitle');
    expect(rendered).toHaveLength(2);
    expect(rendered.map((event) => (event.t === 'subtitle' ? event.seq : 0))).toEqual([5, 6]);
  });
});

describe('useWs session restart (CR-01)', () => {
  const MARKER = { t: 'session_started', epoch: 2 };
  const STRATEGY = { t: 'strategy', id: 's-r1', roundId: 'r1', title: '策略', bullets: ['一'] };

  test('drops the cursors and the rendered stream when the session restarts', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => {
      latest().push(subtitle(1, '旧问题'));
      latest().push(subtitle(2, '旧回答'));
      latest().push(STRATEGY);
    });
    expect(result.current.events).toHaveLength(3);

    // 停止 → 开始模拟会话: the new session renumbers from 1 and reuses "s-r1".
    act(() => {
      latest().push(MARKER);
      latest().push(subtitle(1, '新问题'));
      latest().push(STRATEGY);
    });

    const rendered = result.current.events;
    expect(rendered).toHaveLength(2); // the marker itself is not rendered
    expect(rendered.map((event) => (event.t === 'subtitle' ? event.seq : event.t))).toEqual([
      1,
      'strategy',
    ]);
  });

  test('a resume tail carrying the marker re-renders the new session from scratch', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    // The previous session's high-water mark, then the restart arrives as part
    // of the resume reply (the phone was offline across 停止 / 开始).
    act(() => latest().push(subtitle(8, '旧会话最后一行')));
    act(() => {
      latest().push({
        t: 'timeline',
        events: [MARKER, subtitle(1, '新问题'), subtitle(2, '新回答')],
      });
    });

    const seqs = result.current.events
      .filter((event) => event.t === 'subtitle')
      .map((event) => (event.t === 'subtitle' ? event.seq : 0));
    expect(seqs).toEqual([1, 2]);
  });

  test('resume asks from scratch after a restart so a reconnect cannot skip it', () => {
    renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => latest().push(subtitle(5, '旧会话')));
    act(() => latest().push(MARKER));

    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    act(() => latest().accept());

    // Both halves of the recovery travel: the cursor is reset AND the epoch
    // says which session the phone is actually in — the server replays the
    // whole timeline when the epoch no longer matches, even if the seq alone
    // would look fresh (2 is the high-water mark of both sessions).
    expect(parsed(latest(), 0)).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 2 });
  });
});

describe('useWs control', () => {
  test('sendLanguagePref emits {t:control,language} — never language_pref', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => result.current.sendLanguagePref('all-en'));

    const frame = parsed(latest(), 2); // 0 = resume, 1 = the onopen re-assert
    expect(frame).toEqual({ t: 'control', language: 'all-en' });
    expect(Object.keys(frame)).toEqual(['t', 'language']);
    expect(latest().sent.join('')).not.toContain('language_pref');
  });

  test('a tap while the socket is down is queued and re-sent on the next open (WR-03)', () => {
    const { result } = renderHook(() => useWs(TICKET));
    const socket = latest();

    expect(() => {
      act(() => result.current.sendLanguagePref('all-en'));
    }).not.toThrow();
    expect(socket.sent).toHaveLength(0); // nothing to send on yet

    // The reconnect backoff window elapses and the mode is not lost: onopen
    // re-asserts the user's last choice instead of the default.
    act(() => latest().accept());
    expect(parsed(latest(), 1)).toEqual({ t: 'control', language: 'all-en' });
  });

  test('re-asserts the last chosen mode on every reopen (WR-03)', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());
    expect(parsed(latest(), 1)).toEqual({ t: 'control', language: 'bilingual' });

    act(() => result.current.sendLanguagePref('all-zh'));
    expect(parsed(latest(), 2)).toEqual({ t: 'control', language: 'all-zh' });

    act(() => latest().drop());
    act(() => vi.advanceTimersByTime(1000));
    act(() => latest().accept());

    expect(parsed(latest(), 0)).toEqual({ t: 'resume', sinceSeq: 0, sinceEpoch: 0 });
    expect(parsed(latest(), 1)).toEqual({ t: 'control', language: 'all-zh' });
  });
});

describe('useWs frame gate (T-01-02 regression)', () => {
  test('drops non-JSON, unknown shapes, and malformed subtitles', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => {
      latest().pushRaw('not json at all');
      latest().push({ foo: 'bar' });
      latest().push({ t: 'subtitle', id: 'evil', speaker: 'user', en: 'no seq', final: true });
    });

    expect(result.current.events).toHaveLength(0);
  });

  test('drops frames larger than the 64KB cap', () => {
    const { result } = renderHook(() => useWs(TICKET));
    act(() => latest().accept());

    act(() => {
      latest().push(subtitle(1, 'a'.repeat(70 * 1024)));
      latest().push(subtitle(2, 'small enough'));
    });

    const rendered = result.current.events.filter((event) => event.t === 'subtitle');
    expect(rendered).toHaveLength(1);
    expect(rendered[0].t === 'subtitle' && rendered[0].seq).toBe(2);
  });
});
