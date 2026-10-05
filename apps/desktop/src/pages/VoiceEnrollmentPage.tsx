import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import ErrorBanner from '../components/ErrorBanner';
import MicStatusPill from '../components/MicStatusPill';
import NeobrutalismButton from '../components/NeobrutalismButton';
import WizardShell from '../components/WizardShell';
import { MOCK_VOICE_READING_TEXT } from '../data/mock-data';

const STEPS = ['准备', '录音 1-3 分钟', '训练音色', '试听'] as const;

/** Upper bound of the 1-3 minute take the contract asks for. */
const MAX_SECONDS = 180;
/** The level meter's poll cadence (~10 Hz — cheap over local IPC). */
const LEVEL_POLL_MS = 100;

interface VoiceProfile {
  speakerId: string;
  samplePath: string;
  status: string;
}

interface VoiceStatus {
  profile: VoiceProfile | null;
  voice: { kind: string; name: string };
  warning: string | null;
}

interface TakeResult {
  path: string;
  durationS: number;
  silenceRatio: number;
  bytes: number;
}

/** A capture failure; `afterStop` distinguishes a rejected take (offer
 *  重新录制) from a recorder that never opened (offer 开始录音). */
interface CaptureFailure {
  message: string;
  afterStop: boolean;
}

function formatClock(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
}

/** Rust answers failures as `{ code, message }` — the message is the locked
 *  Chinese copy; anything else falls back. */
function commandMessage(error: unknown, fallback: string): string {
  if (typeof error === 'object' && error !== null) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === 'string' && message.length > 0) return message;
  }
  return fallback;
}

/**
 * VoiceEnrollmentPage (音色注册) — the real four-step wizard (02-04 T4.3):
 * 准备 → 录音 → 训练音色 → 试听.
 *
 * The microphone belongs to Rust (`start/stop_enrollment_recording`, cpal);
 * this page only drives the take, polls `enrollment_level` for the meter, and
 * hands the saved sample to `train_voice_clone`. The header badge mirrors the
 * resolved voice from `get_voice_profile` — the same `resolve_voice()` the
 * cascade reads, so 我的克隆 is only ever shown when the clone will really
 * speak (T-02-20). Step 4 previews both fixed lines (fresh synthesis per
 * click), retrains from the stored sample, and can delete the profile.
 */
