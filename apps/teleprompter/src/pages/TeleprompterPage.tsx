import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { faBrain, faClosedCaptioning } from '@fortawesome/free-solid-svg-icons';
import type { LanguagePref, ServerEvent } from '@nextalk/protocol';
import ChatBubble from '../components/ChatBubble';
import EmptyState from '../components/EmptyState';
import ErrorBanner from '../components/ErrorBanner';
import GateScreen from '../components/GateScreen';
import MobileTabs, { type PhoneTab } from '../components/MobileTabs';
import StatusCapsule, { type CapsuleStatus } from '../components/StatusCapsule';
import StrategyCard from '../components/StrategyCard';
import ThinkingCard from '../components/ThinkingCard';
import Toast from '../components/Toast';
import TypewriterDots from '../components/TypewriterDots';
import { useCenterAnchor } from '../hooks/useCenterAnchor';
import { useWakeLock } from '../hooks/useWakeLock';
import { useWs, type WsConnectionState, type WsTicket } from '../hooks/useWs';

/**
 * The phone teleprompter surface (01-04): status capsule + 字幕 / AI 辅助 tabs
 * over the live LAN stream, with the 开始提词 gate at the bottom.
 *
 * Tab selection is URL state (`?tab=ai`) merged with the pairing `?token=` —
 * a reload (or a phone waking from sleep) lands the user back on the tab they
 * were reading, and the link stays shareable inside the same session.
 */

const TAB_PARAM = 'tab';

type SubtitleEvent = Extract<ServerEvent, { t: 'subtitle' }>;
type StrategyEvent = Extract<ServerEvent, { t: 'strategy' }>;

function readTabFromUrl(): PhoneTab {
  return new URLSearchParams(window.location.search).get(TAB_PARAM) === 'ai' ? 'ai' : 'subs';
}

function writeTabToUrl(tab: PhoneTab): void {
  const url = new URL(window.location.href);
  if (tab === 'ai') url.searchParams.set(TAB_PARAM, 'ai');
  else url.searchParams.delete(TAB_PARAM);
  window.history.replaceState(null, '', url.toString());
}

/** Capsule copy is keyed to the WS lifecycle; reconnecting is the backoff state. */
const CAPSULE_STATUS: Record<WsConnectionState, CapsuleStatus> = {
  connecting: 'connecting',
  connected: 'connected',
  reconnecting: 'reconnecting',
  closed: 'closed',
};

/** True while the desktop is still producing the newest line (dots at stream end). */
function isGenerating(events: ServerEvent[]): boolean {
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    if (event.t === 'status') return event.session === 'generating';
    if (event.t === 'subtitle') return !event.final;
  }
  return false;
}

/**
 * UAT-12: true while the newest content is an interviewer question that has
 * no strategy card yet — the window in which the AI is "thinking". The AI
 * tab renders the 思考中 indicator for as long as this holds (the desktop
 * hosts the mirror in apps/desktop/src/components/AiTimeline.tsx).
 */
export function isAiThinking(events: ServerEvent[]): boolean {
  let lastQuestionRound: string | null = null;
  let lastStrategyRound: string | null = null;
  for (const event of events) {
    if (event.t === 'subtitle' && event.speaker === 'interviewer') {
      lastQuestionRound = event.id.replace(/-q$/, '');
    } else if (event.t === 'strategy') {
      lastStrategyRound = event.roundId;
    }
  }
  if (lastQuestionRound === null) return false;
  return lastStrategyRound !== lastQuestionRound;
}

const NEXT_LANGUAGE: Record<LanguagePref, LanguagePref> = {
  'all-zh': 'all-en',
  'all-en': 'bilingual',
  bilingual: 'all-zh',
};

/** Cycle order follows the locked segments: 中 → EN → EN+中 → 中. */
export function nextLanguagePref(pref: LanguagePref): LanguagePref {
  return NEXT_LANGUAGE[pref];
}

interface TeleprompterPageProps {
  ticket: WsTicket;
}

