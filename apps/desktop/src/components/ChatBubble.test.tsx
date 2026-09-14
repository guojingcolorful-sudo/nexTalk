import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ChatBubble from './ChatBubble';

// Testing Library only self-registers its afterEach cleanup when the runner
// injects globals (vitest globals are off here) — without this, a previous
// render leaks into the next query.
afterEach(cleanup);

// UAT-10: the primary line types out at 40ms/char — run the interval to
// completion so the full text is present for the assertions below.
beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const ANSWER_ZH = '首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。';
const QUESTION_ZH = '你能详细说一下你优化数据库的具体步骤吗？';
const QUESTION_EN =
  'Could you walk me through the specific steps you took to optimize the database?';

/** Reveals every typing line currently rendered (longest line = QUESTION_EN). */
function revealAll() {
  act(() => {
    vi.advanceTimersByTime(20 * QUESTION_EN.length);
  });
}

describe('ChatBubble', () => {
  it('opens an interviewer bubble bilingual, showing the English line and the Chinese subline', () => {
    render(<ChatBubble speaker="interviewer" zh={QUESTION_ZH} en={QUESTION_EN} />);
    revealAll();

    expect(screen.getByText(QUESTION_EN)).toBeTruthy();
    expect(screen.getByText(QUESTION_ZH)).toBeTruthy();
    expect(
      within(screen.getByRole('group', { name: '面试官语言' })).getByRole('button', {
        name: 'EN+中',
        pressed: true,
      }),
    ).toBeTruthy();
  });

  it('opens a user bubble in Chinese only (the user speaks Chinese)', () => {
    render(<ChatBubble speaker="user" zh={ANSWER_ZH} />);
    revealAll();

    expect(screen.getByText(ANSWER_ZH)).toBeTruthy();
    expect(
      within(screen.getByRole('group', { name: '用户语言' })).getByRole('button', {
        name: '中',
        pressed: true,
      }),
    ).toBeTruthy();
  });

  it('keeps each bubble language independent of its neighbors', () => {
    render(
      <>
        <ChatBubble speaker="interviewer" zh={QUESTION_ZH} en={QUESTION_EN} />
        <ChatBubble speaker="user" zh={ANSWER_ZH} />
      </>,
    );
    revealAll();

    fireEvent.click(
      within(screen.getByRole('group', { name: '面试官语言' })).getByRole('button', { name: '中' }),
    );
    revealAll();

    // The interviewer bubble switched to Chinese only …
    expect(screen.queryByText(QUESTION_EN)).toBeNull();
    expect(screen.getByText(QUESTION_ZH)).toBeTruthy();
    // … while the user bubble kept both its preference and its text.
    expect(screen.getByText(ANSWER_ZH)).toBeTruthy();
    expect(
      within(screen.getByRole('group', { name: '用户语言' })).getByRole('button', {
        name: '中',
        pressed: true,
      }),
    ).toBeTruthy();
  });

  it('falls back to the Chinese line when a user bubble has no English yet', () => {
    render(<ChatBubble speaker="user" zh={ANSWER_ZH} />);
    revealAll();

    fireEvent.click(
      within(screen.getByRole('group', { name: '用户语言' })).getByRole('button', { name: 'EN' }),
    );
    revealAll();

    expect(screen.getByText(ANSWER_ZH)).toBeTruthy();
  });

  it('renders no bubble for a payload that carries no text at all', () => {
    const { container } = render(<ChatBubble speaker="interviewer" />);

    expect(container.firstChild).toBeNull();
  });
});
