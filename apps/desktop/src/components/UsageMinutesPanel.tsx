import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/**
 * UsageMinutesPanel（用量面板）— 用户视角的月度额度读数（T3.9 / D-15）。
 * 只显示分钟数（已用/剩余）：分阶段成本明细留在 /diagnostics（D-13 双面板
 * 分层），这里绝不搬 tokens/字符数。数据只有一个来源——Rust 的
 * `usage_summary` 命令聚合的本地轨迹（纯本地，不上传）。没有 IPC 桥时
 * （浏览器预览/jsdom）退回类型化 fixture 并显式标注「预览数据」，与延迟
 * 瀑布同一套习惯：真实运行时绝不显示假读数。
 *
 * 提示式，不阻断（D-15/GOV-17）：接近额度只给提示条，面板照常可用；硬闸门
 * 属 Phase 8 与定价联动，不在这一层。
 */

/** 接近额度的提示阈值（8 成）——提示式，非硬闸门。 */
export const QUOTA_HINT_RATIO = 0.8;

export const QUOTA_HINT_TEXT = '用量已接近本月额度，请留意';
export const EMPTY_USAGE_TEXT = '暂无用量数据';
export const PROVENANCE_TEXT = '数据来自本地记录，不上传云端';
export const PREVIEW_BADGE_TEXT = '预览数据：未连接桌面测量装置';

/** 面板只关心用户视角的三个数：额度、已用、是否有任何溯源记录。 */
export interface UsageMinutesReport {
  quotaMinutes: number;
  usedMinutes: number;
  segments: number;
}

/** 浏览器预览 fixture：仅在没有 IPC 桥时使用，且必定带「预览数据」标注。 */
export const PREVIEW_USAGE_REPORT: UsageMinutesReport = {
  quotaMinutes: 600,
  usedMinutes: 96,
  segments: 8,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function nonNegative(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

/** IPC 信任边界：形状不对的回报整条丢弃（沿用 01/02 的收窄习惯）。 */
export function narrowUsageMinutesReport(value: unknown): UsageMinutesReport | null {
  if (!isRecord(value) || !isRecord(value.usage)) return null;
  const quotaMinutes = nonNegative(value.quotaMinutes);
  const usedMinutes = nonNegative(value.usedMinutes);
  const segments = nonNegative(value.usage.segments);
  if (quotaMinutes === null || usedMinutes === null || segments === null) return null;
  return { quotaMinutes, usedMinutes, segments };
}

export default function UsageMinutesPanel() {
  const [report, setReport] = useState<UsageMinutesReport | null>(null);
  const [preview, setPreview] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke('usage_summary')
      .then((payload) => {
        const next = narrowUsageMinutesReport(payload);
        if (!cancelled && next !== null) setReport(next);
      })
      .catch(() => {
        // 无 IPC 桥（浏览器预览）：显式标注 + 类型化 fixture，不崩溃。
        if (!cancelled) {
          setPreview(true);
          setReport(PREVIEW_USAGE_REPORT);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const hasData = report !== null && report.segments > 0;
  const remainingMinutes =
    report === null ? 0 : Math.max(0, report.quotaMinutes - report.usedMinutes);
  const nearQuota =
    report !== null &&
    hasData &&
    report.quotaMinutes > 0 &&
    report.usedMinutes / report.quotaMinutes >= QUOTA_HINT_RATIO;

  return (
    <section
      aria-label="用量"
      className="space-y-2 rounded-xl border-4 border-black bg-spaceDark p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-xs font-black tracking-wider text-white">用量</h2>
        {preview ? (
          <span className="rounded-full border-2 border-black bg-mortyYellow px-2 py-0.5 text-[10px] font-bold text-black">
            {PREVIEW_BADGE_TEXT}
          </span>
        ) : null}
      </div>

      {hasData && report !== null ? (
        <p className="rounded-lg border-2 border-gray-700 bg-darkerSpace px-3 py-2 text-[13px] font-bold text-white tabular-nums">
          {`本月已用 ${report.usedMinutes} 分钟 / 剩余 ${remainingMinutes} 分钟`}
        </p>
      ) : (
        <p className="rounded-lg border-2 border-gray-700 bg-darkerSpace px-3 py-2 text-xs text-gray-400">
          {EMPTY_USAGE_TEXT}
        </p>
      )}

      {nearQuota ? (
        <p
          role="status"
          className="rounded-lg border-2 border-black bg-mortyYellow px-3 py-1.5 text-[11px] font-bold text-black"
        >
          {QUOTA_HINT_TEXT}
        </p>
      ) : null}

      <p className="text-[10px] text-gray-500">{PROVENANCE_TEXT}</p>
    </section>
  );
}
