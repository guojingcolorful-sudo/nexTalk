import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import {
  faBook,
  faFilePdf,
  faMicrophone,
  faRotate,
  faWaveSquare,
} from '@fortawesome/free-solid-svg-icons';
import HeaderBar from '../components/HeaderBar';
import NexTalkBrand from '../components/NexTalkBrand';
import StealthCard from '../components/StealthCard';
import QrCodeCard from '../components/QrCodeCard';
import KnowledgeRow from '../components/KnowledgeRow';
import NeobrutalismButton from '../components/NeobrutalismButton';
import ErrorBanner from '../components/ErrorBanner';
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
 */
export default function ConsolePage() {
  const navigate = useNavigate();
  const [sessionRunning, setSessionRunning] = useState(false);
  const [startFailed, setStartFailed] = useState(false);

  const startSession = () => {
    setStartFailed(false);
    invoke('start_session')
      .then(() => setSessionRunning(true))
      .catch((err) => {
        console.error('start_session failed', err);
        setStartFailed(true);
      });
  };

  const openDualPane = async () => {
    try {
      // The dual window starts hidden (tauri.conf.json) and is revealed here.
      const dual = await WebviewWindow.getByLabel('dual');
      await dual?.show();
    } catch (err) {
      console.error('showing the dual window failed', err);
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

      <footer className="shrink-0 space-y-2 border-t-4 border-black bg-spaceDark p-3">
        <NeobrutalismButton
          variant="green"
          className="w-full"
          onClick={startSession}
          disabled={sessionRunning}
        >
          {sessionRunning ? '会话进行中' : '开始模拟会话'}
        </NeobrutalismButton>
        <div className="flex gap-2">
          <NeobrutalismButton variant="blue" className="flex-1" onClick={openDualPane}>
            扩展视图
          </NeobrutalismButton>
          <NeobrutalismButton variant="ghost" onClick={() => navigate('/setup')}>
            设置
          </NeobrutalismButton>
        </div>
      </footer>
    </div>
  );
}
