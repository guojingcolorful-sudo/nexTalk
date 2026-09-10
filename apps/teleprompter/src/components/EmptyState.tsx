import type { ReactNode } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';

type EmptyTone = 'green' | 'yellow' | 'blue' | 'gray';

const TONE_CLASS: Record<EmptyTone, string> = {
  green: 'bg-portalGreen text-black',
  yellow: 'bg-mortyYellow text-black',
  blue: 'bg-rickBlue text-black',
  gray: 'bg-gray-700 text-gray-300',
};

interface EmptyStateProps {
  icon: IconDefinition;
  /** 13px/700 heading (Copywriting Contract: 等待语音输入, AI 策略将自动生成, ...). */
  title: string;
  /** 12px/400 gray-400 body — states the next step, never generic filler. */
  body: string;
  tone?: EmptyTone;
  /** Optional primary CTA. */
  action?: ReactNode;
  className?: string;
}

/**
 * Composed empty state (UI-SPEC Component Inventory): icon tile + heading +
 * body + optional CTA. On the phone, empty is the pre-session state, not an
 * error — both tabs pair their copy with the locked Copywriting Contract lines.
 */
export default function EmptyState({
  icon,
  title,
  body,
  tone = 'green',
  action,
  className,
}: EmptyStateProps) {
  const classes = [
    'flex flex-col items-center gap-1 rounded-xl border-4 border-black bg-spaceDark p-4 text-center',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div className={classes}>
      <span
        aria-hidden="true"
        className={`flex h-10 w-10 items-center justify-center rounded-lg border-2 border-black ${TONE_CLASS[tone]}`}
      >
        <FontAwesomeIcon icon={icon} />
      </span>
      <p className="mt-2 text-[13px] font-bold text-white">{title}</p>
      <p className="text-xs text-gray-400">{body}</p>
      {action ? <div className="mt-3">{action}</div> : null}
    </div>
  );
}
