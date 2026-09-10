import { useEffect, useRef, useState } from 'react';

export type ToastKind = 'success' | 'warn' | 'error';

/** Colored 4px hard shadow per kind (UI-SPEC Component Inventory). The 6px
 *  token shadows stay reserved for surfaces and cards. */
const SHADOW_CLASS: Record<ToastKind, string> = {
  success: 'shadow-[4px_4px_0_0_#97ce4c]',
  warn: 'shadow-[4px_4px_0_0_#fbf061]',
  error: 'shadow-[4px_4px_0_0_#ef4444]',
};

/** Motion Contract: toasts auto-dismiss after 3s. */
const DEFAULT_DURATION_MS = 3000;

interface ToastProps {
  /** Transient-failure message (Copywriting Contract: 正在重连…, 已启用防休眠回退模式). */
  title: string;
  body?: string;
  kind?: ToastKind;
  durationMs?: number;
  onDismiss: () => void;
}

/**
 * Transient notification (UI-SPEC Component Inventory): top-center, spaceDark
 * card, colored hard shadow, auto-dismiss after 3s. Reserved for transient
 * failures and one-off confirmations — anything that needs to persist belongs
 * in an ErrorBanner above the affected region.
 */
export default function Toast({
  title,
  body,
  kind = 'success',
  durationMs = DEFAULT_DURATION_MS,
  onDismiss,
}: ToastProps) {
  const [shown, setShown] = useState(false);
  const dismissRef = useRef(onDismiss);

  useEffect(() => {
    dismissRef.current = onDismiss;
  }, [onDismiss]);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => setShown(true));
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => dismissRef.current(), durationMs);
    return () => window.clearTimeout(timer);
  }, [durationMs]);

  const classes = [
    'fixed left-1/2 top-4 z-50 max-w-[300px] -translate-x-1/2 rounded-xl border-4 border-black',
    'bg-spaceDark px-3 py-2 text-white',
    // Reduced motion: the slide collapses, the toast still appears (instantly).
    'transition duration-200 ease-out motion-reduce:transition-none',
    shown ? 'translate-y-0 opacity-100' : '-translate-y-3 opacity-0',
    SHADOW_CLASS[kind],
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div role={kind === 'error' ? 'alert' : 'status'} className={classes} onClick={onDismiss}>
      <p className="text-[13px] font-bold">{title}</p>
      {body ? <p className="mt-0.5 text-xs text-gray-400">{body}</p> : null}
    </div>
  );
}
