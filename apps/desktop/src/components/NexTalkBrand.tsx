import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faBolt } from '@fortawesome/free-solid-svg-icons';

interface NexTalkBrandProps {
  /** Color of the highlighted X in the wordmark. Defaults to portalGreen
   *  (spec §0); pass 'ink' on colored surfaces where the contrast contract
   *  mandates black text (e.g. the portalGreen console header). */
  xColor?: 'green' | 'ink';
  /** Optional Chinese name subtitle (极言). */
  subtitle?: string;
}

/**
 * NexTalk brand per UI-SPEC Component Inventory:
 * NEXTALK uppercase Space Grotesk Bold, X highlighted, super-symbol fa-bolt
 * on a black-bordered tile, optional 极言 subtitle.
 */
export default function NexTalkBrand({ xColor = 'green', subtitle }: NexTalkBrandProps) {
  const xClass = xColor === 'green' ? 'text-portalGreen' : 'text-black';
  return (
    <div className="flex items-center gap-2">
      <span
        aria-hidden="true"
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border-2 border-black bg-white text-black"
      >
        <FontAwesomeIcon icon={faBolt} size="sm" />
      </span>
      <div className="leading-none">
        <p className="text-lg font-bold uppercase tracking-wider">
          NE<span className={xClass}>X</span>TALK
        </p>
        {subtitle ? <p className="mt-1 text-[10px] font-bold tracking-wider opacity-80">{subtitle}</p> : null}
      </div>
    </div>
  );
}
