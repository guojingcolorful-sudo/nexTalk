import { useNavigate } from 'react-router-dom';
import HeaderBar from '../components/HeaderBar';
import LatencyWaterfall from '../components/LatencyWaterfall';
import { E2E_BUDGET_MS, useLatencyWaterfall } from '../hooks/useLatencyWaterfall';

/**
 * DiagnosticsPage（诊断面板）— 开发者观测面：D-13 的「完整分阶段明细归
 * 开发者观测」在 Phase 2 的落点。只读展示 Rust 产出的延迟瀑布，不做任何
 * 判定或重算——预算门禁在 `pipeline::budget` 与 CI 的 rig 车道，面板的职责
 * 是让人能看见超支发生在哪一段。
 *
 * 数据只有两个来源：桌面进程的 `latency` 事件（live），或没有 IPC 桥时的
 * 类型化预览 fixture（preview，页面会显式标注，绝不让假数字冒充实测）。
 */
export default function DiagnosticsPage() {
  const navigate = useNavigate();
  const { report, source } = useLatencyWaterfall();

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-blue">
      <HeaderBar tone="blue" title="诊断面板" onBack={() => navigate('/console')} />

      <main className="flex-1 space-y-3 overflow-y-auto p-4">
        <p className="text-[11px] text-gray-400">
          {`端到端预算 ≤${E2E_BUDGET_MS}ms（冷启动含握手成本）；超支即硬失败，并归因到具体阶段`}
        </p>

        {source === 'preview' ? (
          <p className="inline-flex rounded-full border-2 border-black bg-mortyYellow px-2 py-1 text-[10px] font-bold text-black shadow-[2px_2px_0_0_#000]">
            预览数据：未连接桌面测量装置
          </p>
        ) : null}

        <LatencyWaterfall report={report} />

        {/* 成本面板槽位：02-03 T3.7 在此挂载分阶段成本明细（本波不伪造数据）。 */}
      </main>
    </div>
  );
}
