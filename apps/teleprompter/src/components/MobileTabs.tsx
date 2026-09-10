import { useRef } from 'react';
import type { KeyboardEvent } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faBrain, faClosedCaptioning } from '@fortawesome/free-solid-svg-icons';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';

export type PhoneTab = 'subs' | 'ai';

interface TabSpec {
  id: PhoneTab;
  label: string;
  icon: IconDefinition;
  activeClass: string;
}

/** The phone's two reading surfaces (locked copy: 字幕 / AI 辅助). */
const TABS: readonly TabSpec[] = [
  {
    id: 'subs',
    label: '字幕',
    icon: faClosedCaptioning,
    // Active brand fill: green owns the user's own speech surface.
    activeClass: 'bg-portalGreen text-black shadow-[4px_4px_0_0_#000]',
  },
  {
    id: 'ai',
    label: 'AI 辅助',
    icon: faBrain,
    // Active brand fill: yellow owns every AI/strategy surface.
    activeClass: 'bg-mortyYellow text-black shadow-[4px_4px_0_0_#000]',
  },
];

interface MobileTabsProps {
  value: PhoneTab;
  onChange: (tab: PhoneTab) => void;
}

/**
 * MobileTabs — segmented 字幕 / AI 辅助 switch (UI-SPEC Component Inventory):
 * 40px touch targets, 4px black border, active tab filled with its brand color
 * plus a matching hard shadow. Follows the ARIA tabs pattern with roving
 * tabindex and Left/Right/Home/End keyboard handling.
 */
export default function MobileTabs({ value, onChange }: MobileTabsProps) {
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const activate = (index: number) => {
    const next = TABS[index];
    onChange(next.id);
    buttonRefs.current[index]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const last = TABS.length - 1;
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      activate(index === last ? 0 : index + 1);
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      activate(index === 0 ? last : index - 1);
    } else if (event.key === 'Home') {
      event.preventDefault();
      activate(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      activate(last);
    }
  };

  return (
    <div
      role="tablist"
      aria-label="手机提词视图"
      className="flex shrink-0 gap-2 border-b-4 border-black bg-panel px-3 py-2"
    >
      {TABS.map((tab, index) => {
        const selected = tab.id === value;
        return (
          <button
            key={tab.id}
            ref={(node) => {
              buttonRefs.current[index] = node;
            }}
            type="button"
            role="tab"
            id={`tab-${tab.id}`}
            aria-selected={selected}
            aria-controls={`panel-${tab.id}`}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(tab.id)}
            onKeyDown={(event) => onKeyDown(event, index)}
            className={[
              'inline-flex min-h-[40px] flex-1 items-center justify-center gap-2 rounded-xl border-4 border-black',
              'px-3 text-xs font-bold uppercase tracking-wider',
              'transition duration-150 ease-out focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen',
              selected
                ? `active:scale-[0.98] ${tab.activeClass}`
                : 'bg-spaceDark text-gray-400 hover:translate-y-0.5',
            ].join(' ')}
          >
            <FontAwesomeIcon icon={tab.icon} aria-hidden="true" />
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
