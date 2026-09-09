import { useMemo } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faBolt, faClosedCaptioning } from '@fortawesome/free-solid-svg-icons';
import type { ServerEvent, Speaker } from '@nextalk/protocol';
import { useTypewriter } from './hooks/useTypewriter';
import { useWs, type WsTicket } from './hooks/useWs';

/**
 * Teleprompter H5 (01-02 walking skeleton) — the minimal phone surface that
 * proves the skeleton's "one real integration": one simulated subtitle flows
 * desktop → LAN WS → this page with typewriter rendering. The full phone UI
 * (subtitle history, strategy cards, pairing screens) is 01-04.
 *
 * Pairing comes from the QR URL query: `?token={token}` (research Open
 * Question 3 keeps an optional `ws=` override for dev/e2e so the page can
 * point at a mock WS server without the desktop app).
 */
function pairingTicket(): WsTicket | null {
  const params = new URLSearchParams(window.location.search);
  const token = params.get('token');
  if (!token) return null;
  return { token, url: params.get('ws') ?? undefined };
}

/** One language line typed out character by character. */
function TypedLine({ text }: { text: string }) {
  const shown = useTypewriter(text);
  return <p className="min-h-[1.5em] whitespace-pre-wrap break-words">{shown}</p>;
}

const SPEAKER_LABEL: Record<Speaker, string> = {
  interviewer: '面试官',
  user: '我',
};

const SPEAKER_CHIP_CLASS: Record<Speaker, string> = {
  interviewer: 'bg-rickBlue text-black',
  user: 'bg-portalGreen text-black',
};

/** The latest final subtitle event (the skeleton renders one live card). */
function latestFinalSubtitle(events: ServerEvent[]) {
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    if (event.t === 'subtitle' && event.final) return event;
  }
  return null;
}

const STATE_COPY = {
  connecting: { dot: 'bg-mortyYellow', label: '连接中…' },
  open: { dot: 'bg-portalGreen', label: '已连接' },
  closed: { dot: 'bg-red', label: '连接已断开' },
} as const;

export default function App() {
  const ticket = useMemo(pairingTicket, []);
  const { events, state } = useWs(ticket);
  const subtitle = useMemo(() => latestFinalSubtitle(events), [events]);
  const status = STATE_COPY[state];

  return (
    <div className="flex h-full justify-center">
      <div className="dot-matrix-root flex h-full w-full max-w-[390px] flex-col overflow-hidden">
        {/* Brand + connection status */}
        <header className="flex h-14 shrink-0 items-center justify-between border-b-4 border-black bg-portalGreen px-3 text-black">
          <div className="flex items-center gap-2">
            <span className="flex h-7 w-7 items-center justify-center rounded-md border-2 border-black bg-white text-xs">
              <FontAwesomeIcon icon={faBolt} aria-hidden="true" />
            </span>
            <p className="text-lg font-bold uppercase tracking-wider">
              NE<span className="text-black underline decoration-2">X</span>TALK
              <span className="ml-2 text-xs font-normal tracking-normal">极言</span>
            </p>
          </div>
          <div className="flex items-center gap-1.5 text-xs font-bold" aria-live="polite">
            <span className={`h-2.5 w-2.5 rounded-full ${status.dot}`} aria-hidden="true" />
            <span>{status.label}</span>
          </div>
        </header>

        {/* Subtitle surface */}
        <main className="flex-1 space-y-4 overflow-y-auto p-4">
          {!ticket ? (
            <section
              aria-label="缺少配对码"
              className="mt-16 flex flex-col items-center gap-2 rounded-xl border-4 border-black bg-spaceDark p-6 text-center"
            >
              <p className="text-sm font-bold text-white">缺少配对码</p>
              <p className="text-xs text-gray-400">请从桌面端的二维码扫码进入本页</p>
            </section>
          ) : subtitle ? (
            <section
              aria-label="实时字幕"
              className="rounded-xl border-4 border-black bg-spaceDark p-3"
            >
              <div className="mb-2 flex items-center gap-2">
                <span
                  className={`rounded-md border-2 border-black px-2 py-0.5 text-[11px] font-bold ${SPEAKER_CHIP_CLASS[subtitle.speaker]}`}
                >
                  {SPEAKER_LABEL[subtitle.speaker]}
                </span>
                <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
                  中文
                </span>
                <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
                  English
                </span>
              </div>
              <div className="space-y-1 text-[15px] font-semibold leading-relaxed text-white">
                {subtitle.zh ? (
                  <TypedLine key={`${subtitle.id}-zh`} text={subtitle.zh} />
                ) : null}
                {subtitle.en ? (
                  <TypedLine key={`${subtitle.id}-en`} text={subtitle.en} />
                ) : null}
              </div>
            </section>
          ) : (
            <section
              aria-label="等待字幕"
              className="mt-16 flex flex-col items-center gap-2 text-center"
            >
              <span className="flex h-12 w-12 items-center justify-center rounded-xl border-4 border-black bg-portalGreen text-black">
                <FontAwesomeIcon icon={faClosedCaptioning} aria-hidden="true" />
              </span>
              <p className="text-sm font-bold text-white">等待语音输入</p>
              <p className="text-xs text-gray-400">模拟会话开始后，双语字幕将显示在这里</p>
            </section>
          )}
        </main>
      </div>
    </div>
  );
}
