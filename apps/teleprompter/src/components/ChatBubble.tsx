import type { Speaker } from '@nextalk/protocol';
import { useTypewriter } from '../hooks/useTypewriter';

/**
 * TypedLine — one language line revealed character by character (SYNC-05
 * typewriter, 40ms/char, instant under prefers-reduced-motion). The wrapper
 * reserves one line of height so the bubble does not jump while it fills.
 */
export function TypedLine({ text, className }: { text: string; className?: string }) {
  const shown = useTypewriter(text);
  return <p className={`min-h-[1.5em] whitespace-pre-wrap break-words ${className ?? ''}`}>{shown}</p>;
}

interface ChatBubbleProps {
  speaker: Speaker;
  zh?: string;
  en?: string;
}

const SPEAKER_LABEL: Record<Speaker, string> = {
  interviewer: '面试官',
  user: '我',
};

function present(text?: string): string | undefined {
  return text !== undefined && text.trim().length > 0 ? text : undefined;
}

/**
 * ChatBubble — one subtitle in the phone's stream (UI-SPEC Component
 * Inventory, mobile variant): the language actually spoken is the primary
 * line, the other language tucks under it as the translation subline. Speaker
 * is carried by alignment + color + label, never by color alone (a11y).
 *
 * Contrast contract: bubbles sit on dark neutrals (slate-800 / green-900), so
 * message text is white; the interviewer's translation is rickBlue, the
 * user's is portalGreen.
 */
export default function ChatBubble({ speaker, zh, en }: ChatBubbleProps) {
  const isUser = speaker === 'user';

  const zhText = present(zh);
  const enText = present(en);
  const primary = isUser ? zhText : enText;
  const secondary = isUser ? enText : zhText;

  const primaryText = primary ?? secondary;
  if (primaryText === undefined) return null;
  const subline = primary === undefined ? undefined : secondary;

  return (
    <article
      aria-label={SPEAKER_LABEL[speaker]}
      className={`flex w-full flex-col ${isUser ? 'items-end' : 'items-start'}`}
    >
      <span className="mb-1 text-xs font-bold uppercase tracking-wider text-gray-400">
        {SPEAKER_LABEL[speaker]}
      </span>
      <div
        className={`w-fit max-w-[95%] rounded-xl border-2 p-3 ${
          isUser
            ? 'rounded-tr-none border-portalGreen bg-green-900'
            : 'rounded-tl-none border-black bg-slate-800'
        }`}
      >
        <TypedLine
          key={`${speaker}-${primaryText}`}
          text={primaryText}
          className="text-[15px] font-semibold leading-normal text-white"
        />
        {subline !== undefined ? (
          <p
            className={`mt-1.5 whitespace-pre-wrap break-words font-semibold ${
              isUser ? 'text-[13px] text-portalGreen' : 'text-[12px] text-rickBlue'
            }`}
          >
            {subline}
          </p>
        ) : null}
      </div>
    </article>
  );
}
