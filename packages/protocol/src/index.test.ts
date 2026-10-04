import { describe, expect, it } from 'vitest';
import { isServerEvent } from './index';

describe('isServerEvent narrowing — valid payloads accepted', () => {
  it('accepts a well-formed subtitle event', () => {
    expect(
      isServerEvent({ t: 'subtitle', id: 's1', speaker: 'interviewer', seq: 1, en: 'hello', final: true }),
    ).toBe(true);
  });

  it('accepts a subtitle with optional zh/en omitted', () => {
    expect(isServerEvent({ t: 'subtitle', id: 's1', speaker: 'user', seq: 2, final: false })).toBe(true);
  });

  it('accepts strategy events with string[] bullets', () => {
    expect(
      isServerEvent({
        t: 'strategy',
        id: 'st1',
        roundId: 'r1',
        title: '数据库优化',
        bullets: ['慢查询日志定位', '拆连表查询'],
      }),
    ).toBe(true);
  });

  it('accepts status events for every session state', () => {
    for (const session of ['idle', 'listening', 'generating', 'ended']) {
      expect(isServerEvent({ t: 'status', session })).toBe(true);
    }
  });

  it('accepts timeline replay events containing nested ServerEvents', () => {
    expect(isServerEvent({ t: 'timeline', events: [{ t: 'status', session: 'listening' }] })).toBe(true);
  });
});

describe('isServerEvent narrowing — client messages rejected', () => {
  it('rejects a control message (client → server only)', () => {
    expect(isServerEvent({ t: 'control', language: 'bilingual' })).toBe(false);
  });

  it('rejects a resume message (client → server only)', () => {
    expect(isServerEvent({ t: 'resume', sinceSeq: 5 })).toBe(false);
  });
});

describe('isServerEvent narrowing — invalid payloads rejected', () => {
  it('rejects non-object values', () => {
    expect(isServerEvent(null)).toBe(false);
    expect(isServerEvent(undefined)).toBe(false);
    expect(isServerEvent('subtitle')).toBe(false);
    expect(isServerEvent(42)).toBe(false);
    expect(isServerEvent([])).toBe(false);
  });

  it('rejects payloads missing the t discriminator', () => {
    expect(isServerEvent({ speaker: 'interviewer', seq: 1, final: true })).toBe(false);
  });

  it('rejects unknown t values', () => {
    expect(isServerEvent({ t: 'teleport', id: 'x' })).toBe(false);
  });

  it('rejects a subtitle with non-numeric seq', () => {
    expect(isServerEvent({ t: 'subtitle', id: 's1', speaker: 'interviewer', seq: '1', final: true })).toBe(false);
  });

  it('rejects a subtitle with seq = NaN (not a finite number)', () => {
    expect(isServerEvent({ t: 'subtitle', id: 's1', speaker: 'interviewer', seq: NaN, final: true })).toBe(false);
  });

  it('rejects a subtitle with speaker outside the union', () => {
    expect(isServerEvent({ t: 'subtitle', id: 's1', speaker: 'recruiter', seq: 1, final: true })).toBe(false);
  });

  it('rejects a subtitle with non-boolean final', () => {
    expect(isServerEvent({ t: 'subtitle', id: 's1', speaker: 'interviewer', seq: 1, final: 'yes' })).toBe(false);
  });

  it('rejects a strategy with non-array bullets', () => {
    expect(isServerEvent({ t: 'strategy', id: 'st1', roundId: 'r1', title: 'x', bullets: 'a,b' })).toBe(false);
  });

  it('rejects a strategy with non-string bullets elements', () => {
    expect(isServerEvent({ t: 'strategy', id: 'st1', roundId: 'r1', title: 'x', bullets: ['a', 1] })).toBe(false);
  });

  it('accepts a strategy carrying the bilingual AI answer (UAT-8)', () => {
    expect(
      isServerEvent({
        t: 'strategy',
        id: 'st1',
        roundId: 'r1',
        title: '数据库优化',
        bullets: ['慢查询日志定位'],
        answerZh: '第一步，通过慢查询日志定位瓶颈。',
        answerEn: 'First, use the slow query log to locate the bottleneck.',
      }),
    ).toBe(true);
  });

  it('rejects a strategy with a non-string answerZh', () => {
    expect(
      isServerEvent({
        t: 'strategy',
        id: 'st1',
        roundId: 'r1',
        title: 'x',
        bullets: ['a'],
        answerZh: 42,
      }),
    ).toBe(false);
  });

  it('rejects a strategy with a non-string answerEn', () => {
    expect(
      isServerEvent({
        t: 'strategy',
        id: 'st1',
        roundId: 'r1',
        title: 'x',
        bullets: ['a'],
        answerEn: true,
      }),
    ).toBe(false);
  });
});

