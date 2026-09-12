import type { LanguagePref, Speaker } from '@nextalk/protocol';
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
  /** Session language mode the phone owns (SYNC-03); absent = speaker default. */
  language?: LanguagePref;
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
 * Inventory, mobile variant): the session language mode picks the primary
 * line, the other language tucks under it as the translation subline. Without
 * a mode the language actually spoken is the primary (user = zh,
 * interviewer = en). Speaker is carried by alignment + color + label, never
 * by color alone (a11y).
 *
 * Contrast contract: bubbles sit on dark neutrals (slate-800 / green-900), so
 * message text is white; the interviewer's translation is rickBlue, the
 * user's is portalGreen.
 */
export default function ChatBubble({ speaker, zh, en, language }: ChatBubbleProps) {
  const isUser = speaker === 'user';

  const zhText = present(zh);
  const enText = present(en);

  // The mode the phone owns (SYNC-03) drives the primary line; when the
  // requested language has not arrived in this subtitle, fall back to the
  // other one — the bubble never renders empty.
  let primary: string | undefined;
  if (language === 'all-zh') primary = zhText ?? enText;
  else if (language === 'all-en') primary = enText ?? zhText;
  else primary = (isUser ? zhText : enText) ?? (isUser ? enText : zhText);

  if (primary === undefined) return null;
  const subline = primary === zhText ? enText : zhText;

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
          key={`${speaker}-${primary}`}
          text={primary}
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
