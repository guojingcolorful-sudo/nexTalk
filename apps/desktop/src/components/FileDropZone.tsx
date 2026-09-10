import { useState, type DragEvent, type ReactNode } from 'react';

interface FileDropZoneProps {
  /** Fired by a drop and by the CTA inside the zone (Phase 5 parses for real). */
  onImport: () => void;
  /** The zone content — its CTA is the keyboard path to onImport. */
  children: ReactNode;
  className?: string;
}

/**
 * FileDropZone per UI-SPEC: dashed 4px black border, fill turns portalGreen
 * while a file is dragged over it.
 *
 * The drop target itself is pointer-only by nature, so the import CTA lives
 * inside the zone: every action here is also reachable by keyboard.
 */
export default function FileDropZone({ onImport, children, className = '' }: FileDropZoneProps) {
  const [dragging, setDragging] = useState(false);

  const over = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(true);
  };

  const leave = () => setDragging(false);

  const drop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(false);
    onImport();
  };

  return (
    <div
      data-testid="file-drop-zone"
      onDragOver={over}
      onDragEnter={over}
      onDragLeave={leave}
      onDrop={drop}
      className={`rounded-xl border-4 border-dashed p-4 text-center transition duration-150 motion-reduce:transition-none ${
        dragging ? 'border-black bg-portalGreen text-black' : 'border-black bg-spaceDark'
      } ${className}`}
    >
      {children}
      <p
        className={`mt-2 text-[12px] font-bold ${dragging ? 'text-black' : 'text-gray-400'}`}
        aria-live="polite"
      >
        {dragging ? '松手即导入' : '也可以把简历文件拖到这里'}
      </p>
    </div>
  );
}
