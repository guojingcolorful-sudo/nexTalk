import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faCheck, faFileImport, faFilePdf } from '@fortawesome/free-solid-svg-icons';
import ConfirmModal from '../components/ConfirmModal';
import EmptyState from '../components/EmptyState';
import FileDropZone from '../components/FileDropZone';
import HeaderBar from '../components/HeaderBar';
import NeobrutalismButton from '../components/NeobrutalismButton';
import Skeleton from '../components/Skeleton';
import { MOCK_BADGE_LABEL, MOCK_RESUME, type MockResume } from '../data/mock-data';

/** UI-only phase: the drop/CTA simulates indexing (Phase 5 parses for real). */
const PARSE_DELAY_MS = 400;

/**
 * ResumeImportPage (简历导入) — FileDropZone with a drag-over fill, the parsed
 * file row with its portalGreen success state, removal behind ConfirmModal,
 * and the 尚未导入简历 empty state before anything is imported.
 */
export default function ResumeImportPage() {
  const navigate = useNavigate();
  const [resume, setResume] = useState<MockResume | null>(null);
  const [parsing, setParsing] = useState(false);
  const [pendingRemove, setPendingRemove] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const importResume = () => {
    if (parsing) return;
    setParsing(true);
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setParsing(false);
      setResume(MOCK_RESUME);
    }, PARSE_DELAY_MS);
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green" title="简历导入" onBack={() => navigate('/console')} />

      <main className="flex-1 overflow-y-auto p-4">
        {resume === null ? (
          <FileDropZone onImport={importResume}>
            {parsing ? (
              <div className="space-y-2">
                <Skeleton className="mx-auto h-10 w-10" />
                <Skeleton className="mx-auto h-4 w-2/3" />
                <Skeleton className="mx-auto h-4 w-1/2" />
              </div>
            ) : (
              <EmptyState
                icon={faFileImport}
                title="尚未导入简历"
                body="导入 PDF 或 Word 简历，AI 策略将基于真实经历生成"
                action={
                  <NeobrutalismButton size="sm" onClick={importResume}>
                    导入简历
                  </NeobrutalismButton>
                }
              />
            )}
          </FileDropZone>
        ) : (
          <section
            aria-label="已导入简历"
            className="rounded-xl border-4 border-black bg-spaceDark p-3"
          >
            <div className="flex items-center gap-3">
              <span
                aria-hidden="true"
                className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border-2 border-black bg-mortyYellow text-black"
              >
                <FontAwesomeIcon icon={faFilePdf} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[15px] font-semibold text-white">
                  {resume.fileName}
                </span>
                <span className="block text-[12px] text-gray-400">{resume.sizeLabel}</span>
              </span>
              <span className="flex shrink-0 items-center gap-1 text-[12px] font-bold text-portalGreen">
                <FontAwesomeIcon icon={faCheck} aria-hidden="true" />
                已就绪
              </span>
            </div>

            <div className="mt-3 flex items-center justify-between gap-2">
              <span className="text-[10px] font-bold uppercase tracking-wider text-gray-400">
                解析要点
              </span>
              <span className="rounded border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold text-black">
                {MOCK_BADGE_LABEL}
              </span>
            </div>
            <ul className="mt-2 space-y-1">
              {resume.highlights.map((highlight) => (
                <li key={highlight} className="text-[13px] text-gray-300">
                  {highlight}
                </li>
              ))}
            </ul>

            <NeobrutalismButton
              variant="ghost"
              size="sm"
              className="mt-3 w-full"
              onClick={() => setPendingRemove(true)}
            >
              移除
            </NeobrutalismButton>
          </section>
        )}
      </main>

      <ConfirmModal
        open={pendingRemove}
        title="移除简历？"
        body="AI 策略将不再参考该简历"
        confirmLabel="移除"
        onCancel={() => setPendingRemove(false)}
        onConfirm={() => {
          setResume(null);
          setPendingRemove(false);
        }}
      />
    </div>
  );
}
