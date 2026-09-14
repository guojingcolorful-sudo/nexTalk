import { useEffect, useState } from 'react';

/**
 * useRevealCount — the desktop port of the phone's stagger hook: reveals
 * `count` items one by one so an AI card appears as a thinking process
 * (UAT-11). Each item matures `stepMs` after the previous one; under
 * prefers-reduced-motion everything is visible immediately.
 */
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(() => {
    const mq =
      typeof window !== 'undefined'
        ? window.matchMedia?.('(prefers-reduced-motion: reduce)')
        : undefined;
    return mq?.matches ?? false;
  });

  useEffect(() => {
    const mq =
      typeof window !== 'undefined'
        ? window.matchMedia?.('(prefers-reduced-motion: reduce)')
        : undefined;
    if (!mq) return;
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  return reduced;
}

export function useRevealCount(count: number, stepMs = 400): number {
  // The first item is present immediately; the rest mature one per tick.
  const [revealed, setRevealed] = useState(() => (count === 0 ? 0 : 1));
  const reduced = usePrefersReducedMotion();

  useEffect(() => {
    if (reduced || count === 0) {
      setRevealed(count);
      return;
    }
    setRevealed(1); // the first item is visible from the first frame
    const id = window.setInterval(() => {
      setRevealed((n) => {
        if (n >= count) {
          window.clearInterval(id);
          return n;
        }
        return n + 1;
      });
    }, stepMs);
    return () => window.clearInterval(id);
  }, [count, stepMs, reduced]);

  return revealed;
}
