import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTrashCan } from '@fortawesome/free-solid-svg-icons';
import type { GlossaryCategory } from '../data/mock-data';

const CATEGORY_CLASS: Record<GlossaryCategory, string> = {
  工具: 'bg-rickBlue',
  架构: 'bg-mortyYellow',
  系统: 'bg-portalGreen',
};

interface GlossaryRowProps {
  term: string;
  category: GlossaryCategory;
  /** Divider only where the category group changes — never between rows. */
  topDivider?: boolean;
  onDelete: () => void;
}

/**
 * GlossaryRow per UI-SPEC: 15px/600 term name, a colored category tag, and an
 * icon-only delete control (aria-labelled — icon-only actions must never be
 * unlabelled). Rows stay divider-free inside a category group.
 */
export default function GlossaryRow({
  term,
  category,
  topDivider = false,
  onDelete,
}: GlossaryRowProps) {
  return (
    <li
      className={`flex items-center justify-between gap-3 py-3 ${
        topDivider ? 'border-t-2 border-black' : ''
      }`}
    >
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-[15px] font-semibold text-white">{term}</span>
        <span
          className={`shrink-0 rounded border-2 border-black px-1.5 py-0.5 text-[12px] font-bold text-black ${CATEGORY_CLASS[category]}`}
        >
          {category}
        </span>
      </span>
      <button
        type="button"
        aria-label={`删除术语 ${term}`}
        onClick={onDelete}
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border-2 border-black bg-spaceDark text-gray-400 transition duration-150 hover:translate-y-1 hover:bg-red-500 hover:text-black hover:shadow-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen active:scale-[0.98] motion-reduce:transition-none"
      >
        <FontAwesomeIcon icon={faTrashCan} aria-hidden="true" />
      </button>
    </li>
  );
}
