import type { Ref } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faBrain } from '@fortawesome/free-solid-svg-icons';
import { useRevealCount } from '../hooks/useRevealCount';

/**
 * The locked thinking-process lines (UAT-14): each matures in turn while the
 * newest question awaits its strategy, so the AI tab reads as live thinking
 * instead of an empty wait. In Phase 2 the real copilot reasoning replaces
 * these canned lines. The desktop hosts the mirror component.
 */
const THINKING_LINES = ['正在分析问题要点', '正在匹配你的简历与经验', '正在组织回答结构'] as const;

/**
 * ThinkingCard — the AI 思考中 state for the newest question: a pulsing icon,
 * the label, and the thinking lines revealing one by one (~1.2s apart).
 * Under prefers-reduced-motion every line is present immediately.
 */
export default function ThinkingCard({ nodeRef }: { nodeRef?: Ref<HTMLDivElement> }) {
  const revealed = useRevealCount(THINKING_LINES.length, 1200);

  return (
    <div
      ref={nodeRef}
      role="status"
      aria-label="AI 思考中"
      className="flex items-start gap-2 self-start rounded-xl border-4 border-black bg-white px-3 py-2 shadow-[4px_4px_0_0_#fbf061]"
    >
      <FontAwesomeIcon
        icon={faBrain}
        aria-hidden="true"
        className="mt-0.5 animate-pulse text-mortyYellow [filter:drop-shadow(0_1px_0_#000)]"
      />
      <div>
        <p className="text-xs font-bold uppercase text-gray-600">AI 思考中</p>
        <ul className="mt-1 space-y-0.5">
          {THINKING_LINES.slice(0, revealed).map((line) => (
            <li key={line} className="text-xs font-semibold leading-tight text-black">
              {line}
            </li>
          ))}
        </ul>
      </div>
      <span aria-hidden="true" className="mt-2 flex items-center gap-1">
        {[0, 1, 2].map((dot) => (
          <span
            key={dot}
            className="h-1.5 w-1.5 animate-bounce rounded-full bg-mortyYellow"
            style={{ animationDelay: `${dot * 150}ms` }}
          />
        ))}
      </span>
    </div>
  );
}
