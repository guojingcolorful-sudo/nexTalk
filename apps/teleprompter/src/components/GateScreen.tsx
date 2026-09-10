import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faBolt, faLanguage, faPause } from '@fortawesome/free-solid-svg-icons';
import type { LanguagePref } from '@nextalk/protocol';

/** Locked language segments (UI-SPEC exceptions: 中 / EN / EN+中 stay non-Chinese). */
const LANGUAGE_LABEL: Record<LanguagePref, string> = {
  'all-zh': '中',
  'all-en': 'EN',
  bilingual: 'EN+中',
};

interface GateScreenProps {
  /** 提词 is running (button flips to 暂停提词). */
  sessionActive: boolean;
  /** Screen is being kept awake (wake lock or video fallback). */
  wakeActive?: boolean;
  languagePref: LanguagePref;
  onToggleSession: () => void;
  onCycleLanguage: () => void;
}

/**
 * GateScreen — the phone's bottom action bar (UI-SPEC Component Inventory +
 * Copywriting Contract). Before the session it holds the 开始提词 primary CTA;
 * that same tap is the user gesture the wake lock needs (SYNC-04), which is
 * why the control lives down here in the thumb-reach zone rather than in a
 * header. During the session it flips to 暂停提词 and reports the stay-awake
 * state, and the language segment cycles the one session-level mode the phone
 * pushes back to the desktop (SYNC-03).
 */
export default function GateScreen({
  sessionActive,
  wakeActive = false,
  languagePref,
  onToggleSession,
  onCycleLanguage,
}: GateScreenProps) {
  return (
    <div className="flex shrink-0 flex-col gap-2 border-t-4 border-black bg-spaceDark px-4 pt-3 pb-[max(12px,env(safe-area-inset-bottom))]">
      {sessionActive && wakeActive ? (
        <p className="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-portalGreen">
          <span aria-hidden="true" className="h-2 w-2 rounded-full bg-portalGreen" />
          屏幕常亮已开启
        </p>
      ) : null}

      <div className="flex items-stretch gap-2">
        <button
          type="button"
          onClick={onToggleSession}
          className={[
            'inline-flex h-12 flex-1 items-center justify-center gap-2 rounded-xl border-4 border-black',
            'text-sm font-bold uppercase tracking-wider',
            'transition duration-150 ease-out hover:translate-y-1 hover:shadow-none active:scale-[0.98]',
            'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen',
            sessionActive
              ? 'bg-gray-400 text-black shadow-[4px_4px_0_0_#000]'
              : 'bg-portalGreen text-black shadow-[4px_4px_0_0_#000]',
          ].join(' ')}
        >
          <FontAwesomeIcon icon={sessionActive ? faPause : faBolt} aria-hidden="true" />
          {sessionActive ? '暂停提词' : '开始提词'}
        </button>

        <button
          type="button"
          onClick={onCycleLanguage}
          aria-label={`语言模式 ${LANGUAGE_LABEL[languagePref]}，点击切换`}
          className={[
            'inline-flex min-w-[64px] items-center justify-center gap-2 rounded-xl border-4 border-black bg-panel px-3',
            'text-xs font-bold uppercase tracking-wider text-white',
            'transition duration-150 ease-out hover:translate-y-1 hover:shadow-none active:scale-[0.98]',
            'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen',
          ].join(' ')}
        >
          <FontAwesomeIcon icon={faLanguage} aria-hidden="true" />
          {LANGUAGE_LABEL[languagePref]}
        </button>
      </div>
    </div>
  );
}
