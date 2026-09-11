---
phase: 01-foundation-simulation-mode
reviewed: 2026-09-11T06:35:00Z
depth: standard
files_reviewed: 60
files_reviewed_list:
  - apps/desktop/package.json
  - apps/desktop/src-tauri/src/lan/server.rs
  - apps/desktop/src-tauri/src/lib.rs
  - apps/desktop/src-tauri/src/sim/mod.rs
  - apps/desktop/src-tauri/src/sim/script.rs
  - apps/desktop/src-tauri/src/sim/source.rs
  - apps/desktop/src-tauri/src/sim/source_test.rs
  - apps/desktop/src-tauri/src/state.rs
  - apps/desktop/src-tauri/tests/session_integration.rs
  - apps/desktop/src/App.tsx
  - apps/desktop/src/components/AiTimeline.tsx
  - apps/desktop/src/components/ChatBubble.test.tsx
  - apps/desktop/src/components/ChatBubble.tsx
  - apps/desktop/src/components/ConfirmModal.tsx
  - apps/desktop/src/components/FileDropZone.tsx
  - apps/desktop/src/components/FormField.tsx
  - apps/desktop/src/components/GlossaryRow.tsx
  - apps/desktop/src/components/HeaderBar.tsx
  - apps/desktop/src/components/KnowledgeRow.tsx
  - apps/desktop/src/components/LanguageToggle.test.tsx
  - apps/desktop/src/components/LanguageToggle.tsx
  - apps/desktop/src/components/MicStatusPill.tsx
  - apps/desktop/src/components/NeobrutalismButton.tsx
  - apps/desktop/src/components/NexTalkBrand.tsx
  - apps/desktop/src/components/PanelHeader.tsx
  - apps/desktop/src/components/QrCodeCard.tsx
  - apps/desktop/src/components/StealthCard.tsx
  - apps/desktop/src/components/Toast.tsx
  - apps/desktop/src/components/TypewriterDots.tsx
  - apps/desktop/src/components/WizardShell.tsx
  - apps/desktop/src/hooks/useTauriEvents.ts
  - apps/desktop/src/pages/ConsolePage.tsx
  - apps/desktop/src/pages/DualPanePage.tsx
  - apps/desktop/src/pages/GlossaryPage.tsx
  - apps/desktop/src/pages/RecordingsPage.tsx
  - apps/desktop/src/pages/ResumeImportPage.tsx
  - apps/desktop/src/pages/ReviewPage.tsx
  - apps/desktop/src/pages/SetupWizardPage.tsx
  - apps/desktop/src/pages/VoiceEnrollmentPage.tsx
  - apps/teleprompter/src/App.tsx
  - apps/teleprompter/src/components/ChatBubble.tsx
  - apps/teleprompter/src/components/GateScreen.tsx
  - apps/teleprompter/src/components/MobileTabs.tsx
  - apps/teleprompter/src/components/StatusCapsule.tsx
  - apps/teleprompter/src/components/StrategyCard.tsx
  - apps/teleprompter/src/hooks/useTypewriter.ts
  - apps/teleprompter/src/hooks/useTypewriter.test.tsx
  - apps/teleprompter/src/hooks/useWakeLock.ts
  - apps/teleprompter/src/hooks/useWakeLock.test.tsx
  - apps/teleprompter/src/hooks/useWs.ts
  - apps/teleprompter/src/hooks/useWs.test.tsx
  - apps/teleprompter/src/pages/TeleprompterPage.tsx
  - e2e/demo.spec.ts
  - e2e/desktop.spec.ts
  - e2e/skeleton.spec.ts
  - e2e/teleprompter.spec.ts
  - packages/design-tokens/src/tailwind-preset.js
  - packages/design-tokens/src/tokens.css
  - packages/protocol/src/index.ts
  - packages/protocol/src/index.test.ts
  - tools/vendor-experiments/rtt/measure.mjs
findings:
  critical: 1
  warning: 8
  info: 9
  total: 18
status: issues_found
---

# Phase 1: Code Review Report

**Reviewed:** 2026-09-11T06:35:00Z
**Depth:** standard
**Files Reviewed:** 60
**Status:** issues_found

## Summary

