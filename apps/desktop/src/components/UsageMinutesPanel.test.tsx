import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import SetupWizardPage from '../pages/SetupWizardPage';
import UsageMinutesPanel from './UsageMinutesPanel';

/**
 * T3.9 用量面板 (D-15/GOV-17): 用户视角的月度分钟数读数——正常态显示
 * 「本月已用 X 分钟 / 剩余 Y 分钟」，接近额度只提示不阻断，无溯源数据时
 * 显示「暂无用量数据」（不是 0/NaN），没有 IPC 桥时退回类型化预览 fixture
 * 并显式标注，面板注明数据纯本地。分阶段明细留在 /diagnostics，不在这里。
 */

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

function report(usedMinutes: number, segments = 12) {
  return {
    usage: {
      sessions: 2,
      segments,
      skippedLines: 0,
      sttAudioMs: usedMinutes * 60_000,
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
    usedMinutes,
  };
}

function renderPanel() {
  return render(<UsageMinutesPanel />);
}

describe('UsageMinutesPanel 用量面板', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  // 本仓库 vitest 未开 globals，RTL 自动 cleanup 不生效——显式清理。
  afterEach(() => {
    cleanup();
  });

  it('renders this month’s used and remaining minutes from usage_summary', async () => {
    invokeMock.mockResolvedValue(report(120));
    renderPanel();

    expect(await screen.findByText('本月已用 120 分钟 / 剩余 480 分钟')).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith('usage_summary');

    // 未到阈值：不出现提示条。
    expect(screen.queryByText('用量已接近本月额度，请留意')).toBeNull();
  });

  it('shows the near-quota hint at 80% without blocking the panel', async () => {
    invokeMock.mockResolvedValue(report(480));
    renderPanel();

    expect(await screen.findByText('本月已用 480 分钟 / 剩余 120 分钟')).toBeTruthy();
    // 提示式（D-15）：提示出现的同时，读数照常可用——没有硬停、没有禁用。
    expect(screen.getByText('用量已接近本月额度，请留意')).toBeTruthy();
  });

  it('renders 暂无用量数据 when no usage was recorded — not 0, not NaN', async () => {
    invokeMock.mockResolvedValue(report(0, 0));
    renderPanel();

    expect(await screen.findByText('暂无用量数据')).toBeTruthy();
    expect(screen.queryByText(/本月已用/)).toBeNull();
    expect(screen.queryByText(/NaN/)).toBeNull();
  });

  it('falls back to the typed preview fixture when the command is unavailable', async () => {
    invokeMock.mockRejectedValue(new Error('no ipc bridge'));
    renderPanel();

    // 浏览器预览：显式标注 + 类型化 fixture，面板不崩溃。
    expect(await screen.findByText('预览数据：未连接桌面测量装置')).toBeTruthy();
    expect(screen.getByText('本月已用 96 分钟 / 剩余 504 分钟')).toBeTruthy();
  });

  it('states the local-only provenance in Chinese', async () => {
    invokeMock.mockResolvedValue(report(120));
    renderPanel();

    expect(await screen.findByText('数据来自本地记录，不上传云端')).toBeTruthy();
  });
});

describe('UsageMinutesPanel mount', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it('is mounted on the /setup settings page', async () => {
    invokeMock.mockResolvedValue(report(60));

    render(
      <MemoryRouter>
        <SetupWizardPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText('用量')).toBeTruthy();
    expect(await screen.findByText('本月已用 60 分钟 / 剩余 540 分钟')).toBeTruthy();
  });
});
