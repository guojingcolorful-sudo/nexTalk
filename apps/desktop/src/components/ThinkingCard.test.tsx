import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ThinkingCard from './ThinkingCard';

afterEach(cleanup);

describe('ThinkingCard (UAT-14)', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('reveals the thinking lines one by one (~1.2s apart)', () => {
    vi.useFakeTimers();
    render(<ThinkingCard />);

    expect(screen.getByText('AI 思考中')).toBeTruthy();
    expect(screen.getByText('正在分析问题要点')).toBeTruthy();
    expect(screen.queryByText('正在匹配你的简历与经验')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1200);
    });
    expect(screen.getByText('正在匹配你的简历与经验')).toBeTruthy();
    expect(screen.queryByText('正在组织回答结构')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(2400);
    });
    expect(screen.getByText('正在组织回答结构')).toBeTruthy();
  });

  it('shows every line immediately under prefers-reduced-motion', () => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn().mockReturnValue({
        matches: true,
        media: '(prefers-reduced-motion: reduce)',
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    );

    render(<ThinkingCard />);

    expect(screen.getByText('正在分析问题要点')).toBeTruthy();
    expect(screen.getByText('正在匹配你的简历与经验')).toBeTruthy();
    expect(screen.getByText('正在组织回答结构')).toBeTruthy();
  });
});