export default function VoiceEnrollmentPage() {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [phase, setPhase] = useState<'idle' | 'recording'>('idle');
  const [elapsed, setElapsed] = useState(0);
  const [level, setLevel] = useState(0);
  const [take, setTake] = useState<TakeResult | null>(null);
  const [stopping, setStopping] = useState(false);
  const [captureError, setCaptureError] = useState<CaptureFailure | null>(null);
  const [training, setTraining] = useState(false);
  const [trainError, setTrainError] = useState<string | null>(null);
  const [voice, setVoice] = useState<VoiceStatus | null>(null);
  const [previewing, setPreviewing] = useState<'zh' | 'en' | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [retraining, setRetraining] = useState(false);
  const [retrainError, setRetrainError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const elapsedRef = useRef(0);
  const phaseRef = useRef<'idle' | 'recording'>('idle');
  const stoppingRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    void invoke<VoiceStatus>('get_voice_profile')
      .then((status) => {
        if (!cancelled) setVoice(status);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // Leaving the page mid-take must release the device (WR-05).
  useEffect(
    () => () => {
      if (phaseRef.current === 'recording') {
        void invoke('stop_enrollment_recording').catch(() => undefined);
      }
    },
    [],
  );

  const enterPhase = useCallback((next: 'idle' | 'recording') => {
    phaseRef.current = next;
    setPhase(next);
  }, []);

  const startTake = useCallback(async () => {
    setCaptureError(null);
    setTrainError(null);
    try {
      await invoke('start_enrollment_recording');
    } catch (error) {
      setCaptureError({
        message: commandMessage(error, '无法开始录音，请检查麦克风'),
        afterStop: false,
      });
      return;
    }
    elapsedRef.current = 0;
    setElapsed(0);
    setLevel(0);
    enterPhase('recording');
  }, [enterPhase]);

  const stopTake = useCallback(async (): Promise<TakeResult | null> => {
    if (stoppingRef.current) return null;
    stoppingRef.current = true;
    setStopping(true);
    try {
      const result = await invoke<TakeResult>('stop_enrollment_recording');
      setTake(result);
      setCaptureError(null);
      enterPhase('idle');
      setStep(2);
      return result;
    } catch (error) {
      setCaptureError({
        message: commandMessage(error, '录音未通过检查，请重录'),
        afterStop: true,
      });
      enterPhase('idle');
      return null;
    } finally {
      stoppingRef.current = false;
      setStopping(false);
    }
  }, [enterPhase]);

  /** 上一步 during a take cancels it: stop the recorder, discard the result. */
  const cancelTake = useCallback(async () => {
    try {
      await invoke('stop_enrollment_recording');
    } catch {
      // The take is discarded either way.
    }
    enterPhase('idle');
    setStep(0);
  }, [enterPhase]);

  const handlePrev = useCallback(() => {
    if (phaseRef.current === 'recording') {
      void cancelTake();
      return;
    }
    setStep((value) => Math.max(0, value - 1));
  }, [cancelTake]);

  // The take clock: counts up, stops itself at the 3-minute ceiling.
  useEffect(() => {
    if (phase !== 'recording') return undefined;
    const id = window.setInterval(() => {
      elapsedRef.current += 1;
      setElapsed(elapsedRef.current);
      if (elapsedRef.current >= MAX_SECONDS) void stopTake();
    }, 1000);
    return () => window.clearInterval(id);
  }, [phase, stopTake]);

  // The level meter: read the newest input peak from Rust.
  useEffect(() => {
    if (phase !== 'recording') return undefined;
    const id = window.setInterval(() => {
      void invoke<number | null>('enrollment_level')
        .then((value) => {
          if (typeof value === 'number' && Number.isFinite(value)) {
            setLevel(Math.min(1, Math.max(0, value)));
          }
        })
        .catch(() => undefined);
    }, LEVEL_POLL_MS);
    return () => window.clearInterval(id);
  }, [phase]);

  const train = useCallback(async () => {
    if (take === null || training) return;
    setTrainError(null);
    setTraining(true);
    try {
      const status = await invoke<VoiceStatus>('train_voice_clone', {
        samplePath: take.path,
        transcript: MOCK_VOICE_READING_TEXT,
      });
      setVoice(status);
      setStep(3);
    } catch (error) {
      setTrainError(commandMessage(error, '训练失败，请稍后重试'));
    } finally {
      setTraining(false);
    }
  }, [take, training]);

  /** Synthesize one fixed preview line with the voice resolved right now —
   *  no caching, so a preview after 重新训练 plays the new speaker. */
  const preview = useCallback(
    async (kind: 'zh' | 'en') => {
      if (previewing !== null) return;
      setPreviewError(null);
      setPreviewing(kind);
      try {
        await invoke('preview_voice', { kind });
      } catch (error) {
        setPreviewError(commandMessage(error, '试听播放失败，请稍后重试'));
      } finally {
        setPreviewing(null);
      }
    },
    [previewing],
  );

  /** Re-trains from the stored sample (no re-recording). On failure the old
   *  profile is untouched — `train_voice_clone` only saves on success. */
  const retrain = useCallback(async () => {
    const samplePath = voice?.profile?.samplePath;
    if (samplePath === undefined || retraining) return;
    setRetrainError(null);
    setRetraining(true);
    try {
      const status = await invoke<VoiceStatus>('train_voice_clone', {
        samplePath,
        transcript: MOCK_VOICE_READING_TEXT,
      });
      setVoice(status);
    } catch (error) {
      setRetrainError(commandMessage(error, '训练失败，请稍后重试'));
    } finally {
      setRetraining(false);
    }
  }, [voice, retraining]);

  /** 删除音色档案 — wipes the profile and every take (T-02-16), after a
   *  second click confirms. */
  const removeProfile = useCallback(async () => {
    if (deleting) return;
    setDeleteError(null);
    setDeleting(true);
    try {
      await invoke('delete_voice_profile');
      setVoice(await invoke<VoiceStatus>('get_voice_profile'));
    } catch (error) {
      setDeleteError(commandMessage(error, '删除失败，请稍后重试'));
    } finally {
      setDeleting(false);
      setConfirmingDelete(false);
    }
  }, [deleting]);

  const forwardAction = () => {
    if (step === 0) {
      setStep(1);
      return;
    }
    if (step === 1) {
      if (phase === 'recording') {
        void stopTake();
        return;
      }
      if (take !== null) {
        setStep(2);
        return;
      }
      void startTake();
      return;
    }
    if (step === 2) {
      void train();
      return;
    }
    navigate('/console');
  };

  const stepOneLabel =
    phase === 'recording'
      ? '停止录音'
      : take !== null
        ? '下一步'
        : captureError?.afterStop
          ? '重新录制'
          : '开始录音';

  const forwardLabel =
    step === 0
      ? '下一步'
      : step === 1
        ? stepOneLabel
        : step === 2
          ? trainError !== null
            ? '重试'
            : '开始训练'
          : '完成';

  const badge =
    voice !== null ? (
      <span className="rounded-md border-2 border-black bg-black px-2 py-1 text-[10px] font-bold text-white shadow-[2px_2px_0_0_#000]">
        {voice.voice.kind === 'clone' ? '当前音色：我的克隆' : '当前音色：预置'}
      </span>
    ) : null;

  return (
    <WizardShell
      title="音色注册"
      steps={STEPS}
      current={step}
      badge={badge}
      onBack={() => navigate('/console')}
      onPrev={step === 2 && training ? undefined : handlePrev}
      actions={
        <>
          {step === 1 && take !== null && phase === 'idle' ? (
            <NeobrutalismButton
              variant="ghost"
              size="sm"
              disabled={stopping}
              onClick={() => void startTake()}
            >
              重新录制
            </NeobrutalismButton>
          ) : null}
          <NeobrutalismButton
            onClick={forwardAction}
            disabled={stopping || (step === 2 && training)}
            loading={step === 2 && training}
            loadingLabel="正在训练音色…"
          >
            {forwardLabel}
          </NeobrutalismButton>
        </>
      }
    >
      {step === 0 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">准备录音</h2>
          <p className="text-[13px] leading-relaxed text-gray-400">
            找一个安静的环境，用平时说话的音量和语速朗读下面的示例句，录满 1 到 3 分钟。
          </p>
          <p className="rounded-xl border-4 border-black bg-spaceDark p-3 text-[13px] leading-relaxed text-white">
            {MOCK_VOICE_READING_TEXT}
          </p>
          <p className="text-[12px] leading-relaxed text-gray-400">
            首次录音时系统会询问麦克风权限；若曾被拒绝，请到 系统设置 → 隐私与安全性 →
            麦克风 中打开。
          </p>
          {voice?.warning ? (
            <ErrorBanner tone="yellow" title="音色档案不可用" body={voice.warning} />
          ) : null}
          {captureError ? (
            <ErrorBanner tone="red" title="录音失败" body={captureError.message} />
          ) : null}
        </div>
      ) : null}

      {step === 1 ? (
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-[15px] font-bold text-white">录音 1-3 分钟</h2>
            {phase === 'recording' ? <MicStatusPill /> : null}
          </div>
          <p
            data-testid="recording-timer"
            role="timer"
            className="rounded-xl border-4 border-black bg-spaceDark p-3 text-center text-3xl font-bold tabular-nums text-portalGreen"
          >
            {formatClock(elapsed)}
          </p>
          <div
            data-testid="level-meter"
            aria-hidden="true"
            className="h-4 w-full overflow-hidden rounded-full border-4 border-black bg-darkerSpace"
          >
            <div
              className="h-full w-full origin-left bg-portalGreen transition-transform duration-100 ease-out motion-reduce:transition-none"
              style={{ transform: `scaleX(${level})` }}
            />
          </div>
          <section
            aria-label="示例句子"
            className="rounded-xl border-4 border-black bg-spaceDark p-3"
          >
            <p className="mb-1.5 text-[10px] font-bold uppercase tracking-wider text-gray-400">
              示例句子
            </p>
            <p className="text-[14px] font-semibold leading-relaxed text-white">
              {MOCK_VOICE_READING_TEXT}
            </p>
          </section>
          <p className="text-[13px] leading-relaxed text-gray-400">
            {phase === 'recording'
              ? '正在录音，按自然语速朗读示例句子，至少读满 1 分钟再停止。'
              : take !== null
                ? `已录 ${formatClock(elapsed)}，可以继续训练，或重新录制。`
                : '点击开始录音；录音只保存在本机。'}
          </p>
          {captureError ? (
            <ErrorBanner tone="red" title="录音失败" body={captureError.message} />
          ) : null}
        </div>
      ) : null}

      {step === 2 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">训练音色</h2>
          <p className="rounded-xl border-4 border-black bg-spaceDark p-3 text-[13px] leading-relaxed text-white">
            用刚才的录音训练你的专属音色。训练会把这段录音发送到火山引擎，其余数据都留在本机。
          </p>
          {take !== null ? (
            <p className="text-[12px] leading-relaxed text-gray-400">
              本次录音 {Math.round(take.durationS)} 秒 · 静音占比{' '}
              {Math.round(take.silenceRatio * 100)}%
            </p>
          ) : null}
          {trainError ? <ErrorBanner tone="red" title="训练失败" body={trainError} /> : null}
        </div>
      ) : null}

      {step === 3 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">试听</h2>
          <p className="rounded-xl border-4 border-black bg-spaceDark p-3 text-[13px] leading-relaxed text-white">
            音色注册完成，克隆音色已生效。分别试听中英文示例句，确认这是你要的音色。
          </p>
          <div className="flex gap-2">
            <NeobrutalismButton
              variant="blue"
              size="sm"
              disabled={previewing !== null}
              loading={previewing === 'zh'}
              loadingLabel="正在播放…"
              onClick={() => void preview('zh')}
            >
              试听中文
            </NeobrutalismButton>
            <NeobrutalismButton
              variant="blue"
              size="sm"
              disabled={previewing !== null}
              loading={previewing === 'en'}
              loadingLabel="正在播放…"
              onClick={() => void preview('en')}
            >
              试听英文
            </NeobrutalismButton>
          </div>
          {previewError ? <ErrorBanner tone="red" title="试听失败" body={previewError} /> : null}
          {voice?.profile ? (
            <div className="flex items-center gap-2">
              <NeobrutalismButton
                variant="paper"
                size="sm"
                disabled={retraining || deleting}
                loading={retraining}
                loadingLabel="正在训练音色…"
                onClick={() => void retrain()}
              >
                重新训练
              </NeobrutalismButton>
              {confirmingDelete ? (
                <NeobrutalismButton
                  variant="red"
                  size="sm"
                  disabled={deleting}
                  onClick={() => void removeProfile()}
                >
                  确认删除？
                </NeobrutalismButton>
              ) : (
                <NeobrutalismButton
                  variant="ghost"
                  size="sm"
                  disabled={retraining || deleting}
                  onClick={() => setConfirmingDelete(true)}
                >
                  删除音色档案
                </NeobrutalismButton>
              )}
            </div>
          ) : null}
          {retrainError ? <ErrorBanner tone="red" title="训练失败" body={retrainError} /> : null}
          {deleteError ? <ErrorBanner tone="red" title="删除失败" body={deleteError} /> : null}
          <p className="text-[12px] leading-relaxed text-gray-400">
            重新训练会复用刚才的录音重新生成音色；训练失败时保留当前音色，录音只保存在本机。
          </p>
        </div>
      ) : null}
    </WizardShell>
  );
}
