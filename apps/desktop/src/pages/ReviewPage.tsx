import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { faRotate } from '@fortawesome/free-solid-svg-icons';
import EmptyState from '../components/EmptyState';
import HeaderBar from '../components/HeaderBar';
import NeobrutalismButton from '../components/NeobrutalismButton';
import ReviewReportSection from '../components/ReviewReportSection';
import Skeleton from '../components/Skeleton';
import { MOCK_REVIEW_REPORT, type MockReviewReport } from '../data/mock-data';

/** UI-only phase: 生成报告 simulates the Phase 6 generation pass. */
const GENERATE_DELAY_MS = 700;

/**
 * ReviewPage (复盘报告) — the report starts empty (暂无复盘报告) and 生成报告
 * runs the layout-matched skeleton before the mock ReviewReportSection lands.
 * Both states are reachable so the page is never a dead end.
 */
export default function ReviewPage() {
  const navigate = useNavigate();
  const [report, setReport] = useState<MockReviewReport | null>(null);
  const [generating, setGenerating] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const generate = () => {
    if (generating) return;
    setGenerating(true);
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setGenerating(false);
      setReport(MOCK_REVIEW_REPORT);
    }, GENERATE_DELAY_MS);
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green" title="复盘报告" onBack={() => navigate('/console')} />

      <main className="flex-1 overflow-y-auto p-4">
        <NeobrutalismButton
          variant="paper"
          size="sm"
          className="w-full"
          onClick={generate}
          disabled={generating}
        >
          {report === null ? '生成报告' : '重新生成'}
        </NeobrutalismButton>

        <div className="mt-4">
          {generating ? (
            <div className="space-y-3">
              <Skeleton className="h-8 w-1/2" />
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : report === null ? (
            <EmptyState
              icon={faRotate}
              title="暂无复盘报告"
              body="生成报告后，可查看 Action Items 与关键关注点"
            />
          ) : (
            <ReviewReportSection report={report} />
          )}
        </div>
      </main>
    </div>
  );
}
