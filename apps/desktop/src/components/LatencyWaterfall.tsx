import { useState } from 'react';
import { faStopwatch } from '@fortawesome/free-solid-svg-icons';
import EmptyState from './EmptyState';
import {
  STAGE_LABEL_ZH,
  STAGE_ORDER,
  type LatencyClassReport,
  type LatencyWaterfallSegment,
  type WaterfallReport,
} from '../hooks/useLatencyWaterfall';

/** 冷启动 / 热路径 —— 两类数字分别判定、分别展示，绝不合并成一个平均值。 */
type LatencyClass = 'cold' | 'warm';

const CLASS_ORDER: readonly LatencyClass[] = ['cold', 'warm'];

const CLASS_LABEL_ZH: Record<LatencyClass, string> = {
  cold: '冷启动',
  warm: '热路径',
};

interface LatencyWaterfallProps {
  /** null = 本次进程还没有收到任何 `latency` 事件（空态，不是 0ms）。 */
  report: WaterfallReport | null;
}

/** 阶段条宽度：占 e2e 的比例（向上取整）。重叠阶段可能比停表还长，封顶 100%。 */
export function stageShare(ms: number, e2eMs: number): number {
  if (e2eMs <= 0) return 0;
  return Math.min(100, Math.ceil((ms * 100) / e2eMs));
}

function classButtonLabel(report: WaterfallReport | null, key: LatencyClass): string {
  const latest = report?.[key].latest ?? null;
  return latest === null ? CLASS_LABEL_ZH[key] : `${CLASS_LABEL_ZH[key]} ${latest.e2eMs}ms`;
}

/**
 * LatencyWaterfall — 五阶段横向瀑布（UI-SPEC 设计语言：4px 黑边框、硬偏移
 * 阴影、三功能色；红=超支、蓝=系统/正常）。开发者观测面，只读展示 Rust 侧
 * 已判定的报告：超支微章给出归因阶段与超支毫秒数，冷/热切换把两套数字分别
 * 摆出来（分段按钮上各自带本类最近一段的 e2e，一眼可对比）。
 */
export default function LatencyWaterfall({ report }: LatencyWaterfallProps) {
  const [view, setView] = useState<LatencyClass>('cold');
  const selected: LatencyClassReport | null = report === null ? null : report[view];
  const latest: LatencyWaterfallSegment | null = selected?.latest ?? null;

  return (
    <section aria-label="延迟瀑布" className="rounded-xl border-4 border-black bg-panel p-4 shadow-cartoon-black">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-[13px] font-bold uppercase tracking-wider text-white">端到端瀑布</h2>
        <div role="group" aria-label="冷热切换" className="flex overflow-hidden rounded-full border-2 border-black">
          {CLASS_ORDER.map((key) => (
            <button
              key={key}
              type="button"
              aria-pressed={view === key}
              onClick={() => setView(key)}
              className={`px-3 py-1 text-[10px] font-bold transition focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white ${
                view === key ? 'bg-mortyYellow text-black' : 'bg-panel text-gray-300 hover:text-white'
              }`}
            >
              {classButtonLabel(report, key)}
            </button>
          ))}
        </div>
      </div>

      {latest === null || selected === null ? (
        <EmptyState
          icon={faStopwatch}
          title="暂无测量数据"
          body="会话开始后，每段语音的五个流式边界会在这里展开"
          tone="blue"
          className="mt-3"
        />
      ) : (
        <>
          <div className="mt-3 flex items-end justify-between gap-3">
            <div>
              <p className="text-[10px] uppercase tracking-wider text-gray-400">本段端到端</p>
              <output
                aria-label="本段端到端"
                className={`text-4xl font-bold leading-none ${
                  latest.verdict.kind === 'overBudget' ? 'text-red-500' : 'text-white'
                }`}
              >
                {`${latest.e2eMs}ms`}
              </output>
            </div>
            <ul aria-label="分位" className="text-right text-[10px] tabular-nums text-gray-400">
              <li>{`p50 ${selected.e2e.p50Ms}ms`}</li>
              <li>{`p95 ${selected.e2e.p95Ms}ms`}</li>
              <li>{`${selected.segments} 段 · 超支 ${selected.overBudgetSegments}`}</li>
            </ul>
          </div>

          {latest.verdict.kind === 'overBudget' ? (
            <p className="mt-3 inline-flex rounded-full border-2 border-black bg-red-500 px-2 py-1 text-[10px] font-bold text-black shadow-[2px_2px_0_0_#000]">
              {`超支：${STAGE_LABEL_ZH[latest.verdict.stage]} +${latest.verdict.overByMs}ms`}
            </p>
          ) : null}

          {latest.overlapMs > 0 ? (
            <p className="mt-2 text-[10px] text-gray-400">
              {`朴素串行和 ${latest.serialSumMs}ms，实测重叠 ${latest.overlapMs}ms——重叠是测出来的，不是假设的`}
            </p>
          ) : null}

          <ul aria-label="阶段耗时" className="mt-3 space-y-2">
            {STAGE_ORDER.map((stage) => {
              const ms = latest.stageMs[stage];
              const share = stageShare(ms, latest.e2eMs);
              const blamed = latest.verdict.kind === 'overBudget' && latest.verdict.stage === stage;
              return (
                <li key={stage}>
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="text-[11px] font-bold text-white">{STAGE_LABEL_ZH[stage]}</span>
                    <span className="text-[11px] tabular-nums text-gray-300">{`${ms}ms · ${share}%`}</span>
                  </div>
                  <div className="mt-1 h-2 w-full overflow-hidden rounded-full border-2 border-black bg-darkerSpace">
                    <div
                      aria-hidden="true"
                      className={`h-full ${blamed ? 'bg-red-500' : 'bg-rickBlue'}`}
                      style={{ width: `${share}%` }}
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}
