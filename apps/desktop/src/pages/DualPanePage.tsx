import { useEffect, useMemo, useRef } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  faBolt,
  faBrain,
  faClosedCaptioning,
  faLightbulb,
} from '@fortawesome/free-solid-svg-icons';
import type { ServerEvent } from '@nextalk/protocol';
import AiTimeline, { isAiThinking, toTimelineItems } from '../components/AiTimeline';
import ChatBubble from '../components/ChatBubble';
import EmptyState from '../components/EmptyState';
import HeaderBar from '../components/HeaderBar';
import MicStatusPill from '../components/MicStatusPill';
import PanelHeader from '../components/PanelHeader';
import TypewriterDots from '../components/TypewriterDots';
import ThinkingCard from '../components/ThinkingCard';
import { useCenterAnchor } from '../hooks/useCenterAnchor';
import { useTauriEvents } from '../hooks/useTauriEvents';

type SubtitleEvent = Extract<ServerEvent, { t: 'subtitle' }>;

/**
 * DualPanePage (860x680) — the extended view (DSK-02 / DSK-04): live
 * subtitle stream on the left, AI timeline on the right.
 *
 * Rust is the single source of truth: both panes read the same narrowed
 * `session` stream (useTauriEvents applies isServerEvent before anything can
 * reach React state), so a malformed payload renders nothing here. Per-bubble
 * language choice is local UI state; the session mode the phone applies
 * (SYNC-03) arrives on the same stream and seeds every untouched bubble.
 */
export default function DualPanePage() {
  const { events, status, languageMode } = useTauriEvents();

  const subtitles = useMemo(
    () => events.filter((event): event is SubtitleEvent => event.t === 'subtitle'),
    [events],
  );
  const timelineItems = useMemo(() => toTimelineItems(events), [events]);
  // UAT-12: the AI pane shows the thinking state for as long as the newest
  // question has no strategy card yet.
  const aiThinking = useMemo(() => isAiThinking(events), [events]);

  const generating = status === 'generating';
  const listening = status === 'listening' || generating;

  // UAT-9/13: the newest content of BOTH panes must stay in the MIDDLE of the
  // viewport, not at the bottom edge — the reader's eye never chases content
  // and nothing is ever occluded. The subtitle anchor is the NEWEST subtitle:
  // the latest question and then its answer each take the center as they
  // arrive, and older content moves up in real time.
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const lastTimelineRef = useRef<HTMLDivElement | null>(null);
  // `.at(-1)` is Safari 15.4+; macOS 12.0-12.2 ships 15.0-15.3 (WR-06).
  const anchorId = subtitles.length > 0 ? subtitles[subtitles.length - 1].id : null;
  const lastTimelineId =
    timelineItems.length > 0 ? timelineItems[timelineItems.length - 1].id : null;
  // UAT-14/15: the anchors stay centered WHILE their content grows — the
  // ResizeObserver re-centers on every typed character, so the newest
  // question, thinking text and answer never leave the middle of the screen.
  const thinkingRef = useRef<HTMLDivElement | null>(null);
  useCenterAnchor(anchorRef, anchorId);
  useCenterAnchor(thinkingRef, aiThinking);
  useCenterAnchor(lastTimelineRef, !aiThinking && lastTimelineId !== null && lastTimelineId);

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-blue">
      <HeaderBar
        tone="blue"
        title="扩展视图"
        actions={
          listening ? (
            <div className="flex items-center gap-2">
              <MicStatusPill />
            </div>
          ) : null
        }
      />

      <div className="flex flex-1 overflow-hidden">
        <section
          aria-label="实时字幕"
          className="flex w-1/2 flex-col border-r-4 border-black bg-spaceDark"
        >
          <PanelHeader tone="gray" icon={faClosedCaptioning} title="实时字幕" />
          <div
            aria-live="polite"
            className="flex flex-1 flex-col gap-6 overflow-y-auto p-4"
            data-testid="subtitle-stream"
          >
            {subtitles.map((subtitle, index) => (
              <div
                key={subtitle.id}
                ref={subtitle.id === anchorId ? anchorRef : null}
              >
                <ChatBubble
                  speaker={subtitle.speaker}
                  zh={subtitle.zh}
                  en={subtitle.en}
                  mode={languageMode}
                  instant={index < subtitles.length - 1}
                />
              </div>
            ))}
            {generating ? <TypewriterDots /> : null}
            {subtitles.length === 0 ? (
              <EmptyState
                icon={faClosedCaptioning}
                title="等待语音输入"
                body="模拟会话开始后，双语字幕将显示在这里"
              />
            ) : null}
          </div>
        </section>

        <section aria-label="AI 辅助" className="flex w-1/2 flex-col bg-panel">
          <PanelHeader
            tone="yellow"
            icon={faBrain}
            title="AI 辅助"
            trailing={
              generating ? (
                <FontAwesomeIcon
                  icon={faBolt}
                  aria-hidden="true"
                  className="animate-pulse text-[10px] text-black motion-reduce:animate-none"
                />
              ) : null
            }
          />
          <div
            aria-live="polite"
            className="flex-1 overflow-y-auto p-4"
            data-testid="ai-timeline"
          >
            {timelineItems.length > 0 ? (
              <AiTimeline items={timelineItems} lastItemRef={lastTimelineRef} />
            ) : (
              <EmptyState
                icon={faLightbulb}
                tone="yellow"
                title="AI 策略将自动生成"
                body="提问结束后，策略卡片会出现在这里"
              />
            )}
            {aiThinking ? <ThinkingCard nodeRef={thinkingRef} /> : null}
          </div>
        </section>
      </div>
    </div>
  );
}
