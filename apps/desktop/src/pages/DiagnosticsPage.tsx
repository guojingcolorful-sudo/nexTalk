import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import HeaderBar from '../components/HeaderBar';
import LatencyWaterfall from '../components/LatencyWaterfall';
import { E2E_BUDGET_MS, useLatencyWaterfall } from '../hooks/useLatencyWaterfall';

/**
 * DiagnosticsPage（诊断面板）— 开发者观测面：D-13 的「完整分阶段明细归
 * 开发者观测」在 Phase 2 的落点。只读展示 Rust 产出的延迟瀑布与本月分阶段
 * 成本（T3.7），不做任何判定或重算——预算门禁在 `pipeline::budget` 与 CI
 * 的 rig 车道，费率表在 `trace::STT_USD_PER_MINUTE` 一族命名常量里；面板的
 * 职责是让人能看见超支发生在哪一段。
 *
 * 数据只有两个来源：桌面进程（live），或没有 IPC 桥时的类型化预览 fixture
 * （preview，页面会显式标注，绝不让假数字冒充实测）。成本面板没有预览
 * fixture——`usage_summary` 不可达或本月无轨迹时显示「暂无用量数据」，
 * 绝不编一个月度数字。链路健康面板（CR-01/WR-03）同样只读：三个丢弃计数
 * 由 Rust 给出（活动会话读写入器本身，停止后读快照），前端只渲染。
 */

interface UsageSummaryDto {
  sessions: number;
  segments: number;
  skippedLines: number;
  sttAudioMs: number;
  translatePromptTokens: number;
  translateCompletionTokens: number;
  ttsChars: number;
}

interface CostReportDto {
  sttUsd: number;
  translateUsd: number;
  ttsUsd: number;
  totalUsd: number;
  budgetUsd: number;
  overBudget: boolean;
}

interface LinkHealthDto {
  mirrorFailures: number;
  traceDroppedRecords: number;
  traceWriteFailures: number;
}

interface UsageReportDto {
  usage: UsageSummaryDto;
  cost: CostReportDto;
  quotaMinutes: number;
  usedMinutes: number;
  health: LinkHealthDto;
}

/** 金额一律四位小数：分段级差异（$0.0004/段）也要可见。 */
export function formatUsd(value: number): string {
  return `$${value.toFixed(4)}`;
}

