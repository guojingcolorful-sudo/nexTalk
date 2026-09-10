interface SkeletonProps {
  /** Layout-matched dimensions, e.g. "h-4 w-32" — always mirror the real
   *  content so loading causes no layout shift (UI-SPEC Layout stability). */
  className?: string;
}

/**
 * Static loading block (UI-SPEC Component Inventory): spaceDark fill + 2px
 * black border at 50% opacity. Deliberately no shimmer — the Interaction
 * States contract forbids generic spinners, and a still block communicates
 * "content pending" without pulling attention.
 */
export default function Skeleton({ className }: SkeletonProps) {
  const classes = [
    'rounded-lg border-2 border-black bg-spaceDark opacity-50',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return <div aria-hidden="true" className={classes} />;
}