Phase 1 (foundation + simulation mode) is largely sound: the sim engine's pure/stateful equivalence holds, the wire contract is mirrored on both sides, the T-01-02 narrowing gate really does stop malformed payloads before React state, the `/ws` upgrade is token-gated, and the H5 renders every inbound string as a JSX text node — there is no `innerHTML` / `dangerouslySetInnerHTML` / `eval` / `new Function` anywhere in shipped source. The concurrency core is correctly ordered (engine lock → state lock, never the reverse) and `advance_sim_for` re-checks the session epoch under the engine lock before publishing.

The one defect that must be fixed before this phase ships is **CR-01**: the phone's resume/dedupe cursors are never reset when the desktop starts a *new* session, so the second 开始模拟会话 of a process renders an empty teleprompter while the desktop looks perfectly healthy — a silent failure of the product's core value chain.

The warnings cluster in three places: (1) the session lifecycle has no "timeline was reset" signal, so the webviews keep stale bubbles (WR-02) and the H5 drops the new session's events outright (CR-01); (2) a lagging WS broadcast receiver permanently kills that phone's fan-out (WR-01), latent today but live the moment real streaming STT replaces the sim ticks; (3) the voice-enrollment recorder leaves the microphone live when the user navigates back mid-countdown (WR-05).

Verified clean and worth recording: `replay_after_subtitle_seq` is correct over monotonic seq; `client_disconnected`'s saturating arithmetic is correct; the sim's milestone bookkeeping (`emitted`, `next_seq`, `round_start_ms`) is correct under interrupt/repeat; `ChatBubble`'s language fallback matrix is correct for every `LanguagePref` × speaker combination; the static server's `ServeDir`/`ServeFile` path handling is not traversal-prone; no secret is hardcoded anywhere (the vendor tool reads credentials from the environment only).

## Critical Issues

### CR-01: A second session renders nothing on the phone — the resume cursor is never reset across session restarts

**File:** `apps/teleprompter/src/hooks/useWs.ts:69,71,88-94,121,155` (with `apps/desktop/src-tauri/src/state.rs:226-242,352-355`)

**Issue:** `start_session` swaps in a fresh engine and clears the timeline, so subtitle `seq` restarts at 1 and strategy ids restart at `s-r1`. The H5 keeps `seenSeqRef` and `seenStrategyIdsRef` for the lifetime of the mount — the subscription effect's deps are `[ticket?.token, ticket?.url]` (line 155), and the ticket is parsed exactly once in `apps/teleprompter/src/App.tsx:37` (`useMemo(pairingTicket, [])`), so neither ever changes. The dedupe loop then discards the entire new session:

```ts
// useWs.ts:88 — after a restart this still holds the previous session's high-water mark
if (event.seq <= seenSeqRef.current) continue; // replay tail overlap
// useWs.ts:92 — "s-r1" is already in the set from the previous session
if (seenStrategyIdsRef.current.has(event.id)) continue;
```

Reproduction with no mocks: pair a phone → run the scripted session to the end → press 停止 → press 开始模拟会话 again. The desktop console and the dual pane stream the new session; the phone's capsule flips to 实时同步中 but the subtitle stream and the AI cards stay empty, and `resume` (line 121) sends the stale `sinceSeq`, so even a reconnect cannot recover. The scripted script only emits seq 1..8, so every line lands at or below the stale cursor — the phone shows nothing for the whole session, silently. The e2e suites cannot catch it: `e2e/demo.spec.ts:232` asserts only that the desktop CTA returns to 开始模拟会话, and `e2e/teleprompter.spec.ts` never restarts a session against a live socket.

The same missing signal (there is no "the timeline restarted" event in the wire model) produces WR-02 on the desktop side.

**Fix:** Introduce a session identity on the wire and clear client cursors when it changes. The state layer already has the right value — `session_epoch` — it just never reaches the clients. Minimal shape:

```rust
// state.rs — announce the new session before anything else can be appended.
pub fn start_session(&self) -> Result<u64, String> {
    let current = self.session_status();
    if current == SessionStatus::Listening || current == SessionStatus::Generating {
        return Err("a simulated session is already running".into());
    }
    let epoch = self.session_epoch.fetch_add(1, Ordering::SeqCst) + 1;
    self.replace_sim();
    self.reset_timeline();
    self.publish(ServerEvent::SessionStarted { epoch });   // new variant, no client state
    self.announce_status(SessionStatus::Listening);
    Ok(epoch)
}
```

