import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  faSatelliteDish,
  faTriangleExclamation,
  faWifi,
} from '@fortawesome/free-solid-svg-icons';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';

/**
 * Connection states the capsule can announce. `reconnecting` is produced by
 * the hardened useWs (backoff ladder) — it carries the locked Copywriting
 * Contract line 正在自动重连 so the phone explains the wait instead of
 * silently freezing.
 */
export type CapsuleStatus = 'connecting' | 'connected' | 'reconnecting' | 'closed';

interface StatusCopy {
  label: string;
  icon: IconDefinition;
  labelClass: string;
}

const STATUS_COPY: Record<CapsuleStatus, StatusCopy> = {
  connecting: {
    label: '正在连接',
    icon: faSatelliteDish,
    labelClass: 'text-gray-300',
  },
  connected: {
    label: '实时同步中',
    icon: faWifi,
    labelClass: 'text-white',
  },
  reconnecting: {
    label: '正在自动重连',
    icon: faTriangleExclamation,
    labelClass: 'text-mortyYellow',
  },
  closed: {
    label: '连接已断开',
    icon: faTriangleExclamation,
    labelClass: 'text-red-500',
  },
};

interface StatusCapsuleProps {
  status: CapsuleStatus;
}

/**
 * StatusCapsule — the phone's single persistent status readout (UI-SPEC
 * Component Inventory): #1A1A22 pill, black border + hard shadow, red
 * breathing dot, 12px/700 label, trailing status icon. `role="status"` lets
 * assistive tech learn about a dropped link without moving focus.
 */
export default function StatusCapsule({ status }: StatusCapsuleProps) {
  const copy = STATUS_COPY[status];

  return (
    <span
      role="status"
      className="inline-flex items-center gap-2 rounded-full border-2 border-black bg-panel px-3 py-2 shadow-[2px_2px_0_0_#000]"
    >
      <span
        aria-hidden="true"
        className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-red-500 motion-reduce:animate-none"
      />
      <span className={`text-xs font-bold uppercase tracking-wider ${copy.labelClass}`}>
        {copy.label}
      </span>
      <FontAwesomeIcon
        icon={copy.icon}
        aria-hidden="true"
        className={`shrink-0 text-xs ${copy.labelClass}`}
      />
    </span>
  );
}
