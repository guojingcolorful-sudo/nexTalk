/**
 * @nextalk/protocol — shared WebSocket message contract (desktop ↔ H5).
 *
 * One event model, two transports: the Rust core emits ServerEvent through
 * Tauri events (desktop webviews) AND WS broadcast (H5). ClientMessage flows
 * from the renderers back to the Rust server.
 *
 * The field names here are the cross-plan contract (01-01/01-02/01-04/01-05):
 * the Rust server parses with serde deny_unknown_fields and drops connections
 * on parse failure — no plan may rename them.
 *
 * Security: every inbound WS payload MUST pass through isServerEvent before
 * reaching a renderer (threat T-01-02).
 */

export type LanguagePref = 'all-zh' | 'all-en' | 'bilingual';

export type Speaker = 'interviewer' | 'user';

export type ServerEvent =
  | {
      /**
       * Session identity (01-05 restart fix): published as the FIRST event of
       * every started session and carried in resume replies. Clients drop their
       * dedupe cursors and rendered events when the epoch changes — a restarted
       * session renumbers its subtitles from 1 and reuses strategy ids, so a
       * stale cursor would otherwise swallow it whole.
       */
      t: 'session_started';
      epoch: number;
    }
  | {
      t: 'subtitle';
      id: string;
      speaker: Speaker;
      seq: number;
      zh?: string;
      en?: string;
      final: boolean;
    }
  | {
      t: 'strategy';
      id: string;
      roundId: string;
      title: string;
      bullets: string[];
    }
  | {
      t: 'status';
      session: 'idle' | 'listening' | 'generating' | 'ended';
    }
  | {
      t: 'language';
      language: LanguagePref;
    }
  | {
      t: 'timeline';
      events: ServerEvent[];
    };

export type ClientMessage =
  | {
      t: 'control';
      /**
       * Session language mode the phone owns (SYNC-03). Omitted on
       * action-only frames.
       */
      language?: LanguagePref;
      /**
       * Session lifecycle action the phone may trigger (SYNC-01 round-trip):
       * 开始提词 starts the simulated session on the desktop, 暂停提词 stops
       * it. Absent on mode-only frames.
       */
      action?: 'start_session' | 'stop_session';
    }
  | {
      t: 'resume';
      sinceSeq: number;
      /**
       * Session epoch the phone believes it is in (0 before it has seen a
       * marker). The server replays the whole timeline when it no longer
       * matches, so a phone that was offline across a restart recovers even
       * when its subtitle cursor cannot be told apart from the new session's.
       * Optional: a client that omits it falls back to the seq cursor alone.
       */
      sinceEpoch?: number;
    };

const LANGUAGE_PREFS: readonly string[] = ['all-zh', 'all-en', 'bilingual'];
const SPEAKERS: readonly string[] = ['interviewer', 'user'];
const SESSION_STATES: readonly string[] = ['idle', 'listening', 'generating', 'ended'];

function isRecord(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}

function isString(x: unknown): x is string {
  return typeof x === 'string';
}

/**
 * Discriminated-union narrowing guard for ServerEvent.
 * Validates field types per variant so invalid/malicious WS payloads
 * never reach renderers.
 */
export function isServerEvent(x: unknown): x is ServerEvent {
  if (!isRecord(x) || !isString(x.t)) return false;

  switch (x.t) {
    case 'session_started':
      return typeof x.epoch === 'number' && Number.isFinite(x.epoch);
    case 'subtitle':
      return (
        isString(x.id) &&
        isString(x.speaker) &&
        SPEAKERS.includes(x.speaker) &&
        typeof x.seq === 'number' &&
        Number.isFinite(x.seq) &&
        typeof x.final === 'boolean' &&
        (x.zh === undefined || isString(x.zh)) &&
        (x.en === undefined || isString(x.en))
      );
    case 'strategy':
      return (
        isString(x.id) &&
        isString(x.roundId) &&
        isString(x.title) &&
        Array.isArray(x.bullets) &&
        x.bullets.every(isString)
      );
    case 'status':
      return isString(x.session) && SESSION_STATES.includes(x.session);
    case 'language':
      return isString(x.language) && LANGUAGE_PREFS.includes(x.language);
    case 'timeline':
      return Array.isArray(x.events) && x.events.every((e) => isServerEvent(e));
    default:
      return false;
  }
}
