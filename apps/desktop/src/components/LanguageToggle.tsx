import type { LanguagePref, Speaker } from '@nextalk/protocol';

const SEGMENTS: ReadonlyArray<{ value: LanguagePref; label: string }> = [
  { value: 'all-zh', label: '中' },
  { value: 'all-en', label: 'EN' },
  { value: 'bilingual', label: 'EN+中' },
];

interface LanguageToggleProps {
  speaker: Speaker;
  value: LanguagePref;
  onChange: (next: LanguagePref) => void;
}

/**
 * LanguageToggle per UI-SPEC: a black container with a 2px border and
 * 10px/700 中 / EN / EN+中 segments. The selected segment is filled with the
 * speaker brand color (portalGreen for the user, gray-600 for the
 * interviewer) and every segment is an aria-pressed button inside a labelled
 * role="group", so the control is operable and announced without a mouse.
 */
export default function LanguageToggle({ speaker, value, onChange }: LanguageToggleProps) {
  const isUser = speaker === 'user';
  const frameClass = isUser ? 'border-portalGreen' : 'border-gray-600';
  const selectedClass = isUser ? 'bg-portalGreen text-black' : 'bg-gray-600 text-white';
  const idleClass = isUser ? 'text-portalGreen hover:bg-gray-800' : 'text-gray-400 hover:text-white';

  return (
    <div
      role="group"
      aria-label={isUser ? '用户语言' : '面试官语言'}
      className={`flex shrink-0 rounded-full border-2 bg-black ${frameClass}`}
    >
      {SEGMENTS.map((segment) => {
        const selected = segment.value === value;
        return (
          <button
            key={segment.value}
            type="button"
            aria-pressed={selected}
            onClick={() => onChange(segment.value)}
            className={`rounded-full px-2 py-0.5 text-[10px] font-bold uppercase transition duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-portalGreen ${
              selected ? selectedClass : idleClass
            }`}
          >
            {segment.label}
          </button>
        );
      })}
    </div>
  );
}
