import type { ReactNode } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faXmark } from '@fortawesome/free-solid-svg-icons';

type ErrorTone = 'red' | 'yellow';

const TONE_CLASS: Record<ErrorTone, string> = {
  red: 'bg-red-500',
  yellow: 'bg-mortyYellow',
};

interface ErrorBannerProps {
  /** Short failure statement (Copywriting Contract: 配对失败, 麦克风不可用, ...). */
  title: string;
  /** The solution path — what the user should do next. */
  body: string;
  tone?: ErrorTone;
  /** Optional recovery control (e.g. a 重试 button). */
  action?: ReactNode;
  className?: string;
}

/**
 * Inline error banner (UI-SPEC Component Inventory): colored fill, black text,
 * 4px border, optional 重试. role="alert" announces the failure to assistive
 * tech without stealing focus.
 */
export default function ErrorBanner({
  title,
  body,
  tone = 'red',
  action,
  className,
}: ErrorBannerProps) {
  const classes = [
    'flex items-start gap-2 rounded-xl border-4 border-black p-3 text-black',
    TONE_CLASS[tone],
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div role="alert" className={classes}>
      <FontAwesomeIcon icon={faXmark} aria-hidden="true" className="mt-0.5 shrink-0" />
      <div className="min-w-0">
        <p className="text-[13px] font-bold">{title}</p>
        <p className="mt-0.5 text-xs font-semibold">{body}</p>
        {action ? <div className="mt-2">{action}</div> : null}
      </div>
    </div>
  );
}
