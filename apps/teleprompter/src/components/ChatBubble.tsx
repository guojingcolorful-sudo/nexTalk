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
  /**
   * True for every bubble except the newest line (UAT-12): past subtitles
   * render complete immediately — only the line being spoken types out.
   */
  instant?: boolean;
  /**
   * GOV-14 / D-12: the translator never delivered for this segment. Carries
   * the aggregatable error code (D-19); the bubble shows the identical locked
   * degraded form as the desktop — original Chinese only, never a fabricated
   * English line, never a silent gap.
   */
  degraded?: { errorCode: string };
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
export default function ChatBubble({
  speaker,
  zh,
  en,
  language,
  instant = false,
  degraded,
}: ChatBubbleProps) {
  const isUser = speaker === 'user';

  const zhText = present(zh);
  const enText = present(en);
  const degradedView = degraded !== undefined;

  // The mode the phone owns (SYNC-03) FILTERS the bubble (UAT-4): 中 shows
  // the Chinese line only, EN the English line only, EN+中 both with the
  // spoken language as the primary. When the requested language has not
  // arrived in this subtitle, fall back to the other — never render empty.
  // Degraded (GOV-14/D-12) overrides all of it: the original Chinese is the
  // only line that may render, because no translation exists.
  let primary: string | undefined;
  let subline: string | undefined;
  if (degradedView) {
    primary = zhText;
  } else if (language === 'all-zh') {
    primary = zhText ?? enText;
  } else if (language === 'all-en') {
    primary = enText ?? zhText;
  } else {
    primary = (isUser ? zhText : enText) ?? (isUser ? enText : zhText);
    subline = primary === zhText ? enText : zhText;
  }

  if (primary === undefined && !degradedView) return null;

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
        {degraded !== undefined ? (
          <div
            className={`mb-1.5 flex flex-wrap items-center gap-2 ${isUser ? 'flex-row-reverse' : ''}`}
          >
            <span
              className="border-2 border-black bg-red-500 px-2 py-0.5 text-xs font-bold text-white shadow-[2px_2px_0_0_#000]"
              data-error-code={degraded.errorCode}
            >
              翻译失败
            </span>
            <span className="text-xs font-bold text-red-400">翻译服务暂时不可用</span>
            <span className="text-xs font-semibold text-gray-400">正在重试</span>
          </div>
        ) : null}
        {instant ? (
          <p className="min-h-[1.5em] whitespace-pre-wrap break-words text-[15px] font-semibold leading-normal text-white">
            {primary}
          </p>
        ) : (
          <TypedLine
            key={`${speaker}-${primary}`}
            text={primary}
            className="text-[15px] font-semibold leading-normal text-white"
          />
        )}
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
