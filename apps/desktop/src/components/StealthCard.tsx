import { useEffect, useRef, useState } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faMask } from '@fortawesome/free-solid-svg-icons';

/** How long the window stays transparent after the hotkey preview (ms). */
const FLASH_MS = 500;

/**
 * StealthCard (UI-SPEC Component Inventory): rickBlue card with the 隐形模式
 * label, the plain-language description, and the Cmd + Shift + H keycap.
 *
 * Phase 1 note: clicking only plays the transitional opacity feedback
 * (Motion Contract: opacity → 0). The real orderOut hiding, the global
 * shortcut, and the accessory activation policy are Phase 4 — until then the
 * window is never actually hidden.
 */
export default function StealthCard() {
  const [flashing, setFlashing] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const preview = () => {
    setFlashing(true);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      setFlashing(false);
      timer.current = null;
    }, FLASH_MS);
  };

  const classes = [
    'w-full rounded-xl border-4 border-black bg-rickBlue p-4 text-left text-black shadow-cartoon-black',
    'transition ease-out hover:translate-y-1 hover:shadow-none active:scale-[0.98]',
    'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-black',
    // 0.1s during the stealth flash (Motion Contract), 150ms for press physics.
    flashing ? 'opacity-0 duration-100' : 'opacity-100 duration-150',
    'motion-reduce:transition-none',
  ]
    .filter(Boolean)
    .join(' ');

  // The visible label + description + keycap form the accessible name — no
  // aria-label, so assistive tech hears the whole affordance.
  return (
    <button type="button" onClick={preview} className={classes}>
      <span className="flex items-center gap-2 text-sm font-bold uppercase">
        <FontAwesomeIcon icon={faMask} aria-hidden="true" />
        隐形模式
      </span>
      <span className="mt-2 block text-xs font-semibold">投屏时一键隐藏全屏界面，防抓屏防录制。</span>
      <span className="mt-2 flex justify-center">
        <span className="rounded border-2 border-black bg-white px-2 py-1 text-xs font-bold">Cmd + Shift + H</span>
      </span>
    </button>
  );
}
