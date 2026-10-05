import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import DiagnosticsPage from './DiagnosticsPage';

/**
 * T3.7 成本面板 (D-13): the diagnostics page renders the month's staged cost
 * from `usage_summary` — three staged rows (STT / 翻译 / TTS) plus 合计, the
 * 估算 chip and the 超预算 badge — and falls back to 「暂无用量数据」 when the
 * month holds no traces. The panel never computes anything itself: every
 * number is exactly what Rust handed over.
 */

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

const REPORT = {
  usage: {
    sessions: 2,
    segments: 12,
    skippedLines: 0,
    sttAudioMs: 600_000,
    translatePromptTokens: 1_000_000,
    translateCompletionTokens: 500_000,
    ttsChars: 100_000,
  },
  cost: {
    sttUsd: 0.048,
    translateUsd: 1.55,
    ttsUsd: 4.5,
    totalUsd: 6.098,
    budgetUsd: 5,
    overBudget: true,
  },
  quotaMinutes: 600,
  usedMinutes: 10,
};

const EMPTY_REPORT = {
  usage: {
    sessions: 0,
    segments: 0,
    skippedLines: 0,
    sttAudioMs: 0,
    translatePromptTokens: 0,
    translateCompletionTokens: 0,
    ttsChars: 0,
  },
  cost: {
    sttUsd: 0,
    translateUsd: 0,
    ttsUsd: 0,
    totalUsd: 0,
    budgetUsd: 5,
    overBudget: false,
  },
  quotaMinutes: 600,
  usedMinutes: 0,
};

function renderPage() {
  return render(
    <MemoryRouter>
      <DiagnosticsPage />
    </MemoryRouter>,
  );
}

describe('DiagnosticsPage 成本面板', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    listenMock.mockResolvedValue(() => {});
  });

  it('renders the three staged costs, the total and the over-budget state', async () => {
    invokeMock.mockResolvedValue(REPORT);
    renderPage();

    expect(await screen.findByText('成本明细')).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith('usage_summary');

    // 三段 + 合计：金额逐一来自 Rust 的费率表，前端不重算。
    expect(screen.getByText('STT 语音识别')).toBeTruthy();
    expect(screen.getByText('$0.0480')).toBeTruthy();
    expect(screen.getByText('翻译 (文字)')).toBeTruthy();
    expect(screen.getByText('$1.5500')).toBeTruthy();
    expect(screen.getByText('TTS 语音合成')).toBeTruthy();
    expect(screen.getByText('$4.5000')).toBeTruthy();
    expect(screen.getByText('合计')).toBeTruthy();
    expect(screen.getByText('$6.0980')).toBeTruthy();

    // 各阶段按自己的计量单位展示用量。
    expect(screen.getByText('10 分钟')).toBeTruthy();
    expect(screen.getByText('1,500,000 tokens')).toBeTruthy();
    expect(screen.getByText('100,000 字符')).toBeTruthy();

    // 估算标注与超预算徽标。
    expect(screen.getByText('估算')).toBeTruthy();
    expect(screen.getByText('超预算')).toBeTruthy();
  });

  it('falls back to 暂无用量数据 when the month holds no traces', async () => {
    invokeMock.mockResolvedValue(EMPTY_REPORT);
    renderPage();

    expect(await screen.findByText('暂无用量数据')).toBeTruthy();
    expect(screen.queryByText('超预算')).toBeNull();
    expect(screen.queryByText('$0.0000')).toBeNull();
  });
});
