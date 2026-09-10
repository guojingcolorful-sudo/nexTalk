import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { faBrain, faClosedCaptioning } from '@fortawesome/free-solid-svg-icons';
import type { LanguagePref, ServerEvent } from '@nextalk/protocol';
import ChatBubble from '../components/ChatBubble';
import EmptyState from '../components/EmptyState';
import GateScreen from '../components/GateScreen';
import MobileTabs, { type PhoneTab } from '../components/MobileTabs';
import StatusCapsule, { type CapsuleStatus } from '../components/StatusCapsule';
import StrategyCard from '../components/StrategyCard';
import TypewriterDots from '../components/TypewriterDots';
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
function capsuleStatus(state: WsConnectionState): CapsuleStatus {
  switch (state) {
    case 'open':
      return 'connected';
    case 'connecting':
      return 'connecting';
    case 'closed':
      return 'closed';
    default:
      return 'reconnecting';
  }
}

/** True while the desktop is still producing the newest line (dots at stream end). */
function isGenerating(events: ServerEvent[]): boolean {
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    if (event.t === 'status') return event.session === 'generating';
    if (event.t === 'subtitle') return !event.final;
  }
  return false;
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
  const { events, state } = useWs(ticket);
  const [tab, setTab] = useState<PhoneTab>(readTabFromUrl);
  const [sessionActive, setSessionActive] = useState(false);
  const [languagePref, setLanguagePref] = useState<LanguagePref>('bilingual');
  const streamEndRef = useRef<HTMLDivElement | null>(null);

  const subtitles = useMemo(
    () => events.filter((event): event is SubtitleEvent => event.t === 'subtitle'),
    [events],
  );
  const strategies = useMemo(
    () => events.filter((event): event is StrategyEvent => event.t === 'strategy'),
    [events],
  );
  const generating = useMemo(() => isGenerating(events), [events]);

  const changeTab = useCallback((next: PhoneTab) => {
    setTab(next);
    writeTabToUrl(next);
  }, []);

  const toggleSession = useCallback(() => {
    setSessionActive((active) => !active);
  }, []);

  const cycleLanguage = useCallback(() => {
    setLanguagePref((pref) => nextLanguagePref(pref));
  }, []);

  // Auto-scroll on new content only — no scroll listeners, no hijacking
  // (UI-SPEC Motion Contract).
  useEffect(() => {
    streamEndRef.current?.scrollIntoView?.({ block: 'nearest' });
  }, [subtitles.length, strategies.length, tab]);

  return (
    <div className="flex h-full justify-center">
      <div className="dot-matrix-root flex h-full w-full max-w-[390px] flex-col overflow-hidden">
        {/* Status bar: one capsule + the pairing stamp */}
        <header className="flex shrink-0 items-center justify-between gap-2 border-b-4 border-black bg-panel px-4 py-3">
          <StatusCapsule status={capsuleStatus(state)} />
          <span className="text-xs font-bold tracking-wider text-gray-400">已配对桌面端</span>
        </header>

        <MobileTabs value={tab} onChange={changeTab} />

        <main className="flex-1 overflow-y-auto px-4 pt-3 pb-4">
          {tab === 'subs' ? (
            <section
              id="panel-subs"
              role="tabpanel"
              aria-labelledby="tab-subs"
              aria-live="polite"
              className="flex flex-col gap-4"
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
                subtitles.map((subtitle) => (
                  <ChatBubble
                    key={`${subtitle.id}-${subtitle.seq}`}
                    speaker={subtitle.speaker}
                    zh={subtitle.zh}
                    en={subtitle.en}
                  />
                ))
              )}
              {generating ? <TypewriterDots /> : null}
            </section>
          ) : (
            <section
              id="panel-ai"
              role="tabpanel"
              aria-labelledby="tab-ai"
              aria-live="polite"
              className="flex flex-col gap-4"
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
                  <StrategyCard
                    key={strategy.id}
                    title={strategy.title}
                    bullets={strategy.bullets}
                    roundId={strategy.roundId}
                  />
                ))
              )}
            </section>
          )}
          <div ref={streamEndRef} aria-hidden="true" />
        </main>

        <GateScreen
          sessionActive={sessionActive}
          languagePref={languagePref}
          onToggleSession={toggleSession}
          onCycleLanguage={cycleLanguage}
        />
      </div>
    </div>
  );
}
