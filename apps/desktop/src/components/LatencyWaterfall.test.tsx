import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import LatencyWaterfall, { stageShare } from './LatencyWaterfall';
import {
  E2E_BUDGET_MS,
  STAGE_LABEL_ZH,
  STAGE_ORDER,
  useLatencyWaterfall,
  type LatencyClassReport,
  type LatencyStage,
  type LatencyWaterfallSegment,
  type WaterfallReport,
} from '../hooks/useLatencyWaterfall';

// Testing Library only self-registers cleanup when the runner injects globals
// (vitest globals are off here); without this a previous render leaks forward.
afterEach(cleanup);

// --------------------------------------------------------------- fixtures ---

/**
 * Scripted stage double, same script the Rust rig prints (tests/latency_rig.rs):
 * five boundary offsets → the five adjacent gaps plus the stopwatch. The panel
 * fixtures therefore read exactly like a real `latency` event payload.
 */
function segment(segmentId: number, cold: boolean, offsets: readonly number[]): LatencyWaterfallSegment {
  const stageMs = {} as Record<LatencyStage, number>;
  STAGE_ORDER.forEach((stage, index) => {
    stageMs[stage] = index === 0 ? 0 : offsets[index] - offsets[index - 1];
  });
  const e2eMs = offsets[offsets.length - 1];
  const serialSumMs = STAGE_ORDER.reduce((sum, stage) => sum + stageMs[stage], 0);
  const blamed = STAGE_ORDER.reduce((worst, stage) => (stageMs[stage] > stageMs[worst] ? stage : worst));
  return {
    segmentId,
    cold,
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

function stages(p50Ms: number, p95Ms: number): Record<LatencyStage, { p50Ms: number; p95Ms: number }> {
  return Object.fromEntries(STAGE_ORDER.map((stage) => [stage, { p50Ms, p95Ms }])) as Record<
    LatencyStage,
    { p50Ms: number; p95Ms: number }
  >;
}

function classReport(
  latest: LatencyWaterfallSegment,
  segments: number,
  p50Ms: number,
  p95Ms: number,
): LatencyClassReport {
  return {
    segments,
    e2e: { p50Ms, p95Ms },
    stages: stages(p50Ms, p95Ms),
    overBudgetSegments: latest.verdict.kind === 'overBudget' ? 1 : 0,
    latest,
  };
}

/** A class that has measured nothing yet — the panel's empty branch. */
const EMPTY_CLASS: LatencyClassReport = {
  segments: 0,
  e2e: { p50Ms: 0, p95Ms: 0 },
  stages: stages(0, 0),
  overBudgetSegments: 0,
  latest: null,
};

const COLD = segment(1, true, [0, 260, 480, 900, 1180]); // e2e 1180ms
const WARM = segment(5, false, [0, 255, 505, 925, 1225]); // e2e 1225ms
const REPORT: WaterfallReport = {
  cold: classReport(COLD, 1, 1180, 1180),
  warm: classReport(WARM, 4, 1225, 1300),
};

const OVER_BUDGET = segment(7, true, [0, 700, 1000, 1900, 2501]); // e2e 2501ms, TTS 900ms
const OVER_REPORT: WaterfallReport = {
  cold: classReport(OVER_BUDGET, 1, 2501, 2501),
  warm: classReport(WARM, 4, 1225, 1300),
};

const EMPTY_REPORT: WaterfallReport = { cold: EMPTY_CLASS, warm: EMPTY_CLASS };

/** The bar of a stage row is the only element carrying an inline width. */
function barWidth(row: HTMLElement): string {
  const bar = row.querySelector<HTMLElement>('[style]');
  return bar?.style.width ?? '';
}

// ------------------------------------------------------------- the panel ---

describe('LatencyWaterfall', () => {
  it('renders the five streaming boundaries with their share of the stopwatch, plus the e2e number', () => {
    render(<LatencyWaterfall report={REPORT} />);

    const rows = within(screen.getByRole('list', { name: '阶段耗时' })).getAllByRole('listitem');
    expect(rows).toHaveLength(5);
    for (const [index, stage] of STAGE_ORDER.entries()) {
      expect(rows[index].textContent).toContain(STAGE_LABEL_ZH[stage]);
    }

    // Adjacent-gap durations from the script [0, 260, 480, 900, 1180].
    expect(rows[1].textContent).toContain('260ms');
    expect(rows[2].textContent).toContain('220ms');
    expect(rows[3].textContent).toContain('420ms');
    expect(rows[4].textContent).toContain('280ms');
    // Bars are drawn as the share of the stopwatch (ceil, as the rig reports).
    expect(barWidth(rows[1])).toBe('23%');
    expect(barWidth(rows[3])).toBe('36%');

    expect(screen.getByRole('status', { name: '本段端到端' }).textContent).toBe('1180ms');
    const percentiles = within(screen.getByRole('list', { name: '分位' })).getAllByRole('listitem');
    expect(percentiles[0].textContent).toBe('p50 1180ms');
    expect(percentiles[1].textContent).toBe('p95 1180ms');
  });

  it('flags an over-budget segment with a red badge naming the blamed stage and the overage', () => {
    render(<LatencyWaterfall report={OVER_REPORT} />);

    const badge = screen.getByText('超支：TTS 首音频 +501ms');
    expect(badge.className).toContain('bg-red-500');
    // The e2e number itself is the off-budget one, so it reads red too.
    expect(screen.getByRole('status', { name: '本段端到端' }).className).toContain('text-red-500');
    // The blamed stage's bar is the one painted red.
    const rows = within(screen.getByRole('list', { name: '阶段耗时' })).getAllByRole('listitem');
    expect(rows[3].querySelector('[style]')?.className).toContain('bg-red-500');
    expect(rows[1].querySelector('[style]')?.className).toContain('bg-rickBlue');
  });

  it('keeps the cold and warm numbers apart and switches between them without overwriting either', () => {
    render(<LatencyWaterfall report={REPORT} />);
    const toggle = () => screen.getByRole('group', { name: '冷热切换' });
    const stopwatch = () => screen.getByRole('status', { name: '本段端到端' });

    // Both classes are on screen at once — neither number is averaged into one.
    expect(within(toggle()).getByRole('button', { name: /冷启动/ }).textContent).toContain('1180ms');
    expect(within(toggle()).getByRole('button', { name: /热路径/ }).textContent).toContain('1225ms');
    expect(within(toggle()).getByRole('button', { name: /冷启动/ }).getAttribute('aria-pressed')).toBe('true');
    expect(stopwatch().textContent).toBe('1180ms');

    fireEvent.click(within(toggle()).getByRole('button', { name: /热路径/ }));

    expect(stopwatch().textContent).toBe('1225ms');
    const percentileText = within(screen.getByRole('list', { name: '分位' }))
      .getAllByRole('listitem')
      .map((item) => item.textContent);
    expect(percentileText).toContain('p95 1300ms');

    fireEvent.click(within(toggle()).getByRole('button', { name: /冷启动/ }));

    expect(stopwatch().textContent).toBe('1180ms');
  });

  it('renders the empty state instead of a blank panel when there is nothing measured yet', () => {
    const { unmount } = render(<LatencyWaterfall report={null} />);
    expect(screen.getByText('暂无测量数据')).toBeTruthy();
    unmount();

    // A report whose classes hold no segments is empty too — not a 0ms claim.
    render(<LatencyWaterfall report={EMPTY_REPORT} />);
    expect(screen.getByText('暂无测量数据')).toBeTruthy();
    expect(screen.queryByRole('list', { name: '阶段耗时' })).toBeNull();
  });

  it('saturates the bar at 100% — overlapping stages can work longer than the stopwatch', () => {
    expect(stageShare(260, 1180)).toBe(23);
    expect(stageShare(0, 1180)).toBe(0);
    expect(stageShare(1300, 650)).toBe(100);
    expect(stageShare(260, 0)).toBe(0);
  });
});

// ---------------------------------------------------------------- the hook ---

type Listener = (event: { payload: unknown }) => void;

const { listenMock } = vi.hoisted(() => ({
  listenMock: {
    current: null as null | ((event: string, handler: Listener) => Promise<() => void>),
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: Listener) => {
    if (listenMock.current === null) throw new Error('listen mock not configured');
    return listenMock.current(event, handler);
  },
}));

/** Registers the hook's listener and returns a pusher for its payloads. */
async function mountHook(): Promise<{ emit: (payload: unknown) => void; result: { current: ReturnType<typeof useLatencyWaterfall> } }> {
  const listeners: Listener[] = [];
  listenMock.current = async (_event, handler) => {
    listeners.push(handler);
    return () => undefined;
  };
  const { result } = renderHook(() => useLatencyWaterfall());
  await waitFor(() => expect(listeners).toHaveLength(1));
  return { emit: (payload) => act(() => listeners.forEach((listener) => listener({ payload }))), result };
}

describe('useLatencyWaterfall', () => {
  afterEach(() => {
    listenMock.current = null;
  });

  it('narrows a latency payload into a report and stays on the live source', async () => {
    const { emit, result } = await mountHook();
    expect(result.current.report).toBeNull();
    expect(result.current.source).toBe('live');

    emit({ cold: { ...classReport(OVER_BUDGET, 1, 2501, 2501), latest: OVER_BUDGET }, warm: classReport(WARM, 4, 1225, 1300) });

    expect(result.current.source).toBe('live');
    expect(result.current.report?.cold.latest?.e2eMs).toBe(2501);
    expect(result.current.report?.cold.latest?.verdict).toEqual({
      kind: 'overBudget',
      stage: 'ttsFirstAudio',
      overByMs: 501,
    });
  });

  it('drops a malformed latency payload before it reaches the panel', async () => {
    const { emit, result } = await mountHook();

    emit({ cold: { segments: 'many' }, warm: null });
    expect(result.current.report).toBeNull();

    // A stage map missing four boundaries is not a partially-true waterfall.
    const truncated = { ...COLD, stageMs: { micCallback: 0 } as Record<LatencyStage, number> };
    emit({ cold: { ...classReport(COLD, 1, 1180, 1180), latest: truncated }, warm: classReport(WARM, 4, 1225, 1300) });
    expect(result.current.report).toBeNull();
  });

  it('falls back to the typed preview fixture only when there is no IPC bridge', async () => {
    listenMock.current = async () => {
      throw new Error('no IPC bridge');
    };
    const { result } = renderHook(() => useLatencyWaterfall());

    await waitFor(() => expect(result.current.source).toBe('preview'));

    const preview = result.current.report;
    expect(preview).not.toBeNull();
    expect(preview?.cold.latest).not.toBeNull();
    for (const stage of STAGE_ORDER) {
      expect(preview?.cold.stages[stage]).toBeTruthy();
    }
    // micCallback is the stopwatch origin (0ms by construction); the stages
    // that actually stream carry real numbers.
    expect(preview?.cold.stages.ttsFirstAudio.p50Ms).toBeGreaterThan(0);
    expect(preview?.cold.latest?.verdict.kind).toBe('withinBudget');
  });
});
