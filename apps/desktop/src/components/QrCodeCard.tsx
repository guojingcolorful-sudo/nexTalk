import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faQrcode } from '@fortawesome/free-solid-svg-icons';
import QRCode from 'qrcode';
import ErrorBanner from './ErrorBanner';
import NeobrutalismButton from './NeobrutalismButton';
import { useTauriEvents } from '../hooks/useTauriEvents';

interface PairingInfo {
  url: string;
  port: number;
}

/**
 * QrCodeCard — the live phone-pairing QR (DSK-01).
 *
 * Invokes `get_pairing_info` and renders the pairing URL —
 * `http://{lan_ip}:8787/?token={32-hex}` (threat T-01-01) — as a 144px QR
 * via the `qrcode` package. While the invoke resolves it shows the icon tile
 * (never a broken state); if it fails (LAN server bind failure, or the UI
 * running in a plain browser preview) it shows the 配对失败 banner with a
 * 重试 control instead of an unreadable card.
 *
 * The status line reads `phone_count` telemetry: until 01-05 emits it the
 * count stays unknown and the line shows 等待扫码 rather than a fabricated
 * zero.
 */
export default function QrCodeCard() {
  const { phoneCount } = useTauriEvents();
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let alive = true;
    invoke<PairingInfo>('get_pairing_info')
      .then((info) => QRCode.toDataURL(info.url, { width: 144, margin: 1 }))
      .then((url) => {
        if (alive) setDataUrl(url);
      })
      .catch((err) => {
        console.error('get_pairing_info failed', err);
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [attempt]);

  const connected = phoneCount !== null && phoneCount > 0;

  const retry = () => {
    setFailed(false);
    setAttempt((n) => n + 1);
  };

  return (
    <section
      aria-label="同步手机"
      className="flex flex-col items-center rounded-xl border-4 border-portalGreen bg-darkerSpace p-4"
    >
      <div className="mb-3 h-[152px] w-[152px] overflow-hidden rounded-xl border-4 border-black bg-white">
        {dataUrl ? (
          <img src={dataUrl} width={144} height={144} alt="手机配对二维码" />
        ) : (
          <div className="flex h-full w-full items-center justify-center">
            <FontAwesomeIcon icon={faQrcode} className="text-6xl text-black" />
          </div>
        )}
      </div>

      {failed ? (
        <ErrorBanner
          tone="red"
          title="配对失败"
          body="请确认手机与电脑连接同一 Wi-Fi，然后重新扫码"
          className="w-full"
          action={
            <NeobrutalismButton variant="paper" size="sm" onClick={retry}>
              重试
            </NeobrutalismButton>
          }
        />
      ) : (
        <>
          <p className="text-[10px] font-bold text-gray-400">扫码开启手机跨端辅助展示</p>
          <p
            aria-live="polite"
            className="mt-2 inline-flex items-center gap-1 rounded-full border-2 border-black bg-spaceDark px-2 py-0.5 text-[10px] font-bold text-gray-400"
          >
            <span
              aria-hidden="true"
              className={`h-2 w-2 rounded-full ${connected ? 'bg-portalGreen' : 'bg-gray-500'}`}
            />
            {connected ? `已连接 ${phoneCount} 台设备` : '等待扫码'}
          </p>
        </>
      )}
    </section>
  );
}
