import { useEffect, useState } from 'react';
import { isServerEvent, type ServerEvent } from '@nextalk/protocol';

/**
 * useWs — H5 WebSocket client (01-02 walking skeleton, SYNC-02).
 *
 * - Browser-native WebSocket to `{url}/ws?token={token}`; url defaults to the
 *   page host on port 8787 (the LAN server in the desktop app).
 * - Every inbound payload passes `isServerEvent` BEFORE it can reach a
 *   renderer (threat T-01-02) — malformed or unknown shapes are dropped.
 * - `timeline` events flatten their nested events into the stream.
 * - NO reconnect logic yet — reconnect + resume land in 01-04.
 */

export type WsConnectionState = 'connecting' | 'open' | 'closed';

export interface WsTicket {
  /** 128-bit pairing token from the QR URL (T-01-01). */
  token: string;
  /** WS server base URL; defaults to `ws://{page host}:8787`. */
  url?: string;
}

export interface WsResult {
  /** Events that passed the isServerEvent gate, in arrival order. */
  events: ServerEvent[];
  state: WsConnectionState;
}

export function useWs(ticket: WsTicket | null): WsResult {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [state, setState] = useState<WsConnectionState>('connecting');

  useEffect(() => {
    if (!ticket || !ticket.token) {
      setState('closed');
      return;
    }

    const base = ticket.url ?? `ws://${window.location.hostname}:8787`;
    const socket = new WebSocket(
      `${base}/ws?token=${encodeURIComponent(ticket.token)}`,
    );
    let alive = true;

    socket.onopen = () => {
      if (alive) setState('open');
    };

    socket.onmessage = (message) => {
      let payload: unknown;
      try {
        payload = JSON.parse(String(message.data));
      } catch {
        return; // not JSON — nothing to render
      }
      // T-01-02: never render unvalidated payloads.
      if (!isServerEvent(payload)) return;
      const eventsToAppend = payload.t === 'timeline' ? payload.events : [payload];
      setEvents((prev) => [...prev, ...eventsToAppend]);
    };

    socket.onclose = () => {
      if (alive) setState('closed');
    };
    socket.onerror = () => {
      if (alive) setState('closed');
    };

    return () => {
      alive = false;
      socket.close();
    };
  }, [ticket?.token, ticket?.url]);

  return { events, state };
}
