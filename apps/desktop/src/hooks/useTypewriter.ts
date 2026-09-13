import { useEffect, useState } from 'react';

/**
 * useTypewriter — the desktop port of the phone's typewriter hook
 * (apps/teleprompter/src/hooks/useTypewriter.ts, SYNC-05): both surfaces
 * reveal text at the same cadence so the demo feels like one teleprompter.
 *
 * Contract:
 * - reveals one character per `intervalMs` tick (default 40ms) — exactly
 *   `intervalMs × text.length` ms to full reveal;
 * - with `prefers-reduced-motion: reduce`, the full text renders immediately
 *   with zero timers pending;
 * - the interval is cleared whenever `text`/`intervalMs` change or on unmount
 *   (the interval also self-clears once the text is fully revealed).
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

export function useTypewriter(text: string, intervalMs = 40): string {
  const [visible, setVisible] = useState(0);
  const reduced = usePrefersReducedMotion();

  useEffect(() => {
    if (reduced) {
      setVisible(text.length);
      return;
    }
    setVisible(0);
    const id = window.setInterval(
      () =>
        setVisible((n) =>
          n >= text.length ? (window.clearInterval(id), n) : n + 1,
        ),
      intervalMs,
    );
    return () => window.clearInterval(id);
  }, [text, intervalMs, reduced]);

  return text.slice(0, visible);
}
