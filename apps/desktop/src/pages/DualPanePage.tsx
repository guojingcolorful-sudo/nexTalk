import { useEffect, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  faBolt,
  faBrain,
  faClosedCaptioning,
  faForwardStep,
  faLightbulb,
  faRotateLeft,
} from '@fortawesome/free-solid-svg-icons';
import type { ServerEvent } from '@nextalk/protocol';
import AiTimeline, { toTimelineItems } from '../components/AiTimeline';
import ChatBubble from '../components/ChatBubble';
import EmptyState from '../components/EmptyState';
import HeaderBar from '../components/HeaderBar';
import MicStatusPill from '../components/MicStatusPill';
import NeobrutalismButton from '../components/NeobrutalismButton';
import PanelHeader from '../components/PanelHeader';
import TypewriterDots from '../components/TypewriterDots';
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
 *
 * 打断 / 重听 (D-03) only exist while an answer is generating — outside that
 * phase the commands are rejected server-side, so the buttons do not offer it.
 */
export default function DualPanePage() {
  const { events, status, languageMode } = useTauriEvents();

  const subtitles = useMemo(
    () => events.filter((event): event is SubtitleEvent => event.t === 'subtitle'),
    [events],
  );
  const timelineItems = useMemo(() => toTimelineItems(events), [events]);

  const generating = status === 'generating';
  const listening = status === 'listening' || generating;

  const control = (command: 'interrupt' | 'repeat') => {
    invoke(command).catch((err) => {
      // The phase moved on between the click and the command — nothing to do.
      console.error(`${command} failed`, err);
    });
  };

  const streamEndRef = useRef<HTMLDivElement>(null);
  // `.at(-1)` is Safari 15.4+; macOS 12.0-12.2 ships 15.0-15.3 (WR-06).
  const lastSubtitleId =
    subtitles.length > 0 ? subtitles[subtitles.length - 1].id : null;
  useEffect(() => {
    if (lastSubtitleId === null) return;
    // Motion contract: follow new lines only, block: 'nearest' + behavior
    // 'auto' so the jump is never animated.
    streamEndRef.current?.scrollIntoView({ block: 'nearest', behavior: 'auto' });
  }, [lastSubtitleId]);

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-blue">
      <HeaderBar
        tone="blue"
        title="扩展视图"
        actions={
          listening ? (
            <div className="flex items-center gap-2">
              <MicStatusPill />
              <div className="flex gap-1">
                <NeobrutalismButton
                  variant="paper"
                  size="sm"
                  disabled={!generating}
                  title={generating ? undefined : '回答生成中才可打断'}
                  onClick={() => control('interrupt')}
                >
                  <FontAwesomeIcon icon={faForwardStep} aria-hidden="true" />
                  打断
                </NeobrutalismButton>
                <NeobrutalismButton
                  variant="paper"
                  size="sm"
                  disabled={!generating}
                  title={generating ? undefined : '回答生成中才可重听'}
                  onClick={() => control('repeat')}
                >
                  <FontAwesomeIcon icon={faRotateLeft} aria-hidden="true" />
                  重听
                </NeobrutalismButton>
              </div>
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
            {subtitles.map((subtitle) => (
              <ChatBubble
                key={subtitle.id}
                speaker={subtitle.speaker}
                zh={subtitle.zh}
                en={subtitle.en}
                mode={languageMode}
              />
            ))}
            {generating ? <TypewriterDots /> : null}
            {subtitles.length === 0 ? (
              <EmptyState
                icon={faClosedCaptioning}
                title="等待语音输入"
                body="模拟会话开始后，双语字幕将显示在这里"
              />
            ) : null}
            <div ref={streamEndRef} />
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
              <AiTimeline items={timelineItems} />
            ) : (
              <EmptyState
                icon={faLightbulb}
                tone="yellow"
                title="AI 策略将自动生成"
                body="提问结束后，策略卡片会出现在这里"
              />
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
