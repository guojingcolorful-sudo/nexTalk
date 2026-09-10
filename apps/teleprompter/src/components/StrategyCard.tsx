import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faLightbulb } from '@fortawesome/free-solid-svg-icons';

interface StrategyCardProps {
  title: string;
  bullets: string[];
  /** Round the strategy answers, shown as the card's context stamp. */
  roundId?: string;
}

/**
 * StrategyCard — the AI 辅助 tab's reading surface (UI-SPEC Component
 * Inventory): paper-white card, 4px black border and the yellow 6px hard
 * shadow that marks every AI surface. Text is rendered as React text nodes
 * only — inbound WS payloads are never interpreted as markup (T-01-12).
 */
export default function StrategyCard({ title, bullets, roundId }: StrategyCardProps) {
  return (
    <article className="w-full rounded-xl border-4 border-black bg-white p-4 text-black shadow-[6px_6px_0_0_#fbf061]">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-bold uppercase tracking-wider text-gray-500">AI 策略</span>
        {roundId ? (
          <span className="rounded-md border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold uppercase">
            {roundId}
          </span>
        ) : null}
      </div>

      <h2 className="mt-2 text-[15px] font-bold uppercase tracking-wider">{title}</h2>

      <ul className="mt-2 space-y-1.5">
        {bullets.map((bullet) => (
          <li key={bullet} className="flex gap-2 text-[15px] font-semibold leading-normal">
            <FontAwesomeIcon
              icon={faLightbulb}
              aria-hidden="true"
              className="mt-1 shrink-0 text-mortyYellow [filter:drop-shadow(0_1px_0_#000)]"
            />
            <span>{bullet}</span>
          </li>
        ))}
      </ul>
    </article>
  );
}