describe('session identity marker (restart signal)', () => {
  it('accepts a session_started event carrying its epoch', () => {
    expect(isServerEvent({ t: 'session_started', epoch: 1 })).toBe(true);
    expect(isServerEvent({ t: 'session_started', epoch: 2 })).toBe(true);
  });

  it('rejects a session_started without a finite numeric epoch', () => {
    expect(isServerEvent({ t: 'session_started' })).toBe(false);
    expect(isServerEvent({ t: 'session_started', epoch: '1' })).toBe(false);
    expect(isServerEvent({ t: 'session_started', epoch: NaN })).toBe(false);
  });

  it('accepts the marker inside a resume timeline', () => {
    expect(
      isServerEvent({ t: 'timeline', events: [{ t: 'session_started', epoch: 3 }] }),
    ).toBe(true);
  });
});

describe('D-07 provenance fields (confidence + trace)', () => {
  const trace = {
    segmentStartMs: 1240,
    termHits: [{ zh: '慢查询', en: 'slow query' }],
    provider: 'xfyun',
    modelVersion: 'iat',
    confidenceSource: 'proxy' as const,
  };
  const subtitle = {
    t: 'subtitle' as const,
    id: 's1',
    speaker: 'interviewer' as const,
    seq: 1,
    en: 'Why is this query slow?',
    final: true,
  };

  it('accepts a subtitle carrying confidence and a full trace', () => {
    expect(isServerEvent({ ...subtitle, confidence: 'low', trace })).toBe(true);
  });

  it('accepts every ConfidenceLevel value', () => {
    for (const confidence of ['high', 'medium', 'low']) {
      expect(isServerEvent({ ...subtitle, confidence })).toBe(true);
    }
  });

  it('accepts a trace with an empty termHits list (Phase 2 sends none)', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, termHits: [] } })).toBe(true);
  });

  it('accepts a trace carrying an aggregatable error code (D-19)', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, errorCode: '45000001' } })).toBe(true);
  });

  it('rejects a confidence outside the union', () => {
    expect(isServerEvent({ ...subtitle, confidence: 'maybe' })).toBe(false);
  });

  it('rejects a trace missing modelVersion', () => {
    const { modelVersion: _omitted, ...incomplete } = trace;
    expect(isServerEvent({ ...subtitle, trace: incomplete })).toBe(false);
  });

  it('rejects a trace with an unknown confidenceSource', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, confidenceSource: 'guessed' } })).toBe(false);
  });

  it('rejects a trace whose termHits are not {zh,en} records', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, termHits: 'slow query' } })).toBe(false);
    expect(isServerEvent({ ...subtitle, trace: { ...trace, termHits: [{ zh: '慢查询' }] } })).toBe(false);
  });

  it('rejects a non-string errorCode', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, errorCode: 45000001 } })).toBe(false);
  });

  it('rejects a non-finite segmentStartMs', () => {
    expect(isServerEvent({ ...subtitle, trace: { ...trace, segmentStartMs: NaN } })).toBe(false);
  });

  it('still accepts a Phase-1 shaped subtitle without confidence or trace', () => {
    expect(isServerEvent(subtitle)).toBe(true);
  });
});

describe('D-08 abstain channel', () => {
  const abstained = {
    t: 'abstained' as const,
    id: 's9',
    speaker: 'user' as const,
    seq: 4,
    segmentStartMs: 2100,
  };

  it('accepts an abstained event for every reason', () => {
    for (const reason of ['silent_audio', 'unrecognized']) {
      expect(isServerEvent({ ...abstained, reason })).toBe(true);
    }
  });

  it('rejects a reason outside the union', () => {
    expect(isServerEvent({ ...abstained, reason: 'bored' })).toBe(false);
  });

  it('rejects an abstained event missing segmentStartMs', () => {
    const { segmentStartMs: _omitted, ...incomplete } = abstained;
    expect(isServerEvent({ ...incomplete, reason: 'silent_audio' })).toBe(false);
  });

  it('rejects a non-finite segmentStartMs', () => {
    expect(isServerEvent({ ...abstained, reason: 'silent_audio', segmentStartMs: Infinity })).toBe(false);
  });

  it('rejects an abstained event with a speaker outside the union', () => {
    expect(isServerEvent({ ...abstained, reason: 'silent_audio', speaker: 'recruiter' })).toBe(false);
  });

  it('accepts an abstained event inside a timeline replay', () => {
    expect(
      isServerEvent({ t: 'timeline', events: [{ ...abstained, reason: 'unrecognized' }] }),
    ).toBe(true);
  });
});

describe('language mode variant (SYNC-03 applied-mode observation)', () => {
  it('accepts every LanguagePref value', () => {
    for (const language of ['all-zh', 'all-en', 'bilingual'] as const) {
      expect(isServerEvent({ t: 'language', language })).toBe(true);
    }
  });

  it('rejects a language event missing the language field', () => {
    expect(isServerEvent({ t: 'language' })).toBe(false);
  });

  it('rejects a language value outside the LanguagePref union', () => {
    expect(isServerEvent({ t: 'language', language: 'french' })).toBe(false);
  });

  it('rejects a timeline containing a non-ServerEvent element', () => {
    expect(isServerEvent({ t: 'timeline', events: [{ t: 'control', language: 'bilingual' }] })).toBe(false);
  });
});
