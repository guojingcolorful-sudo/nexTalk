import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faCheck } from '@fortawesome/free-solid-svg-icons';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';
import NeobrutalismButton from './NeobrutalismButton';

interface KnowledgeRowProps {
  icon: IconDefinition;
  /** Row subject: a file name, a page name, or a feature name. */
  label: string;
  /** Right-hand status text when the row is not in its success state
   *  (variants: 尚未导入 / 3 个术语 / 未注册). */
  value?: string;
  state?: 'idle' | 'success';
  onClick: () => void;
  className?: string;
}

/**
 * KnowledgeRow (UI-SPEC Component Inventory): mortyYellow button-style row
 * with the leading icon, the label, and either the fa-check ready state or a
 * trailing status value. The whole row is the tap target — it navigates to
 * the page that manages that asset (简历导入 / 术语表 / 音色注册).
 *
 * Built on NeobrutalismButton so the press physics, focus ring, and disabled
 * cycle stay identical to every other control.
 */
export default function KnowledgeRow({
  icon,
  label,
  value,
  state = 'idle',
  onClick,
  className,
}: KnowledgeRowProps) {
  const classes = ['w-full', className ?? ''].filter(Boolean).join(' ');

  return (
    <NeobrutalismButton variant="yellow" align="between" onClick={onClick} className={classes}>
      <span className="flex min-w-0 items-center gap-2">
        <FontAwesomeIcon icon={icon} aria-hidden="true" className="shrink-0" />
        <span className="truncate">{label}</span>
      </span>
      {state === 'success' ? (
        <FontAwesomeIcon icon={faCheck} aria-hidden="true" className="shrink-0 text-green-700" />
      ) : (
        <span className="shrink-0 text-xs font-bold opacity-70">{value}</span>
      )}
    </NeobrutalismButton>
  );
}
