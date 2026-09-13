import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faLightbulb, faRobot } from '@fortawesome/free-solid-svg-icons';
import { useRevealCount } from '../hooks/useRevealCount';
import { useTypewriter } from '../hooks/useTypewriter';

interface StrategyCardProps {
  title: string;
  bullets: string[];
  /** Round the strategy answers, shown as the card's context stamp. */
  roundId?: string;
  /** The AI's bilingual suggested answer (UAT-8); hidden while absent. */
  answerZh?: string;
  answerEn?: string;
}

/**
 * TypedAnswer — one AI 智能回答 block. Mounted only after the outline has
 * fully matured (UAT-11), so the typing visibly starts once the bullets are
 * all on screen — the card reads as title → outline → written answer.
 */
function TypedAnswer({ label, text }: { label: string; text: string }) {
  const shown = useTypewriter(text);
  return (
    <div className="rounded-lg border-2 border-black bg-spaceDark p-2.5">
      <p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-gray-400">{label}</p>
      <p className="min-h-[1.5em] whitespace-pre-wrap break-words text-[14px] font-semibold leading-relaxed text-white">
        {shown}
      </p>
    </div>
  );
}

/**
 * StrategyCard — the AI 辅助 tab's reading surface (UI-SPEC Component
 * Inventory): paper-white card, 4px black border and the yellow 6px hard
 * shadow that marks every AI surface. Text is rendered as React text nodes
 * only — inbound WS payloads are never interpreted as markup (T-01-12).
 *
 * UAT-11: the card reveals as a thinking process — the strategy bullets
 * mature one by one (600ms apart), and only after the outline is complete
 * does the AI 智能回答 type itself out. Under prefers-reduced-motion the
 * whole card is present immediately.
 */
export default function StrategyCard({
  title,
  bullets,
  roundId,
  answerZh,
  answerEn,
}: StrategyCardProps) {
  const revealed = useRevealCount(bullets.length);
  const outlineComplete = revealed >= bullets.length;

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
        {bullets.slice(0, revealed).map((bullet) => (
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

      {outlineComplete && (answerZh || answerEn) ? (
        <section
          aria-label="AI 智能回答"
          className="mt-3 space-y-2 border-t-4 border-dashed border-gray-300 pt-3"
        >
          <p className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-gray-500">
            <FontAwesomeIcon icon={faRobot} aria-hidden="true" />
            AI 智能回答
          </p>
          {answerZh ? <TypedAnswer label="中文回答" text={answerZh} /> : null}
          {answerEn ? <TypedAnswer label="English answer" text={answerEn} /> : null}
        </section>
      ) : null}
    </article>
  );
}
