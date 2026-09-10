import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useTauriEvents } from './useTauriEvents';

type SessionHandler = (event: { event: string; id: number; payload: unknown }) => void;

/** Listener registry shared with the mocked Tauri event module. */
const { handlers } = vi.hoisted(() => ({
  handlers: new Map<string, SessionHandler[]>(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, handler: SessionHandler) => {
    handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    return () => undefined;
  },
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

describe('useTauriEvents', () => {
  it('drops a malformed session payload before it reaches state', async () => {
    const { result } = renderHook(() => useTauriEvents());
    await waitForListeners('session');

    // Subtitle-shaped but missing `seq` — the isServerEvent gate must reject it
    // (threat T-01-02: the WS client path shares this bus).
    act(() => {
      emit('session', { t: 'subtitle', id: 'evil', speaker: 'user', zh: '恶意载荷', final: true });
    });

    expect(result.current.events).toHaveLength(0);
  });

  it('accepts valid events and flattens timeline batches', async () => {
    const { result } = renderHook(() => useTauriEvents());
    await waitForListeners('session');

    act(() => {
      emit('session', {
        t: 'subtitle',
        id: 'q1',
        speaker: 'interviewer',
        seq: 1,
        zh: '请简单介绍一下你自己。',
        en: 'Please introduce yourself.',
        final: true,
      });
    });
    act(() => {
      emit('session', {
        t: 'timeline',
        events: [
          { t: 'status', session: 'generating' },
          { t: 'strategy', id: 's1', roundId: 'r1', title: '开场结构', bullets: ['先给结论'] },
        ],
      });
    });

    expect(result.current.events).toHaveLength(3);
    // A `status` event carries the same information as `session_status`.
    expect(result.current.status).toBe('generating');
  });

  it('tracks session_status and phone_count with payload validation', async () => {
    const { result } = renderHook(() => useTauriEvents());
    await waitForListeners('session_status');
    await waitForListeners('phone_count');

    expect(result.current.status).toBe('idle');
    expect(result.current.phoneCount).toBeNull();

    act(() => {
      emit('session_status', { session: 'listening' });
      emit('phone_count', { count: 2 });
    });
    expect(result.current.status).toBe('listening');
    expect(result.current.phoneCount).toBe(2);

    // Malformed payloads leave the last good value untouched.
    act(() => {
      emit('session_status', { session: 'sleeping' });
      emit('phone_count', { count: 'two' });
    });
    expect(result.current.status).toBe('listening');
    expect(result.current.phoneCount).toBe(2);
  });
});