```ts
// useWs.ts — the session changed: drop every cursor and everything rendered.
const accept = (incoming: ServerEvent[]) => {
  for (const event of incoming) {
    if (event.t === 'session_started') {
      seenSeqRef.current = 0;
      seenStrategyIdsRef.current.clear();
      setEvents([]);
      continue;
    }
    ...
  }
};
```

and in `useTauriEvents.ts`, handle the same variant by `setEvents([])` + `setLanguageMode(null)` inside the listener (before the `narrowSession` early return at line 85, which drops anything that narrows to an empty batch).

Two caveats the implementer must handle, both consequences of `replay_after_subtitle_seq` (state.rs:329-343): it slices the timeline *after the last subtitle with `seq <= since_seq`*, so (a) a phone that was offline across the restart receives an empty tail and never sees the marker — put the epoch on every event, or include the marker in the resume reply unconditionally when `since_seq` exceeds the timeline's highest subtitle seq; (b) an empty `Timeline` frame used as a sentinel would be swallowed by that same slice. The epoch-on-events form is the only one that closes the reconnect hole; the marker-only form fixes the connected case that the repro above exercises.

## Warnings

### WR-01: A lagged broadcast receiver silently kills the phone's event fan-out

**File:** `apps/desktop/src-tauri/src/lan/server.rs:163-174`

**Issue:** The per-client forwarder exits on *any* `recv()` error, and `tokio::sync::broadcast` reports a slow receiver as `Err(RecvError::Lagged(_))` rather than blocking:

```rust
let broadcast_task = tokio::spawn(async move {
    while let Ok(event) = broadcast_rx.recv().await {   // Lagged => loop ends, task dies
        let Ok(text) = serde_json::to_string(&event) else { continue; };
        if tx_broadcast.send(Message::Text(text.into())).await.is_err() { break; }
    }
});
```

With a 64-slot channel against the sim's tick rate this cannot trip today, which is exactly why it reads as latent: the moment real streaming STT events replace the scripted ticks (Phase 2), one slow phone (backgrounded tab, GC pause, congested Wi-Fi) falls past the ring buffer, the forwarding task exits, and that phone's socket stays open and healthy-looking while never receiving another event. `phone_count` still reports it as connected, so the console shows 已连接 with a frozen teleprompter, and the client never reconnects because the socket itself is fine — only the forwarder died.

**Fix:** Treat `Lagged` as "drop the missed frames and resync", not as terminal:

```rust
loop {
    match broadcast_rx.recv().await {
        Ok(event) => {
            let Ok(text) = serde_json::to_string(&event) else { continue; };
            if tx_broadcast.send(Message::Text(text.into())).await.is_err() { break; }
        }
        Err(broadcast::error::RecvError::Lagged(skipped)) => {
            tracing::warn!(skipped, "ws client lagged; resuming at the next event");
            continue;
        }
        Err(broadcast::error::RecvError::Closed) => break,
    }
}
```

### WR-02: Stopping or restarting a session never clears the desktop webviews' event state

**File:** `apps/desktop/src/hooks/useTauriEvents.ts:82-94` (with `apps/desktop/src/pages/ConsolePage.tsx:206`)

**Issue:** `session` payloads are only ever appended (`setEvents((previous) => [...previous, ...batch])`, line 86) and no event in the model means "the timeline restarted". `start_session` / `stop_session` clear the Rust-side timeline, but the console and dual-pane windows keep every bubble from the previous session and stack the new session's lines underneath. The locked copy the user just confirmed promises the opposite — `ConsolePage.tsx:206` `body="当前字幕与策略将清空"`, asserted verbatim in `e2e/demo.spec.ts:223` — so the UI contradicts its own contract, and neither window ever returns to the locked empty states (等待语音输入 / AI 策略将自动生成) without a window reload.

