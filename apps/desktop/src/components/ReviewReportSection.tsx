import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faRotate } from '@fortawesome/free-solid-svg-icons';
import { MOCK_BADGE_LABEL, type MockReviewReport } from '../data/mock-data';

const SENTIMENT_CLASS: Record<MockReviewReport['sentiment']['tone'], string> = {
  green: 'bg-portalGreen',
  yellow: 'bg-mortyYellow',
};

/**
 * ReviewReportSection per UI-SPEC: sentiment pill, Action Items as a numbered
 * portalGreen list, the key concerns, and per-question replay rows. Replay
 * itself is Phase 6, so the rows render disabled rather than looking live.
 */
export default function ReviewReportSection({ report }: { report: MockReviewReport }) {
  return (
    <section
      aria-label="复盘报告内容"
      className="space-y-4 rounded-xl border-4 border-black bg-spaceDark p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <span
          className={`rounded-full border-2 border-black px-2 py-0.5 text-[10px] font-bold text-black ${SENTIMENT_CLASS[report.sentiment.tone]}`}
        >
          {report.sentiment.label}
        </span>
        <span className="rounded border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold text-black">
          {MOCK_BADGE_LABEL}
        </span>
      </div>

      <div>
        <h3 className="mb-2 text-[10px] font-bold uppercase tracking-wider text-gray-400">
          Action Items
        </h3>
        <ol className="space-y-2">
          {report.actionItems.map((item, index) => (
            <li key={item} className="flex gap-2">
              <span
                aria-hidden="true"
                className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-portalGreen text-[10px] font-bold text-black"
              >
                {index + 1}
              </span>
              <span className="text-[13px] leading-relaxed text-gray-300">{item}</span>
            </li>
          ))}
        </ol>
      </div>

      <div>
        <h3 className="mb-2 text-[10px] font-bold uppercase tracking-wider text-gray-400">
          关键关注点
        </h3>
        <ul className="space-y-2">
          {report.concerns.map((concern) => (
            <li key={concern} className="flex gap-2 text-[13px] leading-relaxed text-gray-300">
              <span aria-hidden="true" className="text-mortyYellow">
                ·
              </span>
              <span>{concern}</span>
            </li>
          ))}
        </ul>
      </div>

      <div>
        <h3 className="mb-2 text-[10px] font-bold uppercase tracking-wider text-gray-400">
          逐题回放
        </h3>
        <ul className="space-y-2">
          {report.replays.map((replay) => (
            <li key={replay.id}>
              <button
                type="button"
                disabled
                className="flex w-full cursor-not-allowed items-center gap-2 rounded-xl border-2 border-black bg-darkerSpace p-2 text-left opacity-50"
              >
                <FontAwesomeIcon icon={faRotate} aria-hidden="true" className="text-gray-400" />
                <span className="min-w-0 flex-1 truncate text-[13px] text-white">
                  {replay.question}
                </span>
                <span className="shrink-0 text-[12px] text-gray-400">{replay.at}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
