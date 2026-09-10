import type { Ref } from 'react';

interface FormFieldProps {
  id: string;
  /** Visible label above the input — never a placeholder standing in for it. */
  label: string;
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  helper?: string;
  error?: string;
  inputRef?: Ref<HTMLInputElement>;
  className?: string;
}

/**
 * FormField per UI-SPEC: 12px/700 uppercase label above the input, spaceDark
 * input with a 2px black border, optional helper below and red-500 error text
 * under that. The error is wired through aria-invalid + aria-describedby so
 * the field is announced as invalid, not just colored red.
 */
export default function FormField({
  id,
  label,
  value,
  onChange,
  placeholder,
  helper,
  error,
  inputRef,
  className = '',
}: FormFieldProps) {
  const helperId = helper !== undefined ? `${id}-helper` : undefined;
  const errorId = error !== undefined ? `${id}-error` : undefined;
  const describedBy = [helperId, errorId].filter((ref): ref is string => ref !== undefined);

  return (
    <div className={`flex flex-col gap-1 ${className}`}>
      <label htmlFor={id} className="text-[12px] font-bold uppercase tracking-wider text-gray-400">
        {label}
      </label>
      <input
        id={id}
        ref={inputRef}
        type="text"
        value={value}
        placeholder={placeholder}
        aria-invalid={error !== undefined ? true : undefined}
        aria-describedby={describedBy.length > 0 ? describedBy.join(' ') : undefined}
        onChange={(event) => onChange(event.target.value)}
        className="rounded-xl border-2 border-black bg-spaceDark px-3 py-2 text-sm font-semibold text-white transition placeholder:text-gray-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen"
      />
      {helperId !== undefined ? (
        <p id={helperId} className="text-[12px] text-gray-400">
          {helper}
        </p>
      ) : null}
      {errorId !== undefined ? (
        <p id={errorId} className="text-[12px] font-bold text-red-500">
          {error}
        </p>
      ) : null}
    </div>
  );
}
