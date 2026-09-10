import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { faBook } from '@fortawesome/free-solid-svg-icons';
import ConfirmModal from '../components/ConfirmModal';
import EmptyState from '../components/EmptyState';
import FormField from '../components/FormField';
import GlossaryRow from '../components/GlossaryRow';
import HeaderBar from '../components/HeaderBar';
import NeobrutalismButton from '../components/NeobrutalismButton';
import Skeleton from '../components/Skeleton';
import { MOCK_BADGE_LABEL, MOCK_GLOSSARY_TERMS, type MockGlossaryTerm } from '../data/mock-data';

/** UI-only phase: the list renders local state; Phase 4 persists it and wires
 *  the real term protection into translation. */
const LOAD_DELAY_MS = 300;

/**
 * GlossaryPage (术语表) — FormField + 添加术语 above a GlossaryRow list with
 * group-sparse dividers. Deleting goes through ConfirmModal with the locked
 * destructive copy; an empty list offers the 添加术语 CTA.
 */
export default function GlossaryPage() {
  const navigate = useNavigate();
  const [terms, setTerms] = useState<MockGlossaryTerm[]>(() => [...MOCK_GLOSSARY_TERMS]);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string | undefined>(undefined);
  const [pendingDelete, setPendingDelete] = useState<MockGlossaryTerm | null>(null);
  const [loading, setLoading] = useState(true);
  const inputRef = useRef<HTMLInputElement>(null);
  const idSeqRef = useRef(0);

  useEffect(() => {
    const id = window.setTimeout(() => setLoading(false), LOAD_DELAY_MS);
    return () => window.clearTimeout(id);
  }, []);

  const addTerm = () => {
    const value = draft.trim();
    if (value.length === 0) {
      setError('请输入术语名称');
      return;
    }
    if (terms.some((term) => term.term === value)) {
      setError('该术语已在术语表中');
      return;
    }
    idSeqRef.current += 1;
    // Phase 4 derives the category from usage; new terms start in 工具.
    setTerms((previous) => [
      ...previous,
      { id: `term-local-${idSeqRef.current}`, term: value, category: '工具' },
    ]);
    setDraft('');
    setError(undefined);
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    addTerm();
  };

  const confirmDelete = () => {
    if (pendingDelete === null) return;
    setTerms((previous) => previous.filter((term) => term.id !== pendingDelete.id));
    setPendingDelete(null);
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar
        tone="green"
        title="术语表"
        onBack={() => navigate('/console')}
        actions={
          <span className="shrink-0 rounded border-2 border-black bg-mortyYellow px-2 py-0.5 text-[10px] font-bold text-black">
            {MOCK_BADGE_LABEL}
          </span>
        }
      />

      <main className="flex-1 overflow-y-auto p-4">
        <form onSubmit={submit} className="space-y-3">
          <FormField
            id="glossary-term"
            label="术语名称"
            value={draft}
            onChange={(next) => {
              setDraft(next);
              if (error !== undefined) setError(undefined);
            }}
            inputRef={inputRef}
            error={error}
            helper="翻译时保持原样的专有名词，例如 K8s、幂等性"
          />
          <NeobrutalismButton type="submit" size="sm" className="w-full">
            添加术语
          </NeobrutalismButton>
        </form>

        <section aria-label="术语列表" className="mt-4">
          {loading ? (
            <ul className="space-y-3">
              <li>
                <Skeleton className="h-8 w-full" />
              </li>
              <li>
                <Skeleton className="h-8 w-2/3" />
              </li>
              <li>
                <Skeleton className="h-8 w-5/6" />
              </li>
            </ul>
          ) : terms.length === 0 ? (
            <EmptyState
              icon={faBook}
              title="术语表为空"
              body="添加专有名词（如 K8s、幂等性），翻译时将保持原样"
              action={
                <NeobrutalismButton size="sm" onClick={() => inputRef.current?.focus()}>
                  添加术语
                </NeobrutalismButton>
              }
            />
          ) : (
            <ul className="rounded-xl border-4 border-black bg-darkerSpace px-3">
              {terms.map((term, index) => {
                const previousCategory = index > 0 ? terms[index - 1]?.category : undefined;
                return (
                  <GlossaryRow
                    key={term.id}
                    term={term.term}
                    category={term.category}
                    topDivider={previousCategory !== undefined && previousCategory !== term.category}
                    onDelete={() => setPendingDelete(term)}
                  />
                );
              })}
            </ul>
          )}
        </section>
      </main>

      <ConfirmModal
        open={pendingDelete !== null}
        title={`删除术语「${pendingDelete?.term ?? ''}」？`}
        body="该术语将不再受保护"
        confirmLabel="删除"
        onCancel={() => setPendingDelete(null)}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
