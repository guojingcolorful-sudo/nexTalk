import type { ReactNode } from 'react';

export type ButtonVariant = 'green' | 'yellow' | 'blue' | 'red' | 'paper' | 'ghost';
export type ButtonSize = 'md' | 'sm';

/** Color-fill variants take black text (all four pass contrast with black);
 *  ghost sits on dark surfaces, so it takes white text. */
const VARIANT_CLASS: Record<ButtonVariant, string> = {
  green: 'bg-portalGreen text-black shadow-cartoon-black focus-visible:outline-black',
  yellow: 'bg-mortyYellow text-black shadow-cartoon-black focus-visible:outline-black',
  blue: 'bg-rickBlue text-black shadow-cartoon-black focus-visible:outline-black',
  red: 'bg-red-500 text-black shadow-cartoon-black focus-visible:outline-black',
  paper: 'bg-white text-black shadow-cartoon-black focus-visible:outline-black',
  ghost: 'bg-transparent text-white focus-visible:outline-portalGreen',
};

const SIZE_CLASS: Record<ButtonSize, string> = {
  md: 'px-4 py-3 text-sm',
  sm: 'px-3 py-2 text-xs',
};

const BASE_CLASS = [
  'inline-flex items-center gap-2 whitespace-nowrap rounded-xl border-4 border-black',
  'font-bold uppercase tracking-wider',
  // Motion Contract: press physics on every button (150ms ease-out).
  'transition duration-150 ease-out hover:translate-y-1 hover:shadow-none active:scale-[0.98]',
  'focus-visible:outline-2 focus-visible:outline-offset-2',
  // Disabled: 50% opacity, gray borders, no shadow, no press motion.
  'disabled:cursor-not-allowed disabled:border-gray-600 disabled:opacity-50 disabled:shadow-none',
  'disabled:hover:translate-y-0 disabled:active:scale-100',
].join(' ');

interface NeobrutalismButtonProps {
  children: ReactNode;
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Row layout (label left, status right) instead of centered content. */
  align?: 'center' | 'between';
  type?: 'button' | 'submit';
  onClick?: () => void;
  disabled?: boolean;
  /** Layout-matched loading state — the label swaps, no spinner (UI-SPEC
   *  Interaction States: "no generic spinners"). */
  loading?: boolean;
  loadingLabel?: string;
  /** Native tooltip — used to state the visible reason for a disabled CTA. */
  title?: string;
  ariaLabel?: string;
  className?: string;
}

/**
 * The single button primitive for the whole product (UI-SPEC Component
 * Inventory): 4px black border + hard offset shadow, press physics, visible
 * focus ring, and the full default/hover/focus/active/loading/disabled cycle.
 * Labels stay at four words or fewer and never wrap at desktop width.
 */
export default function NeobrutalismButton({
  children,
  variant = 'green',
  size = 'md',
  align = 'center',
  type = 'button',
  onClick,
  disabled = false,
  loading = false,
  loadingLabel,
  title,
  ariaLabel,
  className,
}: NeobrutalismButtonProps) {
  const isDisabled = disabled || loading;
  const classes = [
    BASE_CLASS,
    SIZE_CLASS[size],
    VARIANT_CLASS[variant],
    align === 'between' ? 'justify-between text-left' : 'justify-center',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <button
      type={type}
      onClick={onClick}
      disabled={isDisabled}
      title={title}
      aria-label={ariaLabel}
      aria-busy={loading || undefined}
      className={classes}
    >
      {loading && loadingLabel ? loadingLabel : children}
    </button>
  );
}
