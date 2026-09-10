import type { ReactNode } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import type { IconDefinition } from '@fortawesome/free-solid-svg-icons';

type PanelTone = 'gray' | 'yellow';

const TONE_CLASS: Record<PanelTone, string> = {
  gray: 'bg-gray-800 text-white',
  yellow: 'bg-mortyYellow text-black',
};

const ICON_CLASS: Record<PanelTone, string> = {
  gray: 'text-portalGreen',
  yellow: 'text-black',
};

interface PanelHeaderProps {
  tone: PanelTone;
  icon: IconDefinition;
  title: string;
  /** Trailing content (e.g. the pulsing fa-bolt while a strategy generates). */
  trailing?: ReactNode;
}

/**
 * PanelHeader — the 4px-bordered bar at the top of each dual-pane column
 * (gray for 实时字幕, mortyYellow for AI 辅助).
 */
export default function PanelHeader({ tone, icon, title, trailing }: PanelHeaderProps) {
  return (
    <div
      className={`flex shrink-0 items-center justify-between gap-2 border-b-4 border-black p-3 ${TONE_CLASS[tone]}`}
    >
      <h2 className="flex items-center gap-2 text-sm font-bold uppercase tracking-wider">
        <FontAwesomeIcon icon={icon} aria-hidden="true" className={ICON_CLASS[tone]} />
        {title}
      </h2>
      {trailing}
    </div>
  );
}
