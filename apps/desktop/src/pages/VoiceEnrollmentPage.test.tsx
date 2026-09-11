import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import VoiceEnrollmentPage from './VoiceEnrollmentPage';

/**
 * WR-05 regression: the microphone must not stay live after the user leaves
 * the recording step. 上一步 during the countdown used to keep the stream open
 * and the interval ticking — at zero it force-jumped the wizard forward to
 * step 2 from wherever the user had navigated to.
 */

const MAX_SECONDS = 180;

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/voice']}>
      <VoiceEnrollmentPage />
    </MemoryRouter>,
  );
}

describe('VoiceEnrollmentPage recording lifecycle', () => {
  const stopTrack = vi.fn();

  beforeEach(() => {
    stopTrack.mockClear();
    const stream = { getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream;
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: { getUserMedia: vi.fn().mockResolvedValue(stream) },
    });
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  /** Walks to step 1 and starts the take (the mic prompt resolves). */
  async function startTake() {
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '开始录音' }));
    });
    expect(screen.getByRole('button', { name: '停止录音' })).toBeTruthy();
    expect(stopTrack).not.toHaveBeenCalled();
  }

  test('上一步 during the countdown releases the microphone and stops the timer', async () => {
    renderPage();
    await startTake();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '上一步' }));
    });

    expect(stopTrack).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: '下一步' })).toBeTruthy();

    // The interval is gone: driving the clock past the take length must not
    // jump the wizard to 试听与完成 from step 0.
    act(() => {
      vi.advanceTimersByTime(MAX_SECONDS * 1000);
    });
    expect(screen.getByRole('button', { name: '下一步' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '完成' })).toBeNull();
    expect(stopTrack).toHaveBeenCalledTimes(1); // no further release
  });

  test('the countdown still finishes the take when the user stays', async () => {
    renderPage();
    await startTake();

    act(() => {
      vi.advanceTimersByTime(MAX_SECONDS * 1000);
    });

    expect(stopTrack).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: '完成' })).toBeTruthy();
  });
});