**Fix:** Handle the session-identity signal from CR-01 by clearing local state. Note it must sit in the listener *before* `narrowSession`, because a marker that narrows to an empty batch is dropped by the early return at line 85:

```ts
listen<unknown>('session', (event) => {
  const payload = event.payload as { t?: unknown } | null;
  if (payload?.t === 'session_started') {
    setEvents([]);          // the confirmation copy promised this
    setLanguageMode(null);  // the new session runs on per-speaker defaults
    return;
  }
  const batch = narrowSession(event.payload);
  if (batch.length === 0) return;
  setEvents((previous) => [...previous, ...batch]);
  ...
});
```

### WR-03: Phone language mode can silently diverge from the desktop

**File:** `apps/teleprompter/src/pages/TeleprompterPage.tsx:77,111-116` and `apps/teleprompter/src/hooks/useWs.ts:157-161`

**Issue:** Two halves of one defect. (a) The tap handler updates local state optimistically and then calls `sendLanguagePref`, which is a silent no-op whenever the socket is not open (`if (!socket || socket.readyState !== WS_OPEN) return;`) — the control frame is neither queued nor retried, so one tap during the reconnect backoff window (1s→2s→4s→30s, T-01-10) leaves the phone rendering `all-en` while the desktop keeps the previous mode, permanently. (b) `languagePref` (line 77) is initialised to `'bilingual'` and never seeded from the `language` event the desktop echoes back on the same stream — `useWs` delivers it (it passes the narrowing gate), but `TeleprompterPage` only reads the `subtitle` and `strategy` variants out of `events`. After any reload, wake-from-sleep, or a second phone joining, the UI displays a mode the session is not in, and the next tap sends a value derived from that wrong base, lurching the desktop to a mode the user did not ask for.

**Fix:** Adopt the echoed value as the source of truth and re-send the preference on every (re)open:

```tsx
// TeleprompterPage.tsx
const echoedLanguage = useMemo(() => {
  for (let i = events.length - 1; i >= 0; i -= 1) {
    const event = events[i];
    if (event.t === 'language') return event.language;
  }
  return null;
}, [events]);

useEffect(() => {
  if (echoedLanguage !== null) setLanguagePref(echoedLanguage);
}, [echoedLanguage]);
```

```ts
// useWs.ts — remember the last preference and re-apply it inside ws.onopen.
const languageRef = useRef<LanguagePref>('bilingual');
const sendLanguagePref = useCallback((pref: LanguagePref) => {
  languageRef.current = pref;
  const socket = socketRef.current;
  if (!socket || socket.readyState !== WS_OPEN) return; // re-sent by onopen
  send(socket, { t: 'control', language: pref });
}, []);
// in ws.onopen, after the resume frame:
send(ws, { t: 'control', language: languageRef.current });
```

### WR-04: `interrupt_session` / `repeat_session` skip the epoch re-check under the engine lock

**File:** `apps/desktop/src-tauri/src/state.rs:273-302` (contrast `advance_sim_for`, lines 252-265)

**Issue:** `advance_sim_for` deliberately re-reads the session epoch *after* acquiring the engine lock, because the scheduler task holds an epoch captured before it slept:

```rust
let mut sim = engine.lock().expect("sim lock poisoned");
if self.session_epoch() != epoch { return None; }   // stale scheduler tick — drop it
```

`interrupt_session` and `repeat_session` take the same engine lock and mutate the sim (jump the cursor / restart the round) but never re-check the epoch. The window is open: `stop_session` bumps the epoch and a concurrent `interrupt()`/`repeat()` that already read `sim_handle()` can still acquire the *old* engine afterwards and mutate it. The guard inside those methods (`is_generating()` / status) is evaluated against the new session's status, not against the epoch the mutation belongs to, so a stale command can advance a round cursor on the engine the user just started. This is exactly the class of bug the `advance_sim_for` comment exists to prevent; it is currently unreachable only because the command surface happens to serialize the two actions.

**Fix:** Mirror the guard the scheduler path already uses:

```rust
pub fn interrupt_session(&self) -> Result<(), String> {
    let epoch = self.session_epoch();
    let engine = self.sim_handle();
    let mut sim = engine.lock().expect("sim lock poisoned");
    if self.session_epoch() != epoch {
        return Err("the session ended before the command landed".into());
    }
    ...
}
```

