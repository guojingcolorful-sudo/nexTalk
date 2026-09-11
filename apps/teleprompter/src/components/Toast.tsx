import { useEffect } from 'react';

type ToastTone = 'green' | 'yellow' | 'red';

const TONE_SHADOW: Record<ToastTone, string> = {
  green: 'shadow-[4px_4px_0_0_#97ce4c]',
  yellow: 'shadow-[4px_4px_0_0_#fbf061]',
  red: 'shadow-[4px_4px_0_0_#ef4444]',
};

interface ToastProps {
  message: string;
  tone?: ToastTone;
  /** Auto-dismiss length (UI-SPEC Motion Contract: 3s). */
  durationMs?: number;
  onDismiss: () => void;
}

/**
 * Toast — transient, non-blocking feedback (UI-SPEC Component Inventory):
 * top-center, spaceDark card, 4px border, colored hard shadow, 3s auto-dismiss.
 * Reserve it for states the user cannot see any other way (the wake-lock
 * fallback is silent by nature), never for errors that need a fix.
 */
export default function Toast({ message, tone = 'yellow', durationMs = 3000, onDismiss }: ToastProps) {
  useEffect(() => {
    const timer = window.setTimeout(onDismiss, durationMs);
    return () => window.clearTimeout(timer);
  }, [onDismiss, durationMs]);

  return (
    <div
      role="status"
      className={`pointer-events-none fixed left-1/2 top-4 z-50 -translate-x-1/2 rounded-xl border-4 border-black bg-spaceDark px-4 py-2 text-xs font-bold text-white ${TONE_SHADOW[tone]}`}
    >
      {message}
    </div>
  );
}
