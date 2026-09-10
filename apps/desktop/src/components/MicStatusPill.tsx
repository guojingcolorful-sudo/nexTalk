import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faMicrophone } from '@fortawesome/free-solid-svg-icons';

/**
 * MicStatusPill — the red pulsing capsule in the dual-pane header showing
 * that capture is live. Black text on red-500 per the colored-surface
 * contrast rule; the pulse collapses under prefers-reduced-motion.
 */
export default function MicStatusPill() {
  return (
    <span
      role="status"
      className="inline-flex shrink-0 animate-pulse items-center gap-1 rounded-full border-2 border-black bg-red-500 px-2 py-1 text-[10px] font-bold text-black shadow-[2px_2px_0_0_#000] motion-reduce:animate-none"
    >
      <FontAwesomeIcon icon={faMicrophone} aria-hidden="true" />
      麦克风开启-监听中
    </span>
  );
}
