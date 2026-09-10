import { useNavigate } from 'react-router-dom';
import HeaderBar from './HeaderBar';

interface PageStubProps {
  /** Page name in its locked Chinese form (术语表, 音色注册, ...). */
  title: string;
}

/**
 * Temporary scaffold for the six pages that 01-03 builds out in Tasks 3-4.
 *
 * It is NOT part of the UI-SPEC component inventory: it exists so the console
 * hub's navigation contract (every page reachable, every page back-navigable)
 * can be asserted end to end from the first task. Each real page replaces its
 * stub before this plan closes, so no surface ships as a placeholder.
 */
export default function PageStub({ title }: PageStubProps) {
  const navigate = useNavigate();

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green" title={title} onBack={() => navigate('/console')} />
      <main className="flex-1 overflow-y-auto p-4" data-testid="page-stub">
        <p className="text-[13px] font-bold text-white">{title}</p>
        <p className="mt-1 text-xs text-gray-400">页面骨架已就绪，完整界面将在本计划内实现。</p>
      </main>
    </div>
  );
}
