import type { Ref } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faRobot, faUserTie } from '@fortawesome/free-solid-svg-icons';
import type { ServerEvent } from '@nextalk/protocol';

export type TimelineItem =
  | { kind: 'context'; id: string; en?: string; zh?: string }
  | {
      kind: 'strategy';
      id: string;
      title: string;
      bullets: readonly string[];
      answerZh?: string;
      answerEn?: string;
    };

/**
 * Maps the narrowed server events onto right-pane timeline nodes: every
 * interviewer sentence becomes a context node, every strategy event becomes
 * a strategy card. The user's own subtitles stay in the left stream — they
 * are not context for the copilot.
 */
export function toTimelineItems(events: readonly ServerEvent[]): TimelineItem[] {
  const items: TimelineItem[] = [];
  for (const event of events) {
    if (event.t === 'subtitle') {
      if (event.speaker === 'interviewer') {
        items.push({ kind: 'context', id: event.id, en: event.en, zh: event.zh });
      }
    } else if (event.t === 'strategy') {
      items.push({
        kind: 'strategy',
        id: event.id,
        title: event.title,
        bullets: event.bullets,
        answerZh: event.answerZh,
        answerEn: event.answerEn,
      });
    }
  }
  return items;
}

function present(text?: string): string | undefined {
  return text !== undefined && text.trim().length > 0 ? text : undefined;
}

const NODE_CLASS = 'z-10 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border-2 border-black';

/**
 * AiTimeline — the right-pane node column (UI-SPEC: guide line, 24px icon
 * nodes, context card then strategy cards).
 *
 * The UI-SPEC's green draft node has no producer in Phase 1: the locked
 * `ServerEvent` union carries subtitle | strategy | status | language |
 * timeline only, so the draft card renders once a draft event exists
 * (tracked in the plan summary, not fabricated here).
 */
export default function AiTimeline({
  items,
  lastItemRef,
}: {
  items: readonly TimelineItem[];
  /** Attached to the newest item so the pane can center it (UAT-9). */
  lastItemRef?: Ref<HTMLDivElement>;
}) {
  return (
    <div className="relative flex flex-col gap-4">
      <div aria-hidden="true" className="absolute left-[11px] top-0 h-full w-1 bg-gray-700" />
      {items.map((item, index) => {
        const isLast = index === items.length - 1;
        if (item.kind === 'strategy') {
          return (
            <div key={item.id} ref={isLast ? lastItemRef : null} className="flex items-start gap-3">
              <div aria-hidden="true" className={`${NODE_CLASS} bg-portalGreen text-black`}>
                <FontAwesomeIcon icon={faRobot} className="text-[10px]" />
              </div>
              <section className="w-full rounded-xl border-4 border-black bg-white p-3 text-black shadow-cartoon-yellow">
                <p className="mb-2 text-[10px] font-bold uppercase text-gray-600">策略</p>
                <h3 className="mb-2 text-sm font-bold">{item.title}</h3>
                {item.bullets.length > 0 ? (
                  <ul className="flex list-disc flex-col gap-1 pl-4 text-sm font-bold leading-tight">
                    {item.bullets.map((bullet, index) => (
                      <li key={`${item.id}-${index}`}>{bullet}</li>
                    ))}
                  </ul>
                ) : null}
                {item.answerZh || item.answerEn ? (
                  <section
                    aria-label="AI 智能回答"
                    className="mt-2 space-y-1.5 border-t-2 border-dashed border-gray-300 pt-2"
                  >
                    <p className="text-[10px] font-bold uppercase text-gray-500">AI 智能回答</p>
                    {item.answerZh ? (
                      <div className="rounded-md border-2 border-black bg-spaceDark p-2">
                        <p className="mb-0.5 text-[9px] font-bold uppercase text-gray-400">中文回答</p>
                        <p className="whitespace-pre-wrap break-words text-xs font-semibold leading-relaxed text-white">
                          {item.answerZh}
                        </p>
                      </div>
                    ) : null}
                    {item.answerEn ? (
                      <div className="rounded-md border-2 border-black bg-spaceDark p-2">
                        <p className="mb-0.5 text-[9px] font-bold uppercase text-gray-400">
                          English answer
                        </p>
                        <p className="whitespace-pre-wrap break-words text-xs font-semibold leading-relaxed text-white">
                          {item.answerEn}
                        </p>
                      </div>
                    ) : null}
                  </section>
                ) : null}
              </section>
            </div>
          );
        }

        const en = present(item.en);
        const zh = present(item.zh);
        const primary = en ?? zh;
        const secondary = en !== undefined ? zh : undefined;
        if (primary === undefined) return null;

        return (
          <div key={item.id} ref={isLast ? lastItemRef : null} className="flex items-start gap-3">
            <div aria-hidden="true" className={`${NODE_CLASS} bg-slate-600 text-white`}>
              <FontAwesomeIcon icon={faUserTie} className="text-[10px]" />
            </div>
            <section className="w-full rounded-xl border-2 border-black bg-slate-800 p-3 text-sm shadow-cartoon-black">
              <span className="mb-1 block font-bold text-white">{primary}</span>
              {secondary !== undefined ? (
                <span className="block text-xs text-gray-400">{secondary}</span>
              ) : null}
            </section>
          </div>
        );
      })}
    </div>
  );
}
