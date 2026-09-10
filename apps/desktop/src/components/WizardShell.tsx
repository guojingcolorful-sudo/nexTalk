import type { ReactNode } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faCheck } from '@fortawesome/free-solid-svg-icons';
import HeaderBar from './HeaderBar';
import NeobrutalismButton from './NeobrutalismButton';

interface WizardShellProps {
  title: string;
  /** Step names in order; the numbered tiles read them out to assistive tech. */
  steps: readonly string[];
  /** Zero-based index of the active step. */
  current: number;
  onBack: () => void;
  /** Header badge (e.g. the SimSource 模拟模式 chip). */
  badge?: ReactNode;
  /** Omitted on the first step — 上一步 renders only when it can be taken. */
  onPrev?: () => void;
  /** The forward action for the current step (下一步 / 完成 / 开始录音). */
  actions: ReactNode;
  children: ReactNode;
}

/**
 * WizardShell per UI-SPEC: numbered black tiles (completed = portalGreen
 * fill, active = white, upcoming = dim), the step content area, and a footer
 * carrying 上一步 plus the step's forward action. Used by the setup wizard and
 * voice enrollment — both render inside the 340x680 console window.
 */
export default function WizardShell({
  title,
  steps,
  current,
  onBack,
  badge,
  onPrev,
  actions,
  children,
}: WizardShellProps) {
  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green" title={title} onBack={onBack} actions={badge} />

      <nav aria-label="步骤" className="shrink-0 border-b-4 border-black bg-spaceDark p-3">
        <ol className="flex items-center gap-2">
          {steps.map((step, index) => {
            const done = index < current;
            const active = index === current;
            return (
              <li
                key={step}
                aria-current={active ? 'step' : undefined}
                className={`flex items-center gap-2 ${index < steps.length - 1 ? 'flex-1' : ''}`}
              >
                <span
                  aria-hidden="true"
                  className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border-2 border-black text-xs font-bold transition duration-150 ${
                    done
                      ? 'bg-portalGreen text-black'
                      : active
                        ? 'bg-white text-black'
                        : 'bg-darkerSpace text-gray-400'
                  }`}
                >
                  {done ? <FontAwesomeIcon icon={faCheck} /> : index + 1}
                </span>
                <span className="sr-only">{`第 ${index + 1} 步 ${step}`}</span>
                {index < steps.length - 1 ? (
                  <span
                    aria-hidden="true"
                    className={`h-1 flex-1 rounded-full ${done ? 'bg-portalGreen' : 'bg-gray-700'}`}
                  />
                ) : null}
              </li>
            );
          })}
        </ol>
      </nav>

      <main className="flex-1 overflow-y-auto p-4">{children}</main>

      <footer className="flex shrink-0 items-center gap-2 border-t-4 border-black bg-spaceDark p-3">
        {onPrev !== undefined && current > 0 ? (
          <NeobrutalismButton variant="ghost" size="sm" onClick={onPrev}>
            上一步
          </NeobrutalismButton>
        ) : null}
        <div className="flex flex-1 justify-end">{actions}</div>
      </footer>
    </div>
  );
}
