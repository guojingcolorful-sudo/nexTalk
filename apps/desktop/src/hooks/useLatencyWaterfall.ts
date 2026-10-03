import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * 延迟瀑布诊断数据 —— 与 Rust `pipeline::budget` 的 serde 形状一一对应
 * (camelCase)。面板只读，不在前端重算任何判定：预算判定与冷热分离都由 Rust
 * 侧产出，前端只做展示，避免出现「前端算出一套、后端又算一套」的双口径。
 */

/** 五个流式 TTFB 边界（顺序即瀑布顺序）。 */
export type LatencyStage =
  | 'micCallback'
  | 'sttFirstPartial'
  | 'translateFirstToken'
  | 'ttsFirstAudio'
  | 'playbackFirstSample';

export const STAGE_ORDER: readonly LatencyStage[] = [
  'micCallback',
  'sttFirstPartial',
  'translateFirstToken',
  'ttsFirstAudio',
  'playbackFirstSample',
];

export const STAGE_LABEL_ZH: Record<LatencyStage, string> = {
  micCallback: '麦克风回调',
  sttFirstPartial: 'STT 首 partial',
  translateFirstToken: '翻译首 token',
  ttsFirstAudio: 'TTS 首音频',
  playbackFirstSample: '输出首帧',
};

/** 端到端预算（与 Rust `E2E_BUDGET_MS` 同值，AUDI-04）。 */
export const E2E_BUDGET_MS = 2000;

export interface LatencyPercentiles {
  p50Ms: number;
  p95Ms: number;
}

/** 超支即失败：`overBudget` 携带归因阶段与超支毫秒数（没有「仅告警」档）。 */
export type BudgetVerdict =
  | { kind: 'withinBudget' }
  | { kind: 'overBudget'; stage: LatencyStage; overByMs: number };

export interface LatencyWaterfallSegment {
  segmentId: number;
  cold: boolean;
  stageMs: Record<LatencyStage, number>;
  e2eMs: number;
  serialSumMs: number;
  overlapMs: number;
  verdict: BudgetVerdict;
}

export interface LatencyClassReport {
  segments: number;
  e2e: LatencyPercentiles;
  stages: Record<LatencyStage, LatencyPercentiles>;
  overBudgetSegments: number;
  latest: LatencyWaterfallSegment | null;
}

/** 冷启动与热路径两套数字并列，绝不跨类求平均（T-02-02）。 */
export interface WaterfallReport {
  cold: LatencyClassReport;
  warm: LatencyClassReport;
}

/** `live` = 桌面进程在推事件；`preview` = 没有 IPC 桥（浏览器预览/jsdom）。 */
export type LatencySource = 'live' | 'preview';

export interface LatencyWaterfallState {
  report: WaterfallReport | null;
  source: LatencySource;
}

