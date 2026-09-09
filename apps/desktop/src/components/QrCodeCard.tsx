import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faQrcode } from '@fortawesome/free-solid-svg-icons';
import QRCode from 'qrcode';

interface PairingInfo {
  url: string;
  port: number;
}

/**
 * QrCodeCard — the live phone-pairing QR (DSK-01, SYNC-02 skeleton slice).
 *
 * Invokes `get_pairing_info` on mount and renders the pairing URL —
 * `http://{lan_ip}:8787/?token={32-hex}` (threat T-01-01) — as a 144px QR
 * via the `qrcode` package. While the invoke resolves it shows the icon
 * tile (never a broken state); if the invoke fails (e.g. running the
 * desktop UI in a plain browser preview) it degrades to an offline notice.
 */
export default function QrCodeCard() {
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [unavailable, setUnavailable] = useState(false);

  useEffect(() => {
    let alive = true;
    invoke<PairingInfo>('get_pairing_info')
      .then((info) => QRCode.toDataURL(info.url, { width: 144, margin: 1 }))
      .then((url) => {
        if (alive) setDataUrl(url);
      })
      .catch((err) => {
        console.error('get_pairing_info failed', err);
        if (alive) setUnavailable(true);
      });
    return () => {
      alive = false;
    };
  }, []);

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
      <p className="text-[10px] font-bold text-gray-400">
        {unavailable ? '桌面端离线，启动后重试' : '扫码开启手机跨端辅助展示'}
      </p>
    </section>
  );
}