Better structurally: let one mutex guard `sim_handle` + `session_epoch` together so the pattern cannot be forgotten when Phase 2 adds more mutating commands.

### WR-05: Voice enrollment keeps the microphone live after the user navigates back

**File:** `apps/desktop/src/pages/VoiceEnrollmentPage.tsx:54-63,82-93,101`

**Issue:** `recording` is a boolean the interval effect keys off, and nothing ties it to the step the user is actually looking at:

```tsx
useEffect(() => {
  if (!recording) return undefined;
  const id = window.setInterval(() => {
    const next = Math.max(0, remainingRef.current - 1);
    remainingRef.current = next;
    setRemaining(next);
    if (next === 0) finishRecording();   // setStep(2) — from whichever step is showing
  }, 1000);
  return () => window.clearInterval(id);
}, [recording, finishRecording]);
```

`onPrev` (line 101, `setStep((value) => Math.max(0, value - 1))`) does not stop the recorder, so pressing 上一步 during the 5-second countdown leaves `streamRef.current` live — microphone indicator on, audio still captured — while the interval keeps ticking against a view the user has left; at zero it force-jumps the wizard to step 2 from wherever they navigated to. On a feature whose entire purpose is capturing the user's voice, "navigate away and the mic stays hot" is the wrong default.

**Fix:** Release the device on every exit path and stop advancing once the component is gone:

```tsx
useEffect(() => () => {              // unmount = release the device
  streamRef.current?.getTracks().forEach((track) => track.stop());
  streamRef.current = null;
}, []);

const cancelRecording = useCallback(() => {
  streamRef.current?.getTracks().forEach((track) => track.stop());
  streamRef.current = null;
  setRecording(false);
  remainingRef.current = RECORDING_SECONDS;
  setRemaining(RECORDING_SECONDS);
}, []);
// onPrev={handlePrev} where handlePrev calls cancelRecording() before setStep(...)
```

### WR-06: `Array.prototype.at` is used in shipped desktop UI but is outside the declared macOS 12.0 floor and the polyfill set

**File:** `apps/desktop/src/pages/DualPanePage.tsx:58`

**Issue:** `const lastSubtitleId = subtitles.at(-1)?.id ?? null;` — `Array.prototype.at` is Safari 15.4+. The app declares `"minimumSystemVersion": "12.0"` (`apps/desktop/src-tauri/tauri.conf.json`), which spans macOS 12.0-12.2 → Safari 15.0-15.3, where `.at` is undefined (`TypeError: subtitles.at is not a function` on every render, i.e. the extended view dies on open). The Vite target `['safari15', 'es2022']` does not down-level built-ins, and the only polyfill imported (checked in both entry points) is `core-js/proposals/promise-with-resolvers` — there is no `es.array.at` polyfill. `tools/vendor-experiments/rtt/measure.mjs:309-312` uses `.at` too, which is fine (Node).

**Fix:** Use the universally supported form, or add `import 'core-js/actual/array/at';` next to the existing polyfill if the team wants `.at` everywhere:

```tsx
const lastSubtitleId = subtitles.length > 0 ? subtitles[subtitles.length - 1].id : null;
```

### WR-07: The RTT tool prints and persists the full endpoint URL, contradicting its own credential policy

**File:** `tools/vendor-experiments/rtt/measure.mjs:352,374-383`

**Issue:** The module header states "The value never appears in the output" (lines 22-24) and `--auth-env` is carefully validated to reject an inline secret, but the URL is treated as non-sensitive in both sinks: `out(\`endpoint: ${config.method} ${config.url}\`)` echoes `endpoint.href` — **including the query string**, because `URL.href` preserves it — and the report writes `config.url` into `OUTPUT.json`. Several of the vendors this tool targets accept the key in the query string, and the usage text itself documents Gemini's header form at lines 51-54; the default output path is inside the repo, so one run with a keyed URL prints the secret to the terminal (and any CI log or pasted transcript) and commits it to `OUTPUT.json`, the very file the README tells the team to share as the experiment result.

**Fix:** Record origin + path and redact query values:

