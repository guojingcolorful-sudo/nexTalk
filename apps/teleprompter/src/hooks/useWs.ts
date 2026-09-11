import { useCallback, useEffect, useRef, useState } from 'react';
import {
  isServerEvent,
  type ClientMessage,
  type LanguagePref,
  type ServerEvent,
} from '@nextalk/protocol';

/**
 * useWs — H5 WebSocket client (01-02 walking skeleton, hardened in 01-04).
 *
 * - Browser-native WebSocket to `{url}/ws?token={token}`; url defaults to the
 *   page host on port 8787 (the LAN server in the desktop app).
 * - Every inbound payload passes `isServerEvent` BEFORE it can reach a
 *   renderer (threat T-01-02) — malformed or unknown shapes are dropped, and
 *   nothing larger than the server's 64KB frame cap is even parsed.
 * - `timeline` events flatten their nested events into the stream.
 * - Drops reconnect on the locked ladder 1s → 2s → 4s, capped at 30s so a
 *   phone with no wifi never turns into a reconnect storm (T-01-10).
 * - On every (re)open the client sends `{ t: 'resume', sinceSeq }` so the
 *   desktop replays the timeline tail — the phone picks the interview back up
 *   where it stopped instead of coming back blank (SYNC-05).
 */

export type WsConnectionState = 'connecting' | 'connected' | 'reconnecting' | 'closed';

/** Mirrors the server's cap (01-02): oversize frames are never parsed. */
export const MAX_FRAME_BYTES = 64 * 1024;

/** ws.OPEN — compared numerically so test doubles do not need the constant. */
const WS_OPEN = 1;

/** Locked reconnect ladder (research Pattern 3): 1s, 2s, 4s, then 30s forever. */
const BACKOFF_LADDER_MS = [1000, 2000, 4000, 30_000] as const;

/** Jitterless backoff — deterministic so the e2e clock can drive it. */
export function backoffDelay(attempt: number): number {
  const index = Math.min(Math.max(attempt, 1), BACKOFF_LADDER_MS.length) - 1;
  return BACKOFF_LADDER_MS[index];
}

export interface WsTicket {
  /** 128-bit pairing token from the QR URL (T-01-01). */
  token: string;
  /** WS server base URL; defaults to `ws://{page host}:8787`. */
  url?: string;
}

export interface WsResult {
  /** Events that passed the isServerEvent gate, in arrival order, deduped. */
  events: ServerEvent[];
  state: WsConnectionState;
  /** Push the phone's session-level language mode to the desktop (SYNC-03). */
  sendLanguagePref: (pref: LanguagePref) => void;
}

function send(socket: WebSocket, message: ClientMessage): void {
  socket.send(JSON.stringify(message));
}

export function useWs(ticket: WsTicket | null): WsResult {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [state, setState] = useState<WsConnectionState>(
    ticket?.token ? 'connecting' : 'closed',
  );

  const socketRef = useRef<WebSocket | null>(null);
  /** Highest subtitle seq rendered so far — the resume cursor (SYNC-05). */
  const seenSeqRef = useRef(0);
  /** Strategy ids already rendered; replay must not duplicate a card. */
  const seenStrategyIdsRef = useRef<Set<string>>(new Set());
  /** Session identity the renderer is showing (0 = none seen yet, CR-01). */
  const epochRef = useRef(0);
  /** Last mode the user picked; re-asserted on every (re)open (WR-03). */
  const languageRef = useRef<LanguagePref>('bilingual');

  useEffect(() => {
    if (!ticket || !ticket.token) {
      setState('closed');
      return;
    }

    let disposed = false;
    let reconnectTimer: number | undefined;
    let socket: WebSocket | null = null;
    let attempt = 0;

    const accept = (incoming: ServerEvent[]) => {
      const fresh: ServerEvent[] = [];
      let restarted = false;
      for (const event of incoming) {
        if (event.t === 'session_started') {
          // CR-01: a new session renumbers its subtitles from 1 and reuses
          // strategy ids, so both cursors are meaningless — drop them and
          // everything rendered (WR-02's copy promises exactly this).
          seenSeqRef.current = 0;
          seenStrategyIdsRef.current.clear();
          epochRef.current = event.epoch;
          restarted = true;
          fresh.length = 0;
          continue;
        }
        if (event.t === 'subtitle') {
          if (event.seq <= seenSeqRef.current) continue; // replay tail overlap
          seenSeqRef.current = event.seq;
          fresh.push(event);
        } else if (event.t === 'strategy') {
          if (seenStrategyIdsRef.current.has(event.id)) continue;
          seenStrategyIdsRef.current.add(event.id);
          fresh.push(event);
        } else {
          fresh.push(event);
        }
      }
      if (restarted) setEvents(fresh); // replace, never stack sessions
      else if (fresh.length > 0) setEvents((prev) => [...prev, ...fresh]);
    };

    const connect = () => {
      const base = ticket.url ?? `ws://${window.location.hostname}:8787`;
      const ws = new WebSocket(`${base}/ws?token=${encodeURIComponent(ticket.token)}`);
      socket = ws;
      socketRef.current = ws;
      let dropped = false;

      const scheduleReconnect = () => {
        if (disposed || dropped) return;
        dropped = true;
        attempt += 1;
        setState('reconnecting');
        reconnectTimer = window.setTimeout(connect, backoffDelay(attempt));
      };

      ws.onopen = () => {
        if (disposed) return;
        attempt = 0; // a healthy connection resets the ladder
        setState('connected');
        // CR-01: the cursor alone cannot tell two sessions apart (both number
        // their lines from 1), so the epoch travels with it — the server
        // replays the whole timeline when it no longer matches.
        send(ws, {
          t: 'resume',
          sinceSeq: seenSeqRef.current,
          sinceEpoch: epochRef.current,
        });
        // WR-03: a tap that landed while the socket was down is otherwise
        // lost for the rest of the session — re-assert the mode on every open.
        send(ws, { t: 'control', language: languageRef.current });
      };

      ws.onmessage = (message) => {
        if (disposed) return;
        const data = typeof message.data === 'string' ? message.data : '';
        if (data.length === 0 || data.length > MAX_FRAME_BYTES) return;
        let payload: unknown;
        try {
          payload = JSON.parse(data);
        } catch {
          return; // not JSON — nothing to render
        }
        // T-01-02: never render unvalidated payloads.
        if (!isServerEvent(payload)) return;
        accept(payload.t === 'timeline' ? payload.events : [payload]);
      };

      ws.onclose = scheduleReconnect;
      ws.onerror = scheduleReconnect;
    };

    connect();

    return () => {
      disposed = true;
      if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer);
      if (socket) {
        socket.onclose = null;
        socket.onerror = null;
        socket.close();
      }
      socketRef.current = null;
    };
  }, [ticket?.token, ticket?.url]);

  const sendLanguagePref = useCallback((pref: LanguagePref) => {
    languageRef.current = pref;
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WS_OPEN) return; // re-sent by onopen
    send(socket, { t: 'control', language: pref });
  }, []);

  return { events, state, sendLanguagePref };
}
