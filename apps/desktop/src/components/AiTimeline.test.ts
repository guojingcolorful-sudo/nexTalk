import { describe, expect, it } from 'vitest';
import type { ServerEvent } from '@nextalk/protocol';
import { isAiThinking } from './AiTimeline';

const QUESTION_R1: ServerEvent = {
  t: 'subtitle',
  id: 'r1-q',
  speaker: 'interviewer',
  seq: 1,
  zh: '问题',
  en: 'Question',
  final: true,
};

const ANSWER_R1: ServerEvent = {
  t: 'subtitle',
  id: 'r1-a',
  speaker: 'user',
  seq: 2,
  zh: '回答',
  final: true,
};

const STRATEGY_R1: ServerEvent = {
  t: 'strategy',
  id: 's-r1',
  roundId: 'r1',
  title: '策略',
  bullets: [],
};

const QUESTION_R2: ServerEvent = {
  t: 'subtitle',
  id: 'r2-q',
  speaker: 'interviewer',
  seq: 3,
  zh: '新问题',
  en: 'New question',
  final: true,
};

describe('isAiThinking (UAT-12)', () => {
  it('is false with no interviewer question yet', () => {
    expect(isAiThinking([])).toBe(false);
    expect(isAiThinking([ANSWER_R1])).toBe(false);
  });

  it('is true while the newest question has no strategy card', () => {
    expect(isAiThinking([QUESTION_R1])).toBe(true);
  });

  it('stays true while the user answers, until the strategy for that round lands', () => {
    expect(isAiThinking([QUESTION_R1, ANSWER_R1])).toBe(true);
  });

  it('turns false once the strategy for the current round arrives', () => {
    expect(isAiThinking([QUESTION_R1, STRATEGY_R1])).toBe(false);
  });

  it('turns true again when the next question opens a new round', () => {
    expect(isAiThinking([QUESTION_R1, STRATEGY_R1, QUESTION_R2])).toBe(true);
  });
});