```js
const redact = (href) => {
  const url = new URL(href);
  const search = [...url.searchParams].map(([name]) => `${name}=<redacted>`).join('&');
  return `${url.origin}${url.pathname}${search ? `?${search}` : ''}`;
};
// line 352
out(`endpoint: ${config.method} ${redact(config.url)}`);
// line 377
url: redact(config.url),
```

### WR-08: Once the extended view window is closed, 扩展视图 can never bring it back

**File:** `apps/desktop/src/pages/ConsolePage.tsx:72-80` (with the close control in `apps/desktop/src/components/HeaderBar.tsx`)

**Issue:** The dual window's header exposes a close control that calls `getCurrentWindow().close()`, destroying the window. `openDualPane` only looks the window up and shows it:

```tsx
const dual = await WebviewWindow.getByLabel('dual');
await dual?.show();               // null after the user closed it
} catch (err) { console.error('showing the dual window failed', err); }
```

`getByLabel` returns `null` for a destroyed window and `dual?.show()` is then a silent no-op (the catch only logs), so after one accidental 关闭 the 扩展视图 button is dead for the rest of the process — mid-interview, with no feedback explaining why. The e2e suite cannot catch this: `installTauriMock` always reports `['console', 'dual']` as live windows (`e2e/demo.spec.ts:80-81`).

**Fix:** Recreate the window when it is gone, and surface the failure instead of swallowing it:

```tsx
const openDualPane = async () => {
  try {
    const existing = await WebviewWindow.getByLabel('dual');
    if (existing) {
      await existing.show();
      return;
    }
    const dual = new WebviewWindow('dual', {
      url: '/#/dual', width: 860, height: 680, resizable: false, decorations: false, visible: true,
    });
    await new Promise<void>((resolve, reject) => {
      dual.once('tauri://created', () => resolve());
      dual.once('tauri://error', (event) => reject(new Error(String(event.payload))));
    });
  } catch (err) {
    console.error('showing the dual window failed', err);
    setDualFailed(true);   // render an ErrorBanner, as start_session already does
  }
};
```

## Info

### IN-01: Pairing token compared with a non-constant-time `!=`

**File:** `apps/desktop/src-tauri/src/lan/server.rs:135`
**Issue:** `if params.token != state.pairing_token()` short-circuits on the first differing byte, a byte-wise oracle over 32 hex characters. Practically hard to exploit over a LAN (jitter, one attempt per connection), but this comparison is the only thing between the LAN and the session stream (T-01-01).
**Fix:** `if !bool::from(state.pairing_token().as_bytes().ct_eq(params.token.as_bytes()))` using `subtle::ConstantTimeEq`, or compare an HMAC with `Mac::verify_slice`.

### IN-02: The client's frame cap counts UTF-16 units, not bytes

**File:** `apps/teleprompter/src/hooks/useWs.ts:28,126-127`
**Issue:** `if (data.length === 0 || data.length > MAX_FRAME_BYTES) return;` mirrors the server's byte cap (T-01-02) but `String.length` counts UTF-16 code units, so a CJK payload of up to ~192 KB of UTF-8 passes the "mirror". No exploit today (the payload is still parsed and narrowed), but the documented invariant does not hold for the language this app is built around.
**Fix:** `new TextEncoder().encode(data).byteLength > MAX_FRAME_BYTES`, or rename the constant to state the unit and document the 3x slack.

### IN-03: The 64 KiB cap is enforced after the frame has already been buffered

**File:** `apps/desktop/src-tauri/src/lan/server.rs:181`
**Issue:** The length check runs on the assembled `Message::Text`, so tungstenite has already buffered the whole frame. The upgrade is token-gated, but the protocol defaults axum leaves in place are far above the app's limit (16 MiB max frame, 64 MiB max message), so a paired client can force the desktop to allocate that much per message before `MAX_FRAME_BYTES` is ever consulted.
**Fix:** Enforce the limit at the source, in `ws_handler` before `ws.on_upgrade(...)` at line 138:

```rust
ws.max_message_size(MAX_FRAME_BYTES)
  .max_frame_size(MAX_FRAME_BYTES)
  .on_upgrade(move |socket| client_loop(socket, state))
```

