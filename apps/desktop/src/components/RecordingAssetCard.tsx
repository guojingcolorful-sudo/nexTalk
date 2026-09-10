import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTrashCan } from '@fortawesome/free-solid-svg-icons';
import NeobrutalismButton from './NeobrutalismButton';
import {
  MOCK_BADGE_LABEL,
  RECORDING_EXPORT_FORMATS,
  type MockRecording,
} from '../data/mock-data';

interface RecordingAssetCardProps {
  recording: MockRecording;
  onDelete: () => void;
}

/**
 * RecordingAssetCard per UI-SPEC: session date + duration, the two track
 * badges (用户轨 portalGreen / 面试官轨 rickBlue), export buttons as ghost
 * buttons, and a delete control. Export and playback wiring is Phase 6, so
 * the export buttons ship disabled rather than pretending to work.
 */
export default function RecordingAssetCard({ recording, onDelete }: RecordingAssetCardProps) {
  return (
    <article className="rounded-xl border-4 border-black bg-spaceDark p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[13px] font-bold text-white">
          {recording.dateLabel} {recording.timeLabel}
        </span>
        <span className="shrink-0 text-[12px] text-gray-400">{recording.durationLabel}</span>
      </div>

      <div className="mt-2 flex flex-wrap gap-2">
        <span className="rounded-full border-2 border-black bg-portalGreen px-2 py-0.5 text-[10px] font-bold text-black">
          用户轨
        </span>
        <span className="rounded-full border-2 border-black bg-rickBlue px-2 py-0.5 text-[10px] font-bold text-black">
          面试官轨
        </span>
      </div>

      <div className="mt-3 flex gap-2">
        {RECORDING_EXPORT_FORMATS.map((format) => (
          <NeobrutalismButton key={format} variant="ghost" size="sm" disabled className="flex-1">
            {format}
          </NeobrutalismButton>
        ))}
      </div>

      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="rounded border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold text-black">
          {MOCK_BADGE_LABEL}
        </span>
        <button
          type="button"
          aria-label={`删除录音 ${recording.dateLabel} ${recording.timeLabel}`}
          onClick={onDelete}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border-2 border-black bg-darkerSpace text-gray-400 transition duration-150 hover:translate-y-1 hover:bg-red-500 hover:text-black hover:shadow-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-portalGreen active:scale-[0.98] motion-reduce:transition-none"
        >
          <FontAwesomeIcon icon={faTrashCan} aria-hidden="true" />
        </button>
      </div>
    </article>
  );
}
