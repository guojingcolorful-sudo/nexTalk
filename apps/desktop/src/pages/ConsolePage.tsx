import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faClosedCaptioning } from '@fortawesome/free-solid-svg-icons';
import HeaderBar from '../components/HeaderBar';
import NexTalkBrand from '../components/NexTalkBrand';
import QrCodeCard from '../components/QrCodeCard';

/**
 * Console page (340x680) — the hub window (DSK-01). Skeleton slice:
 * brand header, live pairing QR (QrCodeCard), subtitles empty state,
 * and the 开始模拟会话 primary CTA. The full hub (stealth card,
 * knowledge rows, secondary actions) is 01-03.
 */
export default function ConsolePage() {
  const [sessionRunning, setSessionRunning] = useState(false);

  const startSession = () => {
    invoke('start_session')
      .then(() => setSessionRunning(true))
      .catch((err) => {
        console.error('start_session failed', err);
      });
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green">
        <NexTalkBrand xColor="ink" subtitle="极言" />
      </HeaderBar>

      <main className="flex-1 space-y-4 overflow-y-auto p-4">
        {/* Live pairing QR — real get_pairing_info + qrcode rendering */}
        <QrCodeCard />

        {/* Subtitles empty state (pre-session) */}
        <section
          aria-label="实时字幕"
          className="flex flex-col items-center gap-1 rounded-xl border-4 border-black bg-spaceDark p-4 text-center"
        >
          <span className="flex h-10 w-10 items-center justify-center rounded-lg border-2 border-black bg-portalGreen text-black">
            <FontAwesomeIcon icon={faClosedCaptioning} aria-hidden="true" />
          </span>
          <p className="mt-2 text-[13px] font-bold text-white">等待语音输入</p>
          <p className="text-xs text-gray-400">模拟会话开始后，双语字幕将显示在这里</p>
        </section>
      </main>

      <footer className="shrink-0 border-t-4 border-black bg-spaceDark p-3">
        <button
          type="button"
          onClick={startSession}
          disabled={sessionRunning}
          className="w-full rounded-xl border-4 border-black bg-portalGreen px-4 py-3 text-sm font-bold uppercase tracking-wider text-black shadow-cartoon-black transition hover:translate-y-1 hover:shadow-none active:scale-[0.98] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-black disabled:cursor-not-allowed disabled:opacity-50"
        >
          {sessionRunning ? '会话进行中' : '开始模拟会话'}
        </button>
      </footer>
    </div>
  );
}