/** 千分位整数——手写以避开 ICU 环境差异，输出确定。 */
export function formatCount(value: number): string {
  return String(Math.round(value)).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

function formatMinutes(ms: number): string {
  const minutes = ms / 60_000;
  const text = Number.isInteger(minutes) ? String(minutes) : minutes.toFixed(1);
  return `${text} 分钟`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** IPC 信任边界：只有完全符合形状的回包才进入面板。 */
export function isUsageReport(value: unknown): value is UsageReportDto {
  if (!isRecord(value) || !isRecord(value.usage) || !isRecord(value.cost)) {
    return false;
  }
  const { usage, cost } = value;
  return (
    typeof value.quotaMinutes === 'number' &&
    typeof value.usedMinutes === 'number' &&
    typeof usage.sessions === 'number' &&
    typeof usage.segments === 'number' &&
    typeof usage.skippedLines === 'number' &&
    typeof usage.sttAudioMs === 'number' &&
    typeof usage.translatePromptTokens === 'number' &&
    typeof usage.translateCompletionTokens === 'number' &&
    typeof usage.ttsChars === 'number' &&
    typeof cost.sttUsd === 'number' &&
    typeof cost.translateUsd === 'number' &&
    typeof cost.ttsUsd === 'number' &&
    typeof cost.totalUsd === 'number' &&
    typeof cost.budgetUsd === 'number' &&
    typeof cost.overBudget === 'boolean' &&
    isRecord(value.health) &&
    typeof value.health.mirrorFailures === 'number' &&
    typeof value.health.traceDroppedRecords === 'number' &&
    typeof value.health.traceWriteFailures === 'number'
  );
}

function useUsageReport(): UsageReportDto | null {
  const [report, setReport] = useState<UsageReportDto | null>(null);
  useEffect(() => {
    let cancelled = false;
    invoke('usage_summary')
      .then((payload) => {
        if (!cancelled && isUsageReport(payload)) setReport(payload);
      })
      .catch((err) => {
        // 无 IPC 桥（浏览器预览）：保持「暂无用量数据」，不编造数字。
        console.error('usage_summary failed', err);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return report;
}

interface CostRowProps {
  label: string;
  quantity: string;
  cost: string;
  total?: boolean;
}

function CostRow({ label, quantity, cost, total = false }: CostRowProps) {
  return (
    <div
      className={`flex items-center justify-between gap-3 rounded-xl border-4 border-black px-3 py-2 ${
        total ? 'bg-mortyYellow shadow-[4px_4px_0_0_#000]' : 'bg-panel'
      }`}
    >
      <span className={total ? 'text-sm font-black' : 'text-xs font-bold'}>{label}</span>
      <span className="ml-auto text-[10px] text-gray-500 tabular-nums">{quantity}</span>
      <span className={`tabular-nums ${total ? 'text-sm font-black' : 'text-xs font-bold'}`}>
        {cost}
      </span>
    </div>
  );
}

/** 分阶段成本明细：三段 + 合计，数字全部来自 Rust 的费率表。 */
function CostPanel({ report }: { report: UsageReportDto | null }) {
  const hasData = report !== null && report.usage.segments > 0;
  return (
    <section aria-label="成本明细" className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-xs font-black tracking-wider text-black">成本明细</h2>
        <div className="flex items-center gap-2">
          <span className="rounded-full border-2 border-black bg-panel px-2 py-0.5 text-[10px] font-bold text-black shadow-[2px_2px_0_0_#000]">
            估算
          </span>
          {report?.cost.overBudget ? (
            <span className="rounded-full border-2 border-black bg-red-500 px-2 py-0.5 text-[10px] font-bold text-white shadow-[2px_2px_0_0_#000]">
              超预算
            </span>
          ) : null}
        </div>
      </div>
      {hasData && report !== null ? (
        <div className="space-y-2">
          <CostRow
            label="STT 语音识别"
            quantity={formatMinutes(report.usage.sttAudioMs)}
            cost={formatUsd(report.cost.sttUsd)}
          />
          <CostRow
            label="翻译 (文字)"
            quantity={`${formatCount(
              report.usage.translatePromptTokens + report.usage.translateCompletionTokens,
            )} tokens`}
            cost={formatUsd(report.cost.translateUsd)}
          />
          <CostRow
            label="TTS 语音合成"
            quantity={`${formatCount(report.usage.ttsChars)} 字符`}
            cost={formatUsd(report.cost.ttsUsd)}
          />
          <CostRow
            label="合计"
            quantity={`预算 ${formatUsd(report.cost.budgetUsd)}`}
            cost={formatUsd(report.cost.totalUsd)}
            total
          />
        </div>
      ) : (
        <p className="rounded-xl border-4 border-black bg-panel p-4 text-xs text-gray-400">
          暂无用量数据
        </p>
      )}
    </section>
  );
}

/**
 * 链路健康（CR-01/WR-03）：三个丢弃计数的读者。全 0 是正常态；非 0 是
 * 「这一段声音/这一条轨迹真的丢了」的证据——面板只显示 Rust 报来的数字，
 * 不做任何补偿或重试。
 */
function HealthPanel({ report }: { report: UsageReportDto | null }) {
  const clean =
    report !== null &&
    report.health.mirrorFailures === 0 &&
    report.health.traceDroppedRecords === 0 &&
    report.health.traceWriteFailures === 0;
  const rows = report
    ? [
        {
          label: '回声参考缺口',
          hint: '播放参考被 AEC 拒收',
          value: report.health.mirrorFailures,
        },
        {
          label: '轨迹丢弃',
          hint: '队列满/关闭，未入队',
          value: report.health.traceDroppedRecords,
        },
        {
          label: '轨迹写入失败',
          hint: '入队后写盘失败',
          value: report.health.traceWriteFailures,
        },
      ]
    : [];
  return (
    <section aria-label="链路健康" className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-xs font-black tracking-wider text-black">链路健康</h2>
        {report !== null ? (
          <span
            className={`rounded-full border-2 border-black px-2 py-0.5 text-[10px] font-bold shadow-[2px_2px_0_0_#000] ${
              clean ? 'bg-panel text-black' : 'bg-red-500 text-white'
            }`}
          >
            {clean ? '无丢弃' : '有丢弃'}
          </span>
        ) : null}
      </div>
      {report !== null ? (
        <div className="space-y-2">
          {rows.map((row) => (
            <div
              key={row.label}
              className="flex items-center justify-between gap-3 rounded-xl border-4 border-black bg-panel px-3 py-2"
            >
              <span className="text-xs font-bold">{row.label}</span>
              <span className="ml-auto text-[10px] text-gray-500">{row.hint}</span>
              <span
                className={`tabular-nums text-xs font-bold ${
                  row.value > 0 ? 'text-red-600' : ''
                }`}
              >
                {formatCount(row.value)}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <p className="rounded-xl border-4 border-black bg-panel p-4 text-xs text-gray-400">
          暂无链路数据
        </p>
      )}
    </section>
  );
}

export default function DiagnosticsPage() {
  const navigate = useNavigate();
  const { report, source } = useLatencyWaterfall();
  const usageReport = useUsageReport();

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

        <CostPanel report={usageReport} />

        <HealthPanel report={usageReport} />
      </main>
    </div>
  );
}
