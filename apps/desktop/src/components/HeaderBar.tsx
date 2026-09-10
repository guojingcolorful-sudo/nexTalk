import type { ReactNode } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faArrowLeft, faMinus, faXmark } from '@fortawesome/free-solid-svg-icons';
import { getCurrentWindow } from '@tauri-apps/api/window';

type HeaderTone = 'green' | 'blue' | 'yellow';

const TONE_CLASS: Record<HeaderTone, string> = {
  green: 'bg-portalGreen',
  blue: 'bg-rickBlue',
  yellow: 'bg-mortyYellow',
};

interface HeaderBarProps {
  tone: HeaderTone;
  title?: string;
  /** Renders the fa-arrow-left back affordance (missing pages open inside the
   *  console window with a back control per the Navigation contract). */
  onBack?: () => void;
  backLabel?: string;
  /** Trailing content before the window controls (e.g. MicStatusPill). */
  actions?: ReactNode;
  children?: ReactNode;
}

/**
 * HeaderBar per UI-SPEC: colored per surface (portalGreen console /
 * rickBlue dual-pane / mortyYellow AI panel), border-b-4, uppercase 18px
 * title, icon-only window controls (min/close) with aria-labels.
 * All cross-window state flows through Rust — this bar owns window chrome only.
 */
export default function HeaderBar({
  tone,
  title,
  onBack,
  backLabel = '返回控制台',
  actions,
  children,
}: HeaderBarProps) {
  const toneClass = TONE_CLASS[tone];

  const minimize = () => {
    try {
      void getCurrentWindow().minimize();
    } catch {
      // Not running inside a Tauri webview (plain browser dev) — no-op.
    }
  };

  const close = () => {
    try {
      void getCurrentWindow().close();
    } catch {
      // Not running inside a Tauri webview (plain browser dev) — no-op.
    }
  };

  return (
    <header className={`flex h-14 shrink-0 items-center justify-between border-b-4 border-black px-3 ${toneClass}`}>
      <div className="flex min-w-0 items-center gap-2 text-black">
        {onBack ? (
          <button
            type="button"
            aria-label={backLabel}
            onClick={onBack}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-black transition hover:bg-black hover:text-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-black"
          >
            <FontAwesomeIcon icon={faArrowLeft} />
          </button>
        ) : null}
        {children}
        {title ? <h1 className="truncate text-lg font-bold uppercase tracking-wider">{title}</h1> : null}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {actions}
        <button
          type="button"
          aria-label="最小化"
          onClick={minimize}
          className="flex h-8 w-8 items-center justify-center rounded-md text-black transition hover:bg-black hover:text-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-black"
        >
          <FontAwesomeIcon icon={faMinus} />
        </button>
        <button
          type="button"
          aria-label="关闭"
          onClick={close}
          className="flex h-8 w-8 items-center justify-center rounded-md text-black transition hover:bg-black hover:text-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-black"
        >
          <FontAwesomeIcon icon={faXmark} />
        </button>
      </div>
    </header>
  );
}
