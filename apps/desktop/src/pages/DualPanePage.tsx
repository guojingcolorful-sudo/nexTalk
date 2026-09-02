import HeaderBar from '../components/HeaderBar';

/**
 * Dual-pane page (860x680) — minimal stub so the hidden window renders its
 * rickBlue header bar (not blank) when shown. The live subtitles + AI panes
 * (DSK-02/DSK-04) land in 01-03.
 */
export default function DualPanePage() {
  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-blue">
      <HeaderBar tone="blue" title="扩展视图" />
      <main className="flex flex-1 items-center justify-center p-4">
        <p className="text-xs font-bold text-gray-400">双栏直播视图将在 01-03 实现</p>
      </main>
    </div>
  );
}
