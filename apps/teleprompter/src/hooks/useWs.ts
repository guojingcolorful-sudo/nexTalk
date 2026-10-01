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
 * - A session action tapped while the socket is down is queued (last wins)
 *   and flushed on the next open, after the resume/language replay — the
 *   gate the page flips optimistically is never a lie (A).
 * - A liveness watchdog probes a silent socket with the same resume handshake
 *   and rebuilds it when the probe goes unanswered, so a half-open transport
 *   (iOS suspension, NAT timeout, dead Wi-Fi) cannot freeze the teleprompter
 *   on a socket that still looks connected (B).
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

/**
 * Liveness watchdog (B): silence after which the client probes the server.
 * The probe is the resume handshake the client already sends on open — the
 * desktop always answers one (an empty timeline counts), so silence after the
 * probe means the transport, not the desktop, is gone.
 */
export const WATCHDOG_SILENCE_MS = 10_000;

/** Grace for that probe's reply before the socket is closed and rebuilt (B). */
export const WATCHDOG_PROBE_GRACE_MS = 10_000;

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
  /**
   * True once the reconnect ladder has failed repeatedly (attempt ≥ 3) —
   * usually a pairing token the desktop rotated on restart. The page shows
   * the re-scan guidance instead of retrying silently forever.
   */
  stale: boolean;
  /** Push the phone's session-level language mode to the desktop (SYNC-03). */
  sendLanguagePref: (pref: LanguagePref) => void;
  /**
   * Trigger a session lifecycle action on the desktop (SYNC-01 round-trip):
   * 开始提词 is the same function as the desktop's 开始模拟会话. A tap that
   * lands while the socket is down is queued (last wins) and flushed on the
   * next open — the gate the page flips optimistically is never a lie.
   */
  sendSessionAction: (action: 'start_session' | 'stop_session') => void;
}

function send(socket: WebSocket, message: ClientMessage): void {
  socket.send(JSON.stringify(message));
}

export function useWs(ticket: WsTicket | null): WsResult {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [state, setState] = useState<WsConnectionState>(
    ticket?.token ? 'connecting' : 'closed',
  );
  const [stale, setStale] = useState(false);

  const socketRef = useRef<WebSocket | null>(null);
  /** Highest subtitle seq rendered so far — the resume cursor (SYNC-05). */
  const seenSeqRef = useRef(0);
  /** Strategy ids already rendered; replay must not duplicate a card. */
  const seenStrategyIdsRef = useRef<Set<string>>(new Set());
  /** Session identity the renderer is showing (0 = none seen yet, CR-01). */
  const epochRef = useRef(0);
  /** Last mode the user picked; re-asserted on every (re)open (WR-03). */
  const languageRef = useRef<LanguagePref>('bilingual');
  /**
   * Session action tapped while the socket was not open (A). Last one wins:
   * tapping 开始 then 暂停 during an outage must not start a session late.
   */
  const pendingActionRef = useRef<'start_session' | 'stop_session' | null>(null);

  useEffect(() => {
    if (!ticket || !ticket.token) {
      setState('closed');
      return;
    }

    let disposed = false;
    let reconnectTimer: number | undefined;
    let socket: WebSocket | null = null;
    let attempt = 0;

    // B: liveness watchdog. A socket can stay "OPEN" while nothing travels on
    // it (iOS suspension, NAT timeout, dead Wi-Fi): no error, no close, just
    // silence while the teleprompter freezes. Silence is measured by a timer
    // that every inbound frame re-arms, so a probe only goes out when the
    // stream really stopped.
    let watchdogTimer: number | undefined;

    const disarmWatchdog = () => {
      if (watchdogTimer !== undefined) {
        window.clearTimeout(watchdogTimer);
        watchdogTimer = undefined;
      }
    };

    /** The probe was never answered — half-open: let the ladder rebuild it. */
    const rebuildSilentSocket = () => {
      watchdogTimer = undefined;
      if (disposed || !socket || socket.readyState !== WS_OPEN) return;
      // onclose -> scheduleReconnect -> fresh socket, full resume replay.
      socket.close();
    };

    /** T1 of silence elapsed: probe with the resume handshake, then await T2. */
    const probeForLife = () => {
      watchdogTimer = undefined;
      if (disposed || !socket || socket.readyState !== WS_OPEN) return;
      send(socket, {
        t: 'resume',
        sinceSeq: seenSeqRef.current,
        sinceEpoch: epochRef.current,
      });
      watchdogTimer = window.setTimeout(rebuildSilentSocket, WATCHDOG_PROBE_GRACE_MS);
    };

    /** Any inbound frame proves the transport is alive: restart the window. */
    const noteInbound = () => {
      disarmWatchdog();
      watchdogTimer = window.setTimeout(probeForLife, WATCHDOG_SILENCE_MS);
    };

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
        disarmWatchdog();
        attempt += 1;
        setState('reconnecting');
        // UAT-5: three failed rungs usually mean the desktop restarted and
        // rotated the pairing token — surface the re-scan guidance.
        if (attempt >= 3) setStale(true);
        reconnectTimer = window.setTimeout(connect, backoffDelay(attempt));
      };

      ws.onopen = () => {
        if (disposed) return;
        attempt = 0; // a healthy connection resets the ladder
        setState('connected');
        setStale(false);
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
        // A: the same promise for the session gate — a tap swallowed by the
        // outage goes out now, once, after the state the desktop needs to
        // interpret it.
        const pendingAction = pendingActionRef.current;
        if (pendingAction !== null) {
          pendingActionRef.current = null;
          send(ws, { t: 'control', action: pendingAction });
        }
        noteInbound(); // arm the liveness watchdog for this socket
      };

      ws.onmessage = (message) => {
        if (disposed) return;
        // Any frame — even one the gate below drops — proves the transport is
        // alive, so the watchdog starts its silence window over here.
        noteInbound();
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
      disarmWatchdog();
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

  const sendSessionAction = useCallback(
    (action: 'start_session' | 'stop_session') => {
      const socket = socketRef.current;
      if (!socket || socket.readyState !== WS_OPEN) {
        // A: the outage must not swallow the tap — remember the latest one
        // (last wins) and let onopen replay it once the socket is back.
        pendingActionRef.current = action;
        return;
      }
      send(socket, { t: 'control', action });
    },
    [],
  );

  return { events, state, stale, sendLanguagePref, sendSessionAction };
}
