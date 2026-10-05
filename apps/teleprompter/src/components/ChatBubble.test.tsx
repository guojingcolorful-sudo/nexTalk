import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ChatBubble from './ChatBubble';

afterEach(cleanup);
afterEach(() => {
  vi.unstubAllGlobals();
});

/** Reduced motion commits every line instantly — the typewriter itself is
 *  covered by its own suite; here only the degraded contract matters. */
function stubReducedMotion(): void {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  );
}

const ANSWER_ZH = '首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。';

// GOV-14 / D-12: the phone shows the identical locked degraded form as the
// desktop — same copy, same badge, same "never a fabricated translation" rule.
describe('ChatBubble degraded form', () => {
  it('renders the badge, the locked copy, the original and 正在重试 — never a fabricated English line', () => {
    stubReducedMotion();
    render(
      <ChatBubble
        speaker="user"
        zh={ANSWER_ZH}
        en="A translation the pipeline never produced"
        degraded={{ errorCode: 'retry_exhausted' }}
      />,
    );

    const badge = screen.getByText('翻译失败');
    expect(badge.className).toContain('bg-red-500');
    expect(badge.getAttribute('data-error-code')).toBe('retry_exhausted');
    expect(screen.getByText('翻译服务暂时不可用')).toBeTruthy();
    expect(screen.getByText(ANSWER_ZH)).toBeTruthy();
    expect(screen.getByText('正在重试')).toBeTruthy();
    expect(screen.queryByText('A translation the pipeline never produced')).toBeNull();
  });

  it('shows the original even in EN mode (the original is the only truth)', () => {
    stubReducedMotion();
    render(
      <ChatBubble
        speaker="user"
        zh={ANSWER_ZH}
        language="all-en"
        degraded={{ errorCode: 'circuit_open' }}
      />,
    );

    expect(screen.getByText(ANSWER_ZH)).toBeTruthy();
    expect(screen.getByText('翻译服务暂时不可用')).toBeTruthy();
  });
});

// D-03: the phone shows the identical 「待翻译」 state as the desktop for an
// abstained segment — visible, never an empty card and never a fabricated
// line.
describe('ChatBubble abstained form', () => {
  it('renders the 待翻译 marker instead of any text', () => {
    stubReducedMotion();
    render(<ChatBubble speaker="user" abstained />);

    expect(screen.getByText('待翻译')).toBeTruthy();
  });

  it('keeps the marker even when text arrived with it — the state is not a message', () => {
    stubReducedMotion();
    render(<ChatBubble speaker="user" zh={ANSWER_ZH} en="An English line" abstained />);

    expect(screen.getByText('待翻译')).toBeTruthy();
    expect(screen.queryByText(ANSWER_ZH)).toBeNull();
    expect(screen.queryByText('An English line')).toBeNull();
  });
});
