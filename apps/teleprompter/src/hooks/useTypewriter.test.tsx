import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { useTypewriter } from './useTypewriter';

/**
 * SYNC-05 unit gate (01-02 Task 3):
 * - deterministic cadence: exactly intervalMs × chars to full reveal
 * - reduced-motion: full text renders immediately, zero timers pending
 */
describe('useTypewriter', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  test('reveals exactly text.length characters after 40 × text.length ms', () => {
    const text = 'hello nex'; // 9 chars
    const { result } = renderHook(() => useTypewriter(text, 40));

    expect(result.current).toBe('');

    act(() => {
      vi.advanceTimersByTime(40 * 4);
    });
    expect(result.current).toBe('hell');

    act(() => {
      vi.advanceTimersByTime(40 * 5);
    });
    expect(result.current).toBe(text);
  });

  test('reveals nothing before the first interval tick', () => {
    const { result } = renderHook(() => useTypewriter('abc', 40));
    act(() => {
      vi.advanceTimersByTime(39);
    });
    expect(result.current).toBe('');
  });

  test('with prefers-reduced-motion matched, full text renders immediately with 0 timers pending', () => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn().mockReturnValue({
        matches: true,
        media: '(prefers-reduced-motion: reduce)',
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    );

    const text = 'immediate render';
    const { result } = renderHook(() => useTypewriter(text, 40));

    expect(result.current).toBe(text);
    expect(vi.getTimerCount()).toBe(0);
  });
});
