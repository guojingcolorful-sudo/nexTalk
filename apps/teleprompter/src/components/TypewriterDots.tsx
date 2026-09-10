/**
 * TypewriterDots — stream-end indicator shown while a line is still being
 * recognized or translated. Decorative dots (aria-hidden) plus a role="status"
 * label so assistive tech learns a line is in flight without inventing copy.
 */
const DELAY_CLASS = ['', '[animation-delay:0.1s]', '[animation-delay:0.2s]'];

export default function TypewriterDots() {
  return (
    <span role="status" aria-label="正在生成" className="mt-2 flex items-center gap-1 self-start">
      <span aria-hidden="true" className="flex items-center gap-1">
        {DELAY_CLASS.map((delayClass) => (
          <span
            key={delayClass || 'first'}
            className={`h-2 w-2 rounded-full bg-portalGreen animate-bounce motion-reduce:animate-none ${delayClass}`}
          />
        ))}
      </span>
    </span>
  );
}
