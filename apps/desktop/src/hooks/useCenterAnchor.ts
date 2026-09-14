import { useEffect, type RefObject } from 'react';

/**
 * useCenterAnchor — keeps an element centered in its scroll container while
 * `active` holds (UAT-15). A plain scrollIntoView only centers ONCE at mount;
 * the anchored card then GROWS as it types (subtitles, AI answers, the
 * thinking lines), and the newly revealed text extends below the viewport
 * middle. A ResizeObserver re-centers on every growth so the newest content
 * never leaves the middle of the screen.
 */
export function useCenterAnchor(
  ref: RefObject<HTMLDivElement | null>,
  active: unknown,
): void {
  useEffect(() => {
    if (!active) return;
    const element = ref.current;
    if (!element) return;
    const center = () => element.scrollIntoView?.({ block: 'center', behavior: 'auto' });
    center();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => center());
    observer.observe(element);
    return () => observer.disconnect();
  }, [active, ref]);
}
