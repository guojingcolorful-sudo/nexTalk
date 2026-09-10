import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isServerEvent, type ServerEvent } from '@nextalk/protocol';

/** Session state machine mirrored from the Rust `session_status` event. */
export type SessionStatus = 'idle' | 'listening' | 'generating' | 'ended';

const SESSION_STATES: readonly SessionStatus[] = ['idle', 'listening', 'generating', 'ended'];

export interface TauriEventsState {
  /** Narrowed ServerEvents in arrival order (timeline batches flattened). */
  events: ServerEvent[];
  /** Latest session status from `session_status` (or a `status` event). */
  status: SessionStatus;
  /** Connected phone count from `phone_count`, or null before the first
   *  emission (the desktop telemetry lands in 01-05 — until then the QR card
   *  shows 等待扫码 rather than a fake zero). */
  phoneCount: number | null;
}

/** Payload narrowing for the `session` event (threat T-01-02): anything that
 *  is not a well-formed ServerEvent is dropped before it reaches React state. */
function narrowSession(payload: unknown): ServerEvent[] {
  if (!isServerEvent(payload)) return [];
  return payload.t === 'timeline' ? payload.events : [payload];
}

function narrowStatus(payload: unknown): SessionStatus | null {
  if (typeof payload !== 'object' || payload === null) return null;
  const value = (payload as { session?: unknown }).session;
  return typeof value === 'string' && SESSION_STATES.includes(value as SessionStatus)
    ? (value as SessionStatus)
    : null;
}

function narrowPhoneCount(payload: unknown): number | null {
  if (typeof payload !== 'object' || payload === null) return null;
  const value = (payload as { count?: unknown }).count;
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

/**
 * Subscribes the desktop webviews to the Rust event bus.
 *
 * Rust is the single source of truth for cross-window state (01-02): the
 * console and dual windows both read the same `session` / `session_status`
 * stream and never talk to each other directly.
 *
 * Every `session` payload passes through isServerEvent before it can update
 * state — a malformed or hostile payload (the WS client path can reach the
 * same bus) never renders (T-01-02).
 *
 * Outside a Tauri webview (plain-browser preview, jsdom unit tests) `listen`
 * rejects because there is no IPC bridge; the hook then simply never receives
 * events and the UI keeps its empty/initial state. That is the documented
 * degraded mode, not an error path.
 */
export function useTauriEvents(): TauriEventsState {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [status, setStatus] = useState<SessionStatus>('idle');
  const [phoneCount, setPhoneCount] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    const unlisteners: UnlistenFn[] = [];

    const track = (pending: Promise<UnlistenFn>): void => {
      pending
        .then((unlisten) => {
          if (alive) unlisteners.push(unlisten);
          else void unlisten();
        })
        .catch(() => {
          // No IPC bridge (browser preview / jsdom) — stay in the empty state.
        });
    };

    track(
      listen<unknown>('session', (event) => {
        const batch = narrowSession(event.payload);
        if (batch.length === 0) return;
        setEvents((previous) => [...previous, ...batch]);
        for (const item of batch) {
          if (item.t === 'status') setStatus(item.session);
        }
      }),
    );

    track(
      listen<unknown>('session_status', (event) => {
        const next = narrowStatus(event.payload);
        if (next !== null) setStatus(next);
      }),
    );

    track(
      listen<unknown>('phone_count', (event) => {
        const next = narrowPhoneCount(event.payload);
        if (next !== null) setPhoneCount(next);
      }),
    );

    return () => {
      alive = false;
      for (const unlisten of unlisteners) void unlisten();
    };
  }, []);

  return { events, status, phoneCount };
}