export default function TeleprompterPage({ ticket }: TeleprompterPageProps) {
  const { events, state, stale, sendLanguagePref, sendSessionAction } = useWs(ticket);
  const [tab, setTab] = useState<PhoneTab>(readTabFromUrl);
  const [sessionActive, setSessionActive] = useState(false);
  const [languagePref, setLanguagePref] = useState<LanguagePref>('bilingual');
  const [toast, setToast] = useState<string | null>(null);

  // SYNC-04: the 开始提词 tap is the gesture the wake lock needs on a plain
  // http:// LAN origin, so the hook is engaged from the same handler.
  const { isWakeActive, activate: activateWakeLock, deactivate: deactivateWakeLock } =
    useWakeLock({ onFallbackEngaged: () => setToast('已启用防休眠回退模式') });

  const subtitles = useMemo(
    () => events.filter((event): event is SubtitleEvent => event.t === 'subtitle'),
    [events],
  );
  const strategies = useMemo(
    () => events.filter((event): event is StrategyEvent => event.t === 'strategy'),
    [events],
  );
  const generating = useMemo(() => isGenerating(events), [events]);
  // UAT-12: the AI tab shows the thinking state while the newest question
  // awaits its strategy card.
  const aiThinking = useMemo(() => isAiThinking(events), [events]);

  // UAT-13: the scroll anchor is the NEWEST subtitle — the latest question
  // and then its answer each take the center as they arrive, and older
  // content moves up in real time. Until a subtitle exists nothing anchors.
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const anchorId = subtitles.length > 0 ? subtitles[subtitles.length - 1].id : null;
  // UAT-14: on the AI tab the 思考中 card (or the newest strategy card) takes
  // the center — the next question's thinking is on screen immediately.
  const strategyAnchorRef = useRef<HTMLDivElement | null>(null);
  const thinkingAnchorRef = useRef<HTMLDivElement | null>(null);
  const lastStrategyId = strategies.length > 0 ? strategies[strategies.length - 1].id : null;

  // UAT-5 bidirectional: the phone's gate mirrors the DESKTOP's session —
  // when the desktop starts 开始模拟会话 on its own, the phone flips to the
  // live state too (and an ended session flips it back).
  const liveSession = useMemo(() => {
    for (let i = events.length - 1; i >= 0; i -= 1) {
      const event = events[i];
      if (event.t === 'status') {
        return event.session === 'listening' || event.session === 'generating';
      }
    }
    return false;
  }, [events]);

  useEffect(() => {
    setSessionActive(liveSession);
  }, [liveSession]);

  // WR-03: the desktop echoes the applied mode back on the same stream, so
  // that echo — not the local optimistic guess — is the source of truth. A
  // reload, a wake-from-sleep or a second phone otherwise renders a mode the
  // session is not in, and the next tap sends a value derived from that base.
  const echoedLanguage = useMemo<LanguagePref | null>(() => {
    for (let i = events.length - 1; i >= 0; i -= 1) {
      const event = events[i];
      if (event.t === 'language') return event.language;
    }
    return null;
  }, [events]);

  useEffect(() => {
    if (echoedLanguage !== null) setLanguagePref(echoedLanguage);
  }, [echoedLanguage]);

  const changeTab = useCallback((next: PhoneTab) => {
    setTab(next);
    writeTabToUrl(next);
  }, []);

  const toggleSession = useCallback(() => {
    if (sessionActive) {
      deactivateWakeLock();
      // UAT-5 bidirectional: 暂停提词 is the same function as the desktop's
      // 停止 — the session ends everywhere (the desktop status event flips
      // this gate back).
      setSessionActive(false);
      sendSessionAction('stop_session');
      return;
    }
    activateWakeLock();
    setSessionActive(true);
    // SYNC-01 round-trip (UAT-5): 开始提词 is the same function as the
    // desktop's 开始模拟会话 — the desktop starts the sim and opens 扩展视图.
    sendSessionAction('start_session');
  }, [sessionActive, activateWakeLock, deactivateWakeLock, sendSessionAction]);

  const cycleLanguage = useCallback(() => {
    const next = nextLanguagePref(languagePref);
    setLanguagePref(next);
    // SYNC-03: the phone owns the session mode; the desktop applies what we push.
    sendLanguagePref(next);
  }, [languagePref, sendLanguagePref]);

  // Auto-scroll on new content only — no scroll listeners, no hijacking
  // (UI-SPEC Motion Contract). UAT-13/14/15: the anchors stay centered WHILE
  // their content grows — the ResizeObserver re-centers on every typed
  // character, so the newest question, thinking text and answer never leave
  // the middle of the screen.
  useCenterAnchor(anchorRef, tab !== 'ai' && anchorId);
  useCenterAnchor(thinkingAnchorRef, tab === 'ai' && aiThinking);
  useCenterAnchor(strategyAnchorRef, tab === 'ai' && !aiThinking && lastStrategyId);

  return (
    <div className="flex h-full justify-center">
      <div className="dot-matrix-root flex h-full w-full max-w-[390px] flex-col overflow-hidden">
        {/* Status bar: one capsule + the pairing stamp */}
        <header className="flex shrink-0 items-center justify-between gap-2 border-b-4 border-black bg-panel px-4 py-3">
          <StatusCapsule status={CAPSULE_STATUS[state]} />
          <span className="text-xs font-bold tracking-wider text-gray-400">已配对桌面端</span>
        </header>

        <MobileTabs value={tab} onChange={changeTab} />

        <main className="flex-1 overflow-y-auto px-4 pt-3 pb-4">
          {/* UAT-13: both panels stay MOUNTED across tab switches (the
              inactive one is display:none via the `hidden` class) — content
              that already revealed itself never re-runs its animation. */}
          <section
            id="panel-subs"
            role="tabpanel"
            aria-labelledby="tab-subs"
            aria-live="polite"
            className={`flex flex-col gap-4 ${tab !== 'subs' ? 'hidden' : ''}`}
          >
            {subtitles.length === 0 ? (
              <EmptyState
                icon={faClosedCaptioning}
                tone="green"
                title="等待语音输入"
                body="模拟会话开始后，双语字幕将显示在这里"
                className="mt-12"
              />
            ) : (
              subtitles.map((subtitle, index) => (
                <div
                  key={`${subtitle.id}-${subtitle.seq}`}
                  ref={subtitle.id === anchorId ? anchorRef : null}
                >
                  <ChatBubble
                    speaker={subtitle.speaker}
                    zh={subtitle.zh}
                    en={subtitle.en}
                    language={languagePref}
                    instant={index < subtitles.length - 1}
                  />
                </div>
              ))
            )}
            {generating ? <TypewriterDots /> : null}
          </section>
          <section
            id="panel-ai"
            role="tabpanel"
            aria-labelledby="tab-ai"
            aria-live="polite"
            className={`flex flex-col gap-4 ${tab !== 'ai' ? 'hidden' : ''}`}
          >
            {strategies.length === 0 ? (
              <EmptyState
                icon={faBrain}
                tone="yellow"
                title="AI 策略将自动生成"
                body="提问结束后，策略卡片会出现在这里"
                className="mt-12"
              />
            ) : (
              strategies.map((strategy) => (
                <div
                  key={strategy.id}
                  ref={strategy.id === lastStrategyId ? strategyAnchorRef : null}
                >
                  <StrategyCard
                    title={strategy.title}
                    bullets={strategy.bullets}
                    roundId={strategy.roundId}
                    answerZh={strategy.answerZh}
                    answerEn={strategy.answerEn}
                  />
                </div>
              ))
            )}
            {aiThinking ? <ThinkingCard nodeRef={thinkingAnchorRef} /> : null}
          </section>
        </main>

        <GateScreen
          sessionActive={sessionActive}
          wakeActive={isWakeActive}
          languagePref={languagePref}
          onToggleSession={toggleSession}
          onCycleLanguage={cycleLanguage}
        />

        {state === 'reconnecting' && stale ? (
          <ErrorBanner
            tone="red"
            title="连接失败"
            body="请重新扫描桌面二维码"
            className="mx-4 mb-3"
          />
        ) : null}
      </div>

      {toast ? <Toast message={toast} onDismiss={() => setToast(null)} /> : null}
    </div>
  );
}
