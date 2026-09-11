import { useCallback, useEffect, useRef, useState } from 'react';
// Emitted as a real asset (not inlined) so the phone fetches the loop from the
// LAN server and the browser can cache it — zero CDN, zero runtime synthesis.
import blankLoopUrl from '../assets/blank-loop.mp4?no-inline';

/**
 * useWakeLock — keeps the phone screen awake during 提词 (SYNC-04).
 *
 * Two paths, because the phone almost never gets the nice one:
 * 1. Secure context with the Screen Wake Lock API (https, or localhost) →
 *    `navigator.wakeLock.request('screen')`, re-acquired on visibilitychange.
 * 2. Plain `http://192.168.x.x` (no secure context, no API — the real LAN
 *    demo case) → a 1×1 invisible muted looping video keeps the device awake
 *    (the NoSleep technique). It needs the same user gesture, which is why
 *    `activate` must be called from the 开始提词 tap.
 *
 * Everything is gesture-bound and reversed on `deactivate`/unmount: no leaked
 * sentinels, no orphaned video elements.
 */

interface WakeLockSentinelLike {
  release: () => Promise<void>;
  addEventListener?: (type: 'release', listener: () => void) => void;
}

interface WakeLockApi {
  request: (type: 'screen') => Promise<WakeLockSentinelLike>;
}

export interface WakeLockOptions {
  /** Called once when the video fallback takes over (toast 已启用防休眠回退模式). */
  onFallbackEngaged?: () => void;
}

export interface WakeLockResult {
  isWakeActive: boolean;
  isFallback: boolean;
  /** Engage from the user gesture (开始提词). */
  activate: () => void;
  /** Release on 暂停提词 / leaving the page. */
  deactivate: () => void;
}

const FALLBACK_VIDEO_STYLE =
  'position:fixed;left:-2px;top:-2px;width:1px;height:1px;opacity:0;pointer-events:none';

function wakeLockApi(): WakeLockApi | null {
  if (typeof navigator === 'undefined' || typeof window === 'undefined') return null;
  if (window.isSecureContext !== true) return null;
  const api = (navigator as Navigator & { wakeLock?: WakeLockApi }).wakeLock;
  return api ?? null;
}

/** Media elements may reject play() before a gesture — never an unhandled rejection. */
function attemptPlay(video: HTMLVideoElement): void {
  const result: unknown = video.play();
  if (result instanceof Promise) result.catch(() => undefined);
}

export function useWakeLock(options: WakeLockOptions = {}): WakeLockResult {
  const [isWakeActive, setWakeActive] = useState(false);
  const [isFallback, setFallback] = useState(false);

  const sentinelRef = useRef<WakeLockSentinelLike | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  /** True between activate() and deactivate()/unmount. */
  const engagedRef = useRef(false);
  const onFallbackEngagedRef = useRef(options.onFallbackEngaged);

  useEffect(() => {
    onFallbackEngagedRef.current = options.onFallbackEngaged;
  }, [options.onFallbackEngaged]);

  const startVideoFallback = useCallback(() => {
    if (videoRef.current) return;
    const video = document.createElement('video');
    video.src = blankLoopUrl;
    video.loop = true;
    video.muted = true;
    video.playsInline = true;
    video.setAttribute('aria-hidden', 'true');
    video.setAttribute('tabindex', '-1');
    video.style.cssText = FALLBACK_VIDEO_STYLE;
    document.body.appendChild(video);
    videoRef.current = video;

    attemptPlay(video);
    setFallback(true);
    setWakeActive(true);
    onFallbackEngagedRef.current?.();
  }, []);

  const activate = useCallback(() => {
    engagedRef.current = true;
    const api = wakeLockApi();
    if (!api) {
      startVideoFallback();
      return;
    }
    api
      .request('screen')
      .then((sentinel) => {
        if (!engagedRef.current) {
          void sentinel.release();
          return;
        }
        sentinelRef.current = sentinel;
        setWakeActive(true);
        sentinel.addEventListener?.('release', () => setWakeActive(false));
      })
      .catch(() => {
        // NotAllowedError / no user activation / home-screen PWA bug → video.
        if (engagedRef.current) startVideoFallback();
      });
  }, [startVideoFallback]);

  const deactivate = useCallback(() => {
    engagedRef.current = false;
    const sentinel = sentinelRef.current;
    sentinelRef.current = null;
    if (sentinel) void sentinel.release();

    const video = videoRef.current;
    videoRef.current = null;
    if (video) {
      video.pause();
      video.remove();
    }

    setFallback(false);
    setWakeActive(false);
  }, []);

  // The OS releases the lock (and pauses the fallback video) whenever the page
  // is hidden, so both paths have to be re-engaged on the way back.
  useEffect(() => {
    const onVisibilityChange = () => {
      if (!engagedRef.current) return;
      if (document.visibilityState !== 'visible') {
        videoRef.current?.pause();
        return;
      }
      const api = wakeLockApi();
      if (sentinelRef.current && api) {
        api
          .request('screen')
          .then((sentinel) => {
            if (!engagedRef.current) {
              void sentinel.release();
              return;
            }
            sentinelRef.current = sentinel;
            setWakeActive(true);
          })
          .catch(() => undefined);
        return;
      }
      const video = videoRef.current;
      if (video) attemptPlay(video);
    };

    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => document.removeEventListener('visibilitychange', onVisibilityChange);
  }, []);

  // Release everything on unmount without touching state.
  useEffect(
    () => () => {
      engagedRef.current = false;
      const sentinel = sentinelRef.current;
      sentinelRef.current = null;
      if (sentinel) void sentinel.release();
      const video = videoRef.current;
      videoRef.current = null;
      if (video) {
        video.pause();
        video.remove();
      }
    },
    [],
  );

  return { isWakeActive, isFallback, activate, deactivate };
}
