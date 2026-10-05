import { useState } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faMicrophone, faUserTie } from '@fortawesome/free-solid-svg-icons';
import type { LanguagePref, Speaker } from '@nextalk/protocol';
import { useTypewriter } from '../hooks/useTypewriter';
import LanguageToggle from './LanguageToggle';

/**
 * SYNC-03 defaults: the interviewer speaks English, so their bubble opens
 * bilingual (EN + 中); the user speaks Chinese, so their bubble opens
 * Chinese-only (the English line is what the cloned voice will say). The
 * preference is LOCAL per bubble in Phase 1 — a bubble switch must not touch
 * its neighbors.
 */
export const SPEAKER_DEFAULT_PREF: Record<Speaker, LanguagePref> = {
  interviewer: 'bilingual',
  user: 'all-zh',
};

interface ChatBubbleProps {
  speaker: Speaker;
  zh?: string;
  en?: string;
  /** Session mode applied from the phone (SYNC-03), or null while the session
   *  runs on the per-speaker defaults. */
  mode?: LanguagePref | null;
  /**
   * True for every bubble except the newest line (UAT-12): past subtitles
   * render complete immediately — only the line being spoken types out.
   */
  instant?: boolean;
  /**
   * GOV-14 / D-12: the translator never delivered for this segment. Carries
   * the aggregatable error code (D-19); the bubble falls back to the original
   * Chinese and shows the locked degraded copy. An English line is never
   * fabricated, and every language preference is overridden while degraded.
   */
  degraded?: { errorCode: string };
  /**
   * D-03: the segment produced no speakable text — the bubble renders the
   * locked 「待翻译」 state instead of a message. The state outranks any text
   * that arrived with it, and it is never a confidence mark (GOV-01/02,
   * 2026-09-30: subtitles carry no confidence badges).
   */
  abstained?: boolean;
}

function present(text?: string): string | undefined {
  return text !== undefined && text.trim().length > 0 ? text : undefined;
}

/**
 * ChatBubble — one subtitle in the live stream. Primary line = the language
 * the speaker actually uttered; the other language tucks under it as the
 * subline while the bubble is bilingual. A payload with no text at all
 * renders nothing rather than an empty bubble.
 *
 * Language resolution order: a toggle the user pressed on this bubble wins,
 * then the session mode the phone applied, then the speaker default — so an
 * untouched bubble follows the phone live while a deliberate local choice is
 * never overridden.
 *
 * GOV-14 degraded form (D-12): when `degraded` is set the resolution above is
 * bypassed entirely — the bubble renders the original Chinese and the locked
 * 「翻译失败」badge + 「翻译服务暂时不可用」/「正在重试」copy. Never an English
 * line (the translation was never produced), never a silent gap.
 */
export default function ChatBubble({
  speaker,
  zh,
  en,
  mode = null,
  instant = false,
  degraded,
  abstained = false,
}: ChatBubbleProps) {
  const [localPref, setLocalPref] = useState<LanguagePref | null>(null);
  const pref = localPref ?? mode ?? SPEAKER_DEFAULT_PREF[speaker];
  const isUser = speaker === 'user';

  const zhText = present(zh);
  const enText = present(en);
  const degradedView = degraded !== undefined;

  let primary: string | undefined;
  let secondary: string | undefined;
  if (degradedView) {
    // GOV-14/D-12: the original Chinese is the only truth — no language
    // preference may produce an English line the pipeline never translated.
    primary = zhText;
  } else if (pref === 'bilingual') {
    primary = isUser ? zhText : enText;
    secondary = isUser ? enText : zhText;
  } else if (pref === 'all-zh') {
    primary = zhText;
  } else {
    primary = enText;
  }

  // Fall back to the other language when the requested one has not arrived
  // yet (the user's English is produced by the clone, not by the user).
  if (!degradedView && primary === undefined) {
    primary = secondary ?? (pref === 'all-zh' ? enText : zhText);
    secondary = undefined;
  }
  // UAT-10/12: the newest line teleprompters (40ms/char, instant under
  // reduced motion); past subtitles render complete immediately. The subline
  // always stays instant so it never lags behind. The hook runs before any
  // early return: a bubble can gain or lose text between frames, and a
  // conditional hook would change the hook count mid-stream.
  const typedPrimary = useTypewriter(primary ?? '');
  const shownPrimary = instant ? (primary ?? '') : typedPrimary;

  if (primary === undefined && !degradedView && !abstained) return null;

  // D-03: no speakable text — the card is a STATE, not a message. It renders
  // as its own locked 「待翻译」 form (no language toggle: there is nothing to
  // toggle) and outranks any text that arrived with the event.
  if (abstained) {
    return (
      <div className={`flex w-[95%] flex-col gap-1 ${isUser ? 'self-end' : ''}`}>
        <div className={`mb-1 flex items-center gap-2 ${isUser ? 'flex-row-reverse' : ''}`}>
          <span
            className={`text-xs font-bold uppercase ${isUser ? 'text-portalGreen' : 'text-gray-400'}`}
          >
            <FontAwesomeIcon icon={isUser ? faMicrophone : faUserTie} aria-hidden="true" className="mr-1" />
            {isUser ? '用户' : '面试官'}
          </span>
        </div>
        <p
          className={`min-h-[1.5em] rounded-xl border-2 border-dashed p-3 text-[15px] font-semibold text-gray-400 ${
            isUser
              ? 'rounded-tr-none border-portalGreen bg-green-900 text-right'
              : 'rounded-tl-none border-gray-600 bg-slate-800'
          }`}
        >
          待翻译
        </p>
      </div>
    );
  }

  return (
    <div className={`flex w-[95%] flex-col gap-1 ${isUser ? 'self-end' : ''}`}>
      <div className={`mb-1 flex items-center justify-between gap-2 ${isUser ? 'flex-row-reverse' : ''}`}>
        <span
          className={`text-xs font-bold uppercase ${isUser ? 'text-portalGreen' : 'text-gray-400'}`}
        >
          <FontAwesomeIcon icon={isUser ? faMicrophone : faUserTie} aria-hidden="true" className="mr-1" />
          {isUser ? '用户' : '面试官'}
        </span>
        <LanguageToggle speaker={speaker} value={pref} onChange={setLocalPref} />
      </div>

      {degraded !== undefined ? (
        <div
          className={`flex flex-wrap items-center gap-2 ${isUser ? 'flex-row-reverse' : ''}`}
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

      <p
        className={`min-h-[1.5em] rounded-xl border-2 p-3 text-[15px] text-white ${
          isUser
            ? 'rounded-tr-none border-portalGreen bg-green-900 text-right font-bold shadow-[2px_2px_0_0_#97ce4c]'
            : 'rounded-tl-none border-gray-600 bg-slate-800 font-semibold'
        }`}
      >
        {shownPrimary}
      </p>

      {secondary !== undefined ? (
        <p
          className={`-mt-2 rounded-xl border-2 border-t-0 p-2 pt-3 text-[13px] font-bold ${
            isUser
              ? 'rounded-tl-none rounded-tr-none border-portalGreen bg-slate-800 text-right text-portalGreen'
              : 'rounded-tl-none rounded-tr-none border-gray-600 bg-slate-700 text-rickBlue'
          }`}
        >
          {secondary}
        </p>
      ) : null}
    </div>
  );
}
