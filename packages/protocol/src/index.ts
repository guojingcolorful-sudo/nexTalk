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

/** STT confidence for one segment (D-07). */
export type ConfidenceLevel = 'high' | 'medium' | 'low';

/**
 * Who produced a segment's `confidence`. A vendor value and a local proxy
 * estimate are never interchangeable: 讯飞 iat returns no confidence at all
 * (research correction 3), so a proxy value posing as a vendor one would
 * silently misreport the STT's real quality (threat T-02-09).
 */
export type ConfidenceSource = 'vendor' | 'proxy';

/** Why the pipeline left a segment untranslated (D-08 abstain channel). */
export type AbstainReason = 'silent_audio' | 'unrecognized';

/**
 * One glossary term matched inside a segment. Defined now so the wire shape is
 * fixed before anything fills it: Phase 2 always sends an empty list, Phase 4
 * populates it from the term base.
 */
export type TermHit = {
  zh: string;
  en: string;
};

/** Per-segment provenance (D-07): what produced this subtitle, and how sure. */
export type SubtitleTrace = {
  /** Milliseconds from the segment's capture start to its first audio byte. */
  segmentStartMs: number;
  termHits: TermHit[];
  provider: string;
  modelVersion: string;
  confidenceSource: ConfidenceSource;
  /** Aggregatable vendor error code (D-19), when the segment carries one. */
  errorCode?: string;
};

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
      /** Absent when the vendor returned none — never a fabricated value. */
      confidence?: ConfidenceLevel;
      /** Absent on Phase-1 shaped events, which this union still accepts. */
      trace?: SubtitleTrace;
    }
  | {
      /**
       * The pipeline refused to translate this segment (D-08): silence, or
       * speech nothing recognisable came out of. The phone renders the reason
       * instead of an empty subtitle card.
       */
      t: 'abstained';
      id: string;
      speaker: Speaker;
      seq: number;
      reason: AbstainReason;
      segmentStartMs: number;
    }
  | {
      t: 'strategy';
      id: string;
      roundId: string;
      title: string;
      bullets: string[];
      /**
       * The AI's bilingual suggested answer (UAT-8): a complete 中/EN answer
       * to the interviewer's question. Absent while the copilot has not
       * produced one yet.
       */
      answerZh?: string;
      answerEn?: string;
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
const CONFIDENCE_LEVELS: readonly string[] = ['high', 'medium', 'low'];
const CONFIDENCE_SOURCES: readonly string[] = ['vendor', 'proxy'];
const ABSTAIN_REASONS: readonly string[] = ['silent_audio', 'unrecognized'];

function isRecord(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}

function isString(x: unknown): x is string {
  return typeof x === 'string';
}

function isFiniteNumber(x: unknown): x is number {
  return typeof x === 'number' && Number.isFinite(x);
}

function isTermHit(x: unknown): x is TermHit {
  return isRecord(x) && isString(x.zh) && isString(x.en);
}

function isSubtitleTrace(x: unknown): x is SubtitleTrace {
  if (!isRecord(x)) return false;
  return (
    isFiniteNumber(x.segmentStartMs) &&
    Array.isArray(x.termHits) &&
    x.termHits.every(isTermHit) &&
    isString(x.provider) &&
    isString(x.modelVersion) &&
    isString(x.confidenceSource) &&
    CONFIDENCE_SOURCES.includes(x.confidenceSource) &&
    (x.errorCode === undefined || isString(x.errorCode))
  );
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
        isFiniteNumber(x.seq) &&
        typeof x.final === 'boolean' &&
        (x.zh === undefined || isString(x.zh)) &&
        (x.en === undefined || isString(x.en)) &&
        (x.confidence === undefined ||
          (isString(x.confidence) && CONFIDENCE_LEVELS.includes(x.confidence))) &&
        (x.trace === undefined || isSubtitleTrace(x.trace))
      );
    case 'abstained':
      return (
        isString(x.id) &&
        isString(x.speaker) &&
        SPEAKERS.includes(x.speaker) &&
        isFiniteNumber(x.seq) &&
        isString(x.reason) &&
        ABSTAIN_REASONS.includes(x.reason) &&
        isFiniteNumber(x.segmentStartMs)
      );
    case 'strategy':
      return (
        isString(x.id) &&
        isString(x.roundId) &&
        isString(x.title) &&
        Array.isArray(x.bullets) &&
        x.bullets.every(isString) &&
        (x.answerZh === undefined || isString(x.answerZh)) &&
        (x.answerEn === undefined || isString(x.answerEn))
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
