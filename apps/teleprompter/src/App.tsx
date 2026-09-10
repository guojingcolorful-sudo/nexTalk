import { useMemo } from 'react';
import ErrorBanner from './components/ErrorBanner';
import TeleprompterPage from './pages/TeleprompterPage';
import type { WsTicket } from './hooks/useWs';

/**
 * Teleprompter H5 entry (01-04): the complete phone surface. Pairing comes
 * from the QR URL query — `?token={token}` (mandatory) plus an optional `ws=`
 * override kept from 01-02 for dev/e2e mock servers (Open Question 3).
 *
 * A missing token means the phone reached the page without a pairing code
 * (typed URL, stale bookmark, rotated session) — that is a pairing failure,
 * not a connection failure, so the 重扫码 path is shown instead of the page.
 */
function pairingTicket(): WsTicket | null {
  const params = new URLSearchParams(window.location.search);
  const token = params.get('token');
  if (!token) return null;
  return { token, url: params.get('ws') ?? undefined };
}

/** Frame-less error screen: the phone has no session to render at all. */
function PairingErrorScreen() {
  return (
    <div className="flex h-full justify-center">
      <div className="dot-matrix-root flex h-full w-full max-w-[390px] flex-col overflow-hidden px-4 pt-16">
        <ErrorBanner
          title="连接已失效，请重新扫码"
          body="请确认手机与电脑连接同一 Wi-Fi，然后重新扫码"
        />
      </div>
    </div>
  );
}

export default function App() {
  const ticket = useMemo(pairingTicket, []);

  if (!ticket) return <PairingErrorScreen />;
  return <TeleprompterPage ticket={ticket} />;
}
