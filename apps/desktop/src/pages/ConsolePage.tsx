import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  faBook,
  faFilePdf,
  faForwardStep,
  faMicrophone,
  faRotate,
  faRotateLeft,
  faWaveSquare,
} from '@fortawesome/free-solid-svg-icons';
import ConfirmModal from '../components/ConfirmModal';
import HeaderBar from '../components/HeaderBar';
import NexTalkBrand from '../components/NexTalkBrand';
import StealthCard from '../components/StealthCard';
import QrCodeCard from '../components/QrCodeCard';
import KnowledgeRow from '../components/KnowledgeRow';
import NeobrutalismButton from '../components/NeobrutalismButton';
import ErrorBanner from '../components/ErrorBanner';
import { useTauriEvents } from '../hooks/useTauriEvents';
import { MOCK_GLOSSARY_TERMS, MOCK_RESUME } from '../data/mock-data';

/**
 * Console page (340x680) — the hub window (DSK-01).
 *
 * Fixed card order per the UI-SPEC Navigation contract: brand header →
 * StealthCard → sync QR → knowledge base rows (简历导入 / 术语表 / 音色注册)
 * → 本地资产 (录音资产 / 复盘报告), with the action bar pinned at the bottom
 * (开始模拟会话 / 扩展视图 / 设置).
 *
 * The widget is the only entry point for every other surface: the dual-pane
 * window opens through the Tauri window API, and each missing page opens as a
 * full-width view inside this window.
 *
 * The action bar is driven by the session status Rust publishes (UI-02):
 * 开始模拟会话 becomes the live 会话进行中 / 回答生成中 state plus 停止 (behind
 * the locked confirmation), 打断 / 重听 (D-03) appear alongside it, and
 * 扩展视图 only opens once there is a session worth extending.
 */
