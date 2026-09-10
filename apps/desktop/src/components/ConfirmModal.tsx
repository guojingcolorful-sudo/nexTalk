import { useEffect, useId, useRef } from 'react';
import NeobrutalismButton from './NeobrutalismButton';

const FOCUSABLE_SELECTOR = 'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ConfirmModalProps {
  open: boolean;
  /** Question form, e.g. 删除术语「K8s」？ (Copywriting Contract). */
  title: string;
  /** Consequence sentence, e.g. 该术语将不再受保护. */
  body?: string;
  cancelLabel?: string;
  /** The destructive verb, e.g. 删除 / 移除 / 停止. */
  confirmLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
}

/**
 * Confirmation dialog (UI-SPEC Component Inventory): spaceDark card with a 4px
 * black border and 8px black shadow over a black/50 overlay.
 *
 * Accessibility contract: focus is trapped inside the dialog while it is open,
 * Esc closes it, and focus returns to the element that opened it. Destructive
 * actions are never performed without this explicit second step.
 */
export default function ConfirmModal({
  open,
  title,
  body,
  cancelLabel = '取消',
  confirmLabel,
  onCancel,
  onConfirm,
}: ConfirmModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const node = dialogRef.current;
    const focusables = node ? Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)) : [];

    focusables[0]?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCancel();
        return;
      }
      if (event.key !== 'Tab' || focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      previouslyFocused?.focus?.();
    };
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="w-full rounded-xl border-4 border-black bg-spaceDark p-4 shadow-cartoon-black-lg"
      >
        <p id={titleId} className="text-[15px] font-bold text-white">
          {title}
        </p>
        {body ? <p className="mt-2 text-xs text-gray-400">{body}</p> : null}
        <div className="mt-4 flex justify-end gap-2">
          <NeobrutalismButton variant="ghost" size="sm" onClick={onCancel}>
            {cancelLabel}
          </NeobrutalismButton>
          <NeobrutalismButton variant="red" size="sm" onClick={onConfirm}>
            {confirmLabel}
          </NeobrutalismButton>
        </div>
      </div>
    </div>
  );
}