// ------------------------------------------------------------- narrowing ---
// 事件总线是信任边界（与 01 的 isServerEvent 同一套习惯）：载荷先收窄再进
// React state，形状不对就整条丢弃，面板保持上一次的有效读数。

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function nonNegative(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

function isStage(value: unknown): value is LatencyStage {
  return typeof value === 'string' && (STAGE_ORDER as readonly string[]).includes(value);
}

function narrowPercentiles(value: unknown): LatencyPercentiles | null {
  if (!isRecord(value)) return null;
  const p50Ms = nonNegative(value.p50Ms);
  const p95Ms = nonNegative(value.p95Ms);
  return p50Ms === null || p95Ms === null ? null : { p50Ms, p95Ms };
}

/** 五个边界缺一不可——四个边界不是「大致上的一条瀑布」。 */
function narrowStageMs(value: unknown): Record<LatencyStage, number> | null {
  if (!isRecord(value)) return null;
  const stageMs = {} as Record<LatencyStage, number>;
  for (const stage of STAGE_ORDER) {
    const ms = nonNegative(value[stage]);
    if (ms === null) return null;
    stageMs[stage] = ms;
  }
  return stageMs;
}

function narrowVerdict(value: unknown): BudgetVerdict | null {
  if (!isRecord(value)) return null;
  if (value.kind === 'withinBudget') return { kind: 'withinBudget' };
  if (value.kind !== 'overBudget') return null;
  const overByMs = nonNegative(value.overByMs);
  if (overByMs === null || !isStage(value.stage)) return null;
  return { kind: 'overBudget', stage: value.stage, overByMs };
}

function narrowSegment(value: unknown): LatencyWaterfallSegment | null {
  if (!isRecord(value)) return null;
  const segmentId = nonNegative(value.segmentId);
  const e2eMs = nonNegative(value.e2eMs);
  const serialSumMs = nonNegative(value.serialSumMs);
  const overlapMs = nonNegative(value.overlapMs);
  const stageMs = narrowStageMs(value.stageMs);
  const verdict = narrowVerdict(value.verdict);
  if (
    typeof value.cold !== 'boolean' ||
    segmentId === null ||
    e2eMs === null ||
    serialSumMs === null ||
    overlapMs === null ||
    stageMs === null ||
    verdict === null
  ) {
    return null;
  }
  return { segmentId, cold: value.cold, stageMs, e2eMs, serialSumMs, overlapMs, verdict };
}

function narrowClassReport(value: unknown): LatencyClassReport | null {
  if (!isRecord(value)) return null;
  const segments = nonNegative(value.segments);
  const overBudgetSegments = nonNegative(value.overBudgetSegments);
  const e2e = narrowPercentiles(value.e2e);
  if (segments === null || overBudgetSegments === null || e2e === null || !isRecord(value.stages)) {
    return null;
  }

  const stages = {} as Record<LatencyStage, LatencyPercentiles>;
  for (const stage of STAGE_ORDER) {
    const percentiles = narrowPercentiles(value.stages[stage]);
    if (percentiles === null) return null;
    stages[stage] = percentiles;
  }

  let latest: LatencyWaterfallSegment | null = null;
  if (value.latest !== null && value.latest !== undefined) {
    latest = narrowSegment(value.latest);
    if (latest === null) return null;
  }

  return { segments, e2e, stages, overBudgetSegments, latest };
}

/** `latency` 事件的载荷守卫；形状不符返回 null（调用方丢弃该次更新）。 */
export function narrowWaterfallReport(payload: unknown): WaterfallReport | null {
  if (!isRecord(payload)) return null;
  const cold = narrowClassReport(payload.cold);
  const warm = narrowClassReport(payload.warm);
  return cold === null || warm === null ? null : { cold, warm };
}

// ------------------------------------------------------- preview fixture ---
// 仅在没有 IPC 桥时使用（纯浏览器预览/单测），让面板可独立审阅；真实运行时
// 永远显示「暂无测量数据」而不是假数字。脚本沿用 Rust rig 的五个片段。

interface PreviewScript {
  segmentId: number;
  cold: boolean;
  offsets: readonly [number, number, number, number, number];
}

const PREVIEW_SCRIPT: readonly PreviewScript[] = [
  { segmentId: 1, cold: true, offsets: [0, 260, 480, 900, 1180] },
  { segmentId: 2, cold: false, offsets: [0, 240, 470, 880, 1210] },
  { segmentId: 3, cold: false, offsets: [0, 250, 500, 910, 1250] },
  { segmentId: 4, cold: false, offsets: [0, 230, 520, 940, 1300] },
  { segmentId: 5, cold: false, offsets: [0, 255, 505, 925, 1225] },
];

function previewSegment(script: PreviewScript): LatencyWaterfallSegment {
  const stageMs = {} as Record<LatencyStage, number>;
  STAGE_ORDER.forEach((stage, index) => {
    stageMs[stage] = index === 0 ? 0 : script.offsets[index] - script.offsets[index - 1];
  });
  const e2eMs = script.offsets[script.offsets.length - 1];
  const serialSumMs = STAGE_ORDER.reduce((sum, stage) => sum + stageMs[stage], 0);
  const blamed = STAGE_ORDER.reduce((worst, stage) => (stageMs[stage] > stageMs[worst] ? stage : worst));
  return {
    segmentId: script.segmentId,
    cold: script.cold,
    stageMs,
    e2eMs,
    serialSumMs,
    overlapMs: Math.max(0, serialSumMs - e2eMs),
    verdict:
      e2eMs > E2E_BUDGET_MS
        ? { kind: 'overBudget', stage: blamed, overByMs: e2eMs - E2E_BUDGET_MS }
        : { kind: 'withinBudget' },
  };
}

/** 最近秩分位，与 Rust `nearest_rank` 同口径。 */
function nearestRank(values: readonly number[], percentile: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  const rank = Math.min(sorted.length, Math.max(1, Math.ceil((percentile * sorted.length) / 100)));
  return sorted[rank - 1];
}

function previewClass(segments: readonly LatencyWaterfallSegment[]): LatencyClassReport {
  const stages = {} as Record<LatencyStage, LatencyPercentiles>;
  for (const stage of STAGE_ORDER) {
    const values = segments.map((item) => item.stageMs[stage]);
    stages[stage] = { p50Ms: nearestRank(values, 50), p95Ms: nearestRank(values, 95) };
  }
  return {
    segments: segments.length,
    e2e: {
      p50Ms: nearestRank(segments.map((item) => item.e2eMs), 50),
      p95Ms: nearestRank(segments.map((item) => item.e2eMs), 95),
    },
    stages,
    overBudgetSegments: segments.filter((item) => item.verdict.kind === 'overBudget').length,
    latest: segments.length === 0 ? null : segments[segments.length - 1],
  };
}

function previewReport(): WaterfallReport {
  const all = PREVIEW_SCRIPT.map(previewSegment);
  return {
    cold: previewClass(all.filter((item) => item.cold)),
    warm: previewClass(all.filter((item) => !item.cold)),
  };
}

// ------------------------------------------------------------------ hook ---

/**
 * 订阅 Rust 的 `latency` 事件（02-02 起由级联阶段发出）。
 *
 * 事件载荷是信任边界，先过 [`narrowWaterfallReport`]（T-02-01/T-02-04）。
 * 没有 IPC 桥时（浏览器预览、jsdom）退回类型化 fixture 并把 `source` 标为
 * `preview`，面板据此显式标注「预览数据」——真实运行时绝不显示假测量值。
 */
export function useLatencyWaterfall(): LatencyWaterfallState {
  const [report, setReport] = useState<WaterfallReport | null>(null);
  const [source, setSource] = useState<LatencySource>('live');

  useEffect(() => {
    let alive = true;
    let unlisten: UnlistenFn | null = null;

    listen<unknown>('latency', (event) => {
      const next = narrowWaterfallReport(event.payload);
      if (next !== null) setReport(next);
    })
      .then((off) => {
        if (alive) unlisten = off;
        else void off();
      })
      .catch(() => {
        if (!alive) return;
        setSource('preview');
        setReport(previewReport());
      });

    return () => {
      alive = false;
      if (unlisten !== null) void unlisten();
    };
  }, []);

  return { report, source };
}