export default function ConsolePage() {
  const navigate = useNavigate();
  const { status } = useTauriEvents();
  const [startFailed, setStartFailed] = useState(false);
  const [dualFailed, setDualFailed] = useState(false);
  const [confirmStop, setConfirmStop] = useState(false);

  const active = status === 'listening' || status === 'generating';
  const generating = status === 'generating';

  const startSession = () => {
    setStartFailed(false);
    invoke('start_session').catch((err) => {
      console.error('start_session failed', err);
      setStartFailed(true);
    });
  };

  const control = (command: 'interrupt' | 'repeat') => {
    invoke(command).catch((err) => {
      // The phase moved on between the click and the command — nothing to do.
      console.error(`${command} failed`, err);
    });
  };

  const stopSession = () => {
    setConfirmStop(false);
    invoke('stop_session').catch((err) => console.error('stop_session failed', err));
  };

  const openDualPane = async () => {
    setDualFailed(false);
    try {
      // The dual window starts hidden (tauri.conf.json) and is revealed here.
      // Its own header exposes 关闭, which destroys it — getByLabel then
      // returns null and showing is impossible, so recreate it instead of
      // leaving 扩展视图 dead for the rest of the process (WR-08).
      const existing = await WebviewWindow.getByLabel('dual');
      if (existing) {
        await existing.show();
        return;
      }
      const dual = new WebviewWindow('dual', {
        // Mirrors the `dual` entry in tauri.conf.json.
        url: 'index.html#/dual',
        width: 860,
        height: 680,
        resizable: false,
        maximizable: false,
        decorations: false,
        transparent: true,
        shadow: false,
        visible: true,
        center: true,
      });
      await new Promise<void>((resolve, reject) => {
        dual.once('tauri://created', () => resolve());
        dual.once('tauri://error', (event) => reject(new Error(String(event.payload))));
      });
    } catch (err) {
      console.error('opening the dual window failed', err);
      setDualFailed(true);
    }
  };

  return (
    <div className="dot-matrix-root flex h-full w-full flex-col overflow-hidden rounded-3xl border-4 border-black shadow-cartoon-green">
      <HeaderBar tone="green">
        <NexTalkBrand xColor="ink" subtitle="极言" />
      </HeaderBar>

      <main className="flex-1 space-y-4 overflow-y-auto p-4">
        <StealthCard />

        <QrCodeCard />

        <section aria-label="知识库" className="space-y-3">
          <h2 className="text-xs font-bold uppercase tracking-wider text-gray-400">知识库</h2>
          <KnowledgeRow
            icon={faFilePdf}
            label={MOCK_RESUME.fileName}
            state="success"
            onClick={() => navigate('/resume')}
          />
          <KnowledgeRow
            icon={faBook}
            label="术语表"
            value={`${MOCK_GLOSSARY_TERMS.length} 个术语`}
            onClick={() => navigate('/glossary')}
          />
          <KnowledgeRow
            icon={faMicrophone}
            label="音色注册"
            value="未注册"
            onClick={() => navigate('/voice')}
          />
        </section>

        <section aria-label="本地资产" className="space-y-3">
          <h2 className="text-xs font-bold uppercase tracking-wider text-gray-400">本地资产</h2>
          <KnowledgeRow
            icon={faWaveSquare}
            label="录音资产"
            onClick={() => navigate('/recordings')}
          />
          <KnowledgeRow icon={faRotate} label="复盘报告" onClick={() => navigate('/review')} />
        </section>
      </main>

      {startFailed ? (
        <div className="px-4 pb-2">
          <ErrorBanner
            title="模拟音频加载失败"
            body="请重新开始模拟会话"
            action={
              <NeobrutalismButton variant="paper" size="sm" onClick={startSession}>
                重试
              </NeobrutalismButton>
            }
          />
        </div>
      ) : null}

      {dualFailed ? (
        <div className="px-4 pb-2">
          <ErrorBanner
            title="扩展视图打开失败"
            body="请重试"
            action={
              <NeobrutalismButton variant="paper" size="sm" onClick={openDualPane}>
                重试
              </NeobrutalismButton>
            }
          />
        </div>
      ) : null}

      <footer className="shrink-0 space-y-2 border-t-4 border-black bg-spaceDark p-3">
        {active ? (
          <>
            <div className="flex gap-2">
              <NeobrutalismButton
                variant="green"
                className="flex-1"
                disabled
                title="会话已开始，停止后可重新开始"
              >
                {generating ? '回答生成中' : '会话进行中'}
              </NeobrutalismButton>
              <NeobrutalismButton variant="red" onClick={() => setConfirmStop(true)}>
                停止
              </NeobrutalismButton>
            </div>
            <div className="flex gap-2">
              <NeobrutalismButton
                variant="paper"
                size="sm"
                className="flex-1"
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
                className="flex-1"
                disabled={!generating}
                title={generating ? undefined : '回答生成中才可重听'}
                onClick={() => control('repeat')}
              >
                <FontAwesomeIcon icon={faRotateLeft} aria-hidden="true" />
                重听
              </NeobrutalismButton>
            </div>
          </>
        ) : (
          <NeobrutalismButton variant="green" className="w-full" onClick={startSession}>
            开始模拟会话
          </NeobrutalismButton>
        )}

        <div className="flex gap-2">
          <NeobrutalismButton
            variant="blue"
            className="flex-1"
            onClick={openDualPane}
            disabled={!active}
            title={active ? undefined : '开始模拟会话后可打开扩展视图'}
          >
            扩展视图
          </NeobrutalismButton>
          <NeobrutalismButton variant="ghost" onClick={() => navigate('/setup')}>
            设置
          </NeobrutalismButton>
        </div>
      </footer>

      <ConfirmModal
        open={confirmStop}
        title="停止会话？"
        body="当前字幕与策略将清空"
        confirmLabel="停止"
        onCancel={() => setConfirmStop(false)}
        onConfirm={stopSession}
      />
    </div>
  );
}