### IN-04: The `ws=` dev override ships in the production H5 bundle

**File:** `apps/teleprompter/src/App.tsx:15-19`
**Issue:** `params.get('ws') ?? undefined` lets any URL choose the WS server the phone connects to. The desktop never emits a `ws=` param, so this is a dev/test affordance — but it compiles into the same bundle the QR code serves, so a crafted link could point a user's phone at a third-party socket and render attacker-controlled subtitles (contained to that one phone; the desktop cannot be impersonated this way).
**Fix:** `import.meta.env.DEV ? params.get('ws') ?? undefined : undefined`, keeping the e2e override on a test build.

### IN-05: Doc comment claims a strictness the TypeScript guard does not implement

**File:** `apps/desktop/src-tauri/src/lan/server.rs:66` (comment) vs `packages/protocol/src/index.ts:77-109`
**Issue:** The Rust module doc says "unknown fields are rejected on both sides (`deny_unknown_fields`)", but `isServerEvent` checks only the fields each variant needs and ignores extras on the TS side. The Rust decoder is strict; the TS narrowing is permissive. Behaviour is safe (extras are never rendered), but the contract statement is wrong and will mislead Phase 2 as the wire format grows.
**Fix:** Correct the comment, or make the guard strict (per-variant key count) if the byte-exact contract is intended.

### IN-06: Overlapping `activate()` calls can leak a wake-lock sentinel

**File:** `apps/teleprompter/src/hooks/useWakeLock.ts:100-114` (re-acquire path at 144-154)
**Issue:** Each `activate()` awaits `api.request('screen')` and then assigns `sentinelRef.current = sentinel`, with no guard against a second request being in flight. Two rapid taps, or an activate racing the `visibilitychange` re-acquire, leave the first sentinel unreferenced; `deactivate()` (lines 119-121) releases only the last one, so the screen stays awake after 暂停提词 with no UI state explaining it.
**Fix:** Capture an attempt counter before the await and bail out if it changed, mirroring the `engagedRef` check the function already performs after `await`.

### IN-07: `startRecording` is not guarded while `getUserMedia` is pending

**File:** `apps/desktop/src/pages/VoiceEnrollmentPage.tsx:65-84`
**Issue:** The button is disabled only by `recording`, which is set *after* `await devices.getUserMedia(...)` resolves. A double click issues two streams and overwrites `streamRef.current` with the second, so the first is never stopped (`finishRecording` only stops the last one) — the microphone stays live for the rest of the page's life.
**Fix:** Add a `requesting` state set before the await, disable the button on `requesting || recording`, and stop the previous `streamRef.current` before assigning a new one.

### IN-08: `pairing_url` silently falls back to loopback, producing a QR code the phone can never use

**File:** `apps/desktop/src-tauri/src/state.rs:123-132`
**Issue:** When `local_ip_address::local_ip()` fails (no LAN interface, VPN-only, network change mid-session) the URL becomes `http://127.0.0.1:<port>/?token=…`. The console renders it as a scannable QR with no indication anything is wrong; the phone connects to itself and pairing never completes — the exact situation the locked 配对失败 copy exists for, but it is never shown.
**Fix:** Return a `Result` or an `addressSource: 'lan' | 'loopback'` flag and have `QrCodeCard` render the recovery copy when the address is loopback.

### IN-09: `ConfirmModal`'s focus effect re-runs on every parent render

**File:** `apps/desktop/src/components/ConfirmModal.tsx:39-70` (call sites pass inline closures, e.g. `apps/desktop/src/pages/ConsolePage.tsx:208`)
**Issue:** The effect deps are `[open, onCancel]` and every call site passes a fresh arrow function, so any parent re-render while the dialog is open — and the console re-renders on every `session` / `session_status` event — tears down and re-establishes the trap: the cleanup's `previouslyFocused?.focus?.()` fires and then `focusables[0]?.focus()` pulls focus back to 取消, discarding where the user had tabbed mid-decision.
**Fix:** Hold the callbacks in refs (`const cancelRef = useRef(onCancel); cancelRef.current = onCancel;`) and depend on `[open]` only, or wrap the call-site handlers in `useCallback`.

---

_Reviewed: 2026-09-11T06:35:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
