import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { useWakeLock } from './useWakeLock';

/**
 * 01-04 Task 2 gate (SYNC-04): the screen stays awake on hardware where the
 * Wake Lock API exists (secure context) AND on the plain-http LAN origin the
 * phone actually loads, where the API is absent and a hidden muted looping
 * video has to carry the session (research Pattern 4).
 */

interface FakeSentinel {
  release: ReturnType<typeof vi.fn>;
  addEventListener: ReturnType<typeof vi.fn>;
}

function installWakeLock(request: ReturnType<typeof vi.fn>): void {
  Object.defineProperty(navigator, 'wakeLock', { value: { request }, configurable: true });
}

function setVisibility(state: 'visible' | 'hidden'): void {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
  document.dispatchEvent(new Event('visibilitychange'));
}

let playSpy: ReturnType<typeof vi.fn>;
let pauseSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  playSpy = vi.fn().mockResolvedValue(undefined);
  pauseSpy = vi.fn();
  Object.defineProperty(HTMLMediaElement.prototype, 'play', {
    value: playSpy,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(HTMLMediaElement.prototype, 'pause', {
    value: pauseSpy,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(window, 'isSecureContext', { value: true, configurable: true });
});

afterEach(() => {
  Reflect.deleteProperty(navigator, 'wakeLock');
  setVisibility('visible');
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

function videoElement(): HTMLVideoElement | null {
  return document.body.querySelector('video');
}

describe('useWakeLock — secure context', () => {
  test('requests the screen lock from the gesture and releases it on unmount', async () => {
    const sentinel: FakeSentinel = { release: vi.fn().mockResolvedValue(undefined), addEventListener: vi.fn() };
    const request = vi.fn().mockResolvedValue(sentinel);
    installWakeLock(request);

    const { result, unmount } = renderHook(() => useWakeLock());
    expect(result.current.isWakeActive).toBe(false);

    await act(async () => {
      result.current.activate();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith('screen');
    expect(result.current.isWakeActive).toBe(true);
    expect(result.current.isFallback).toBe(false);
    expect(videoElement()).toBeNull();

    unmount();
    expect(sentinel.release).toHaveBeenCalledTimes(1);
  });

  test('re-requests the lock when the page returns to the foreground', async () => {
    const request = vi.fn().mockResolvedValue({
      release: vi.fn().mockResolvedValue(undefined),
      addEventListener: vi.fn(),
    });
    installWakeLock(request);

    const { result } = renderHook(() => useWakeLock());
    await act(async () => {
      result.current.activate();
      await Promise.resolve();
    });
    expect(request).toHaveBeenCalledTimes(1);

    await act(async () => {
      setVisibility('hidden');
      setVisibility('visible');
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledTimes(2);
  });
});

describe('useWakeLock — plain http LAN fallback', () => {
  test('plays a hidden muted looping video when the API is absent, and emits the fallback toast', () => {
    const onFallbackEngaged = vi.fn();
    const { result } = renderHook(() => useWakeLock({ onFallbackEngaged }));

    act(() => result.current.activate());

    const video = videoElement();
    expect(video).not.toBeNull();
    expect(video?.muted).toBe(true);
    expect(video?.loop).toBe(true);
    expect(video?.playsInline).toBe(true);
    expect(video?.getAttribute('aria-hidden')).toBe('true');
    expect(video?.style.opacity).toBe('0');
    expect(video?.style.pointerEvents).toBe('none');
    expect(video?.src).toContain('blank-loop');
    expect(playSpy).toHaveBeenCalledTimes(1);

    expect(result.current.isFallback).toBe(true);
    expect(result.current.isWakeActive).toBe(true);
    expect(onFallbackEngaged).toHaveBeenCalledTimes(1);
  });

  test('falls back when the request rejects (NotAllowedError on an insecure origin)', async () => {
    const request = vi.fn().mockRejectedValue(new Error('NotAllowedError'));
    installWakeLock(request);
    const onFallbackEngaged = vi.fn();

    const { result } = renderHook(() => useWakeLock({ onFallbackEngaged }));
    await act(async () => {
      result.current.activate();
      await Promise.resolve();
    });

    expect(result.current.isFallback).toBe(true);
    expect(playSpy).toHaveBeenCalled();
    expect(onFallbackEngaged).toHaveBeenCalledTimes(1);
  });

  test('pauses the video while hidden and replays it when visible again', () => {
    const { result } = renderHook(() => useWakeLock());
    act(() => result.current.activate());
    playSpy.mockClear();

    act(() => setVisibility('hidden'));
    expect(pauseSpy).toHaveBeenCalledTimes(1);

    act(() => setVisibility('visible'));
    expect(playSpy).toHaveBeenCalledTimes(1);
  });

  test('deactivate stops the fallback and removes the video', () => {
    const { result } = renderHook(() => useWakeLock());
    act(() => result.current.activate());
    expect(videoElement()).not.toBeNull();

    act(() => result.current.deactivate());

    expect(videoElement()).toBeNull();
    expect(result.current.isWakeActive).toBe(false);
    expect(result.current.isFallback).toBe(false);
  });
});
