import { test, expect } from '@playwright/test';
import { WebSocketServer, type WebSocket } from 'ws';

/**
 * skeleton.spec.ts — Task 3 e2e (SYNC-02 walking-skeleton proof):
 * the phone H5 connects over a REAL WebSocket to a mock server speaking
 * @nextalk/protocol shapes and renders one subtitle; malformed payloads
 * (missing seq) are dropped by the isServerEvent gate (threat T-01-02)
 * and never reach the DOM.
 *
 * The mock listens on 8788 (playwright's own teleprompter preview owns
 * 8787); the page is told to bypass the desktop via the `ws=` override
 * param (research Open Question 3). Runs under the `teleprompter` project
 * (the desktop project testIgnores this file).
 */

const MOCK_HOST = '127.0.0.1';
const MOCK_PORT = 8788;

const VALID_SUBTITLE = {
  t: 'subtitle',
  id: 'q1',
  speaker: 'interviewer',
  seq: 1,
  zh: '请简单介绍一下你自己。',
  en: 'Could you walk me through the specific steps you took to optimize the database?',
  final: true,
};

/** Malformed: a subtitle-shaped payload missing `seq` (narrowing must drop it). */
const MALFORMED_SUBTITLE = {
  t: 'subtitle',
  id: 'evil',
  speaker: 'user',
  zh: '恶意载荷不应显示',
  en: 'this should never render',
  final: true,
};

const H5_URL = `/?token=e2e-token&ws=ws://${MOCK_HOST}:${MOCK_PORT}`;

/** Deterministic wait: localhost delivery + render of a leaked payload is
 *  sub-100ms, so 600ms gives a huge margin without slowing the suite. */
const SETTLE_MS = 600;

async function startMockServer(): Promise<{
  nextSocket: Promise<WebSocket>;
  close: () => Promise<void>;
}> {
  const wss = new WebSocketServer({ host: MOCK_HOST, port: MOCK_PORT });
  await new Promise<void>((resolve, reject) => {
    wss.once('listening', resolve);
    wss.once('error', reject);
  });
  const nextSocket = new Promise<WebSocket>((resolve) => {
    wss.once('connection', (socket) => resolve(socket));
  });
  const close = () =>
    new Promise<void>((resolve) => {
      // Force-terminate any leftover client so the next test can rebind
      // 8788 immediately (wss.close() alone waits for clients).
      for (const client of wss.clients) client.terminate();
      wss.close(() => resolve());
    });
  return { nextSocket, close };
}

test('H5 renders one valid subtitle broadcast over WS', async ({ page }) => {
  const { nextSocket, close } = await startMockServer();
  try {
    await page.goto(H5_URL);
    const socket = await nextSocket;

    socket.send(JSON.stringify(VALID_SUBTITLE));

    // zh line types out (~11 chars x 40ms ≈ 0.5s), then the en line
    // (~79 chars ≈ 3.2s) — generous timeout for full-text completion.
    const zhLine = page.getByText(VALID_SUBTITLE.zh);
    await expect(zhLine).toHaveText(VALID_SUBTITLE.zh, { timeout: 10_000 });

    const enLine = page.getByText(VALID_SUBTITLE.en);
    await expect(enLine).toHaveText(VALID_SUBTITLE.en, { timeout: 15_000 });

    // Speaker chip reflects the event (interviewer -> 面试官).
    await expect(page.getByText('面试官', { exact: true })).toBeVisible();
  } finally {
    await close();
  }
});

test('malformed subtitle (missing seq) is dropped by the isServerEvent gate', async ({
  page,
}) => {
  const { nextSocket, close } = await startMockServer();
  try {
    await page.goto(H5_URL);
    const socket = await nextSocket;

    // The malformed payload is the ONLY message on an otherwise healthy
    // connection. If the gate leaked it, the empty state would be replaced
    // by a subtitle card within ~100ms of localhost delivery.
    socket.send(JSON.stringify(MALFORMED_SUBTITLE));
    await page.waitForTimeout(SETTLE_MS);

    await expect(page.getByText(MALFORMED_SUBTITLE.zh)).toHaveCount(0);
    await expect(page.getByText(MALFORMED_SUBTITLE.en)).toHaveCount(0);

    // The empty state is untouched — proof the payload never rendered.
    await expect(page.getByText('等待语音输入')).toBeVisible();
  } finally {
    await close();
  }
});
