/**
 * QuestionCard — the interviewer's question as recorded in the AI 辅助 tab
 * (UAT-16): the phone mirrors the desktop's AI timeline, so each question is
 * logged in real time next to the strategy that answers it. The record shows
 * the complete bilingual lines — instant, never typed (a record is read, not
 * performed).
 */
export default function QuestionCard({ zh, en }: { zh?: string; en?: string }) {
  const zhText = zh !== undefined && zh.trim().length > 0 ? zh : undefined;
  const enText = en !== undefined && en.trim().length > 0 ? en : undefined;
  if (zhText === undefined && enText === undefined) return null;

  return (
    <article className="w-full rounded-xl border-2 border-black bg-slate-800 p-3 shadow-cartoon-black">
      <p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-gray-400">
        面试官提问
      </p>
      {enText !== undefined ? (
        <p className="text-sm font-bold leading-normal text-white">{enText}</p>
      ) : null}
      {zhText !== undefined ? (
        <p className="mt-1 text-xs font-semibold leading-normal text-rickBlue">{zhText}</p>
      ) : null}
    </article>
  );
}
