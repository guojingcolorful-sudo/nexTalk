import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faHeadphones } from '@fortawesome/free-solid-svg-icons';
import ErrorBanner from '../components/ErrorBanner';
import MicStatusPill from '../components/MicStatusPill';
import NeobrutalismButton from '../components/NeobrutalismButton';
import WizardShell from '../components/WizardShell';
import { MOCK_BADGE_LABEL, MOCK_VOICE_READING_TEXT, MOCK_VOICE_SAMPLE_LABEL } from '../data/mock-data';

const STEPS = ['准备', '录音 1-3 分钟', '试听与完成'] as const;

/** Upper bound of the 1-3 minute take the contract asks for. */
const MAX_SECONDS = 180;

function formatCountdown(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
}

/**
 * VoiceEnrollmentPage (音色注册) — 3 steps per the Missing Pages Contract.
 * UI only: the mic is requested so the unavailable path is real, but nothing
 * is captured or cloned yet (Phase 2), and the playback tile is a placeholder.
 */
export default function VoiceEnrollmentPage() {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [recording, setRecording] = useState(false);
  const [remaining, setRemaining] = useState(MAX_SECONDS);
  const [micError, setMicError] = useState(false);
  const remainingRef = useRef(MAX_SECONDS);
  const streamRef = useRef<MediaStream | null>(null);

  const releaseMic = useCallback(() => {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
  }, []);

  const finishRecording = useCallback(() => {
    releaseMic();
    setRecording(false);
    setStep(2);
  }, [releaseMic]);

  /** Leaves the recording step without finishing: release the device and stop
   *  the countdown so it cannot advance a view the user has left (WR-05). */
  const cancelRecording = useCallback(() => {
    releaseMic();
    setRecording(false);
    remainingRef.current = MAX_SECONDS;
    setRemaining(MAX_SECONDS);
  }, [releaseMic]);

  const handlePrev = useCallback(() => {
    cancelRecording();
    setStep((value) => Math.max(0, value - 1));
  }, [cancelRecording]);

  useEffect(
    () => () => {
      releaseMic();
    },
    [releaseMic],
  );

  useEffect(() => {
    if (!recording) return undefined;
    const id = window.setInterval(() => {
      const next = Math.max(0, remainingRef.current - 1);
      remainingRef.current = next;
      setRemaining(next);
      if (next === 0) finishRecording();
    }, 1000);
    return () => window.clearInterval(id);
  }, [recording, finishRecording]);

  const startRecording = async () => {
    setMicError(false);
    try {
      const devices = navigator.mediaDevices;
      if (devices === undefined || typeof devices.getUserMedia !== 'function') {
        throw new Error('mediaDevices unavailable');
      }
      streamRef.current = await devices.getUserMedia({ audio: true });
    } catch {
      setMicError(true);
      return;
    }
    remainingRef.current = MAX_SECONDS;
    setRemaining(MAX_SECONDS);
    setRecording(true);
  };

  const forwardAction = () => {
    if (step === 0) {
      setStep(1);
      return;
    }
    if (step === 1) {
      if (recording) finishRecording();
      else void startRecording();
      return;
    }
    navigate('/console');
  };

  return (
    <WizardShell
      title="音色注册"
      steps={STEPS}
      current={step}
      onBack={() => navigate('/console')}
      onPrev={handlePrev}
      actions={
        <NeobrutalismButton onClick={forwardAction} className="w-full">
          {step === 0 ? '下一步' : step === 1 ? (recording ? '停止录音' : '开始录音') : '完成'}
        </NeobrutalismButton>
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
          {micError ? (
            <ErrorBanner
              tone="red"
              title="麦克风不可用"
              body="请在 系统设置 → 隐私与安全性 → 麦克风 中允许访问"
            />
          ) : null}
        </div>
      ) : null}

      {step === 1 ? (
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-[15px] font-bold text-white">录音 1-3 分钟</h2>
            {recording ? <MicStatusPill /> : null}
          </div>
          <p
            data-testid="recording-countdown"
            role="timer"
            className="rounded-xl border-4 border-black bg-spaceDark p-3 text-center text-3xl font-bold tabular-nums text-portalGreen"
          >
            {formatCountdown(recording ? remaining : MAX_SECONDS)}
          </p>
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
            {recording ? '正在录音，按自然语速朗读示例句子，读满 1 分钟以上再停止。' : '点击开始录音，授权麦克风后开始。'}
          </p>
          {micError ? (
            <ErrorBanner
              tone="red"
              title="麦克风不可用"
              body="请在 系统设置 → 隐私与安全性 → 麦克风 中允许访问"
            />
          ) : null}
        </div>
      ) : null}

      {step === 2 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">试听与完成</h2>
          <section className="rounded-xl border-4 border-black bg-spaceDark p-3">
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="text-[10px] font-bold uppercase tracking-wider text-gray-400">
                {MOCK_VOICE_SAMPLE_LABEL}
              </span>
              <span className="rounded border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold text-black">
                {MOCK_BADGE_LABEL}
              </span>
            </div>
            <div className="flex items-center gap-3">
              <span
                aria-hidden="true"
                className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border-2 border-black bg-portalGreen text-black"
              >
                <FontAwesomeIcon icon={faHeadphones} />
              </span>
              <NeobrutalismButton variant="paper" size="sm" disabled>
                播放
              </NeobrutalismButton>
            </div>
          </section>
          <p className="text-[12px] leading-relaxed text-gray-400">
            真实音色克隆与播放将在后续版本接入，当前样本不会被上传。
          </p>
        </div>
      ) : null}
    </WizardShell>
  );
}
