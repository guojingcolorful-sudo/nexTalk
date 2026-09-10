import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { faHeadphones } from '@fortawesome/free-solid-svg-icons';
import ConfirmModal from '../components/ConfirmModal';
import EmptyState from '../components/EmptyState';
import HeaderBar from '../components/HeaderBar';
import RecordingAssetCard from '../components/RecordingAssetCard';
import Skeleton from '../components/Skeleton';
import { MOCK_RECORDINGS, type MockRecording } from '../data/mock-data';

/** UI-only phase: the list renders local state; Phase 6 records and persists. */
const LOAD_DELAY_MS = 300;

/**
 * RecordingsPage (录音资产) — the dual-track recordings the console lists
 * under 本地资产. Deleting goes through ConfirmModal with the locked
 * destructive copy (unrecoverable delete), and an empty list explains when
 * recordings appear.
 */
export default function RecordingsPage() {
  const navigate = useNavigate();
  const [recordings, setRecordings] = useState<MockRecording[]>(() => [...MOCK_RECORDINGS]);
  const [pendingDelete, setPendingDelete] = useState<MockRecording | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const id = window.setTimeout(() => setLoading(false), LOAD_DELAY_MS);
    return () => window.clearTimeout(id);
  }, []);

  const confirmDelete = () => {
    if (pendingDelete === null) return;
    setRecordings((previous) => previous.filter((item) => item.id !== pendingDelete.id));
    setPendingDelete(null);
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green" title="录音资产" onBack={() => navigate('/console')} />

      <main className="flex-1 overflow-y-auto p-4">
        <section aria-label="录音列表" className="space-y-3">
          {loading ? (
            <>
              <Skeleton className="h-24 w-full" />
              <Skeleton className="h-6 w-1/2" />
            </>
          ) : recordings.length === 0 ? (
            <EmptyState
              icon={faHeadphones}
              title="暂无录音"
              body="会话结束后，双轨录音会出现在这里"
            />
          ) : (
            recordings.map((recording) => (
              <RecordingAssetCard
                key={recording.id}
                recording={recording}
                onDelete={() => setPendingDelete(recording)}
              />
            ))
          )}
        </section>
      </main>

      <ConfirmModal
        open={pendingDelete !== null}
        title="删除录音？"
        body="该会话的录音将被永久删除，不可恢复"
        confirmLabel="删除"
        onCancel={() => setPendingDelete(null)}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
