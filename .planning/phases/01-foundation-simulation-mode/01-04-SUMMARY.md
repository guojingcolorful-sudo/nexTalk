---
phase: 01-foundation-simulation-mode
plan: 04
subsystem: ui
tags: [react, vite, tailwind, websocket, reconnect-backoff, resume-sinceSeq, wake-lock, playwright, vitest, tdd, teleprompter, mobile-h5, neobrutalism]

# Dependency graph
requires:
  - phase: 01-01
    provides: design tokens (@nextalk/design-tokens preset + tokens.css), locked protocol types (@nextalk/protocol) with the isServerEvent narrowing guard, Space Grotesk + FontAwesome local bundling
  - phase: 01-02
    provides: token'd H5 shell (App token gate, useWs stub, useTypewriter, core-js entry import), Rust LAN WS server + 64KB frame cap, the locked r1 sim-script payloads (question/answer/strategy, seq 1-2)
  - phase: 01-03
    provides: desktop ChatBubble/StrategyCard visual language and the explicit-afterEach(cleanup) test convention this plan's specs follow
provides:
  - The complete phone teleprompter surface (390x844) — status capsule, 字幕 / AI 辅助 tabs, bottom 开始提词 gate
  - useWs hardened — 1s/2s/4s/30s backoff ladder, {t:resume,sinceSeq} resume with seq/id dedupe, {t:control,language} send, 64KB cap intact
  - useWakeLock — secure-context Screen Wake Lock + http:// LAN hidden-video fallback with visibilitychange re-engage
  - 8 phone components (StatusCapsule, MobileTabs, ChatBubble+TypedLine, StrategyCard, GateScreen, EmptyState, ErrorBanner, TypewriterDots, Toast)
  - Bundled 977-byte H.264 blank-loop.mp4 asset (the NoSleep-style fallback needs animation to keep the device awake)
  - 27 vitest + 5 Playwright specs (10 with --repeat-each) with an ephemeral-port mock WS server
affects: [01-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - 'Typewriter checkpoint testing: page.clock.install() + pauseAt() before load, then runFor(20 x 40ms) — install() alone leaks real ticks'
    - 'Wake-lock fallback forced deterministically in e2e by deleting Navigator.prototype.wakeLock (http://localhost IS a secure context, the LAN origin is not)'
    - 'Node-side DOM polling helper for specs that freeze the page clock (Playwright auto-wait polls with page rAF)'
    - 'Immutable state updates only in hooks; every inbound frame passes isServerEvent before it can reach React state'
    - 'Design-token palette + hard cartoon shadows as literal Tailwind classes per the UI-SPEC, black text on brand fills, white text only on dark neutrals'

key-files:
  created:
    - apps/teleprompter/src/components/StatusCapsule.tsx
    - apps/teleprompter/src/components/MobileTabs.tsx
    - apps/teleprompter/src/components/ChatBubble.tsx
    - apps/teleprompter/src/components/StrategyCard.tsx
    - apps/teleprompter/src/components/GateScreen.tsx
    - apps/teleprompter/src/components/Toast.tsx
    - apps/teleprompter/src/components/EmptyState.tsx
    - apps/teleprompter/src/components/ErrorBanner.tsx
    - apps/teleprompter/src/components/TypewriterDots.tsx
    - apps/teleprompter/src/hooks/useWakeLock.ts
    - apps/teleprompter/src/hooks/useWakeLock.test.tsx
    - apps/teleprompter/src/hooks/useWs.test.tsx
    - apps/teleprompter/src/pages/TeleprompterPage.test.tsx
    - apps/teleprompter/src/assets/blank-loop.mp4
    - apps/teleprompter/src/vite-env.d.ts
    - e2e/teleprompter.spec.ts
  modified:
    - apps/teleprompter/src/hooks/useWs.ts
    - apps/teleprompter/src/pages/TeleprompterPage.tsx
    - apps/teleprompter/src/App.tsx
    - playwright.config.ts

key-decisions:
  - 'The phone owns a single session-level language mode (中/EN/EN+中) pushed as {t:control,language} — per-bubble toggles stay desktop-only per 01-03'
  - 'Tab selection is URL state (?tab=) merged with ?token= via history.replaceState, so a phone waking from sleep lands back on the tab it was reading and the link stays shareable'
  - 'The wake-lock fallback uses a bundled 977-byte H.264 loop fetched with Vite ?no-inline — no CDN, no runtime media synthesis, browser-cacheable on the LAN'
  - 'The bottom gate bar is a flex-none footer, not a fixed overlay — the stream scrolls in the remaining space so nothing is ever hidden behind it'
  - 'e2e instantiates its own mock WS server on an ephemeral port (ws devDep) and reaches it through the ?ws= override; 8787 stays the untouched product port'
  - 'Inbound language ServerEvents render nothing on the phone (the phone owns its mode display); the variant is the desktop observation channel for 01-05'

patterns-established:
  - 'Reconnect ladder is jitterless and capped (1s/2s/4s/30s) and resets only on a successful open — no reconnect storm (T-01-10)'
  - 'Every (re)open sends the resume cursor before anything else, then dedupes replay by subtitle seq / strategy id'
  - 'Gesture-bound OS capabilities (wake lock, media playback) are engaged from the same 开始提词 tap and released on 暂停/unmount'
  - 'Locked Chinese copy is rendered verbatim from the UI-SPEC copy table, including error, toast and pairing-failure strings'

requirements-completed: [SYNC-02, SYNC-03, SYNC-04, SYNC-05, UI-01, UI-02]

# Metrics
duration: ~2h 30m active across two sessions
completed: 2026-09-11
tasks: 3
files: 20
commits: 4 (+1 metadata)
---

# Phase 1 Plan 04: Complete Phone Teleprompter Summary

**One-liner:** The phone H5 is now a real teleprompter — a token'd LAN WS client that renders typed bilingual subtitles and strategy cards, survives a dropped connection with a jitterless 1s→30s backoff and a `{t:resume,sinceSeq}` replay, pushes `{t:control,language}` back to the desktop, and holds the screen awake on a plain `http://192.168.x.x` origin through a hidden looping-video fallback.

## Performance

- **Duration:** ~2h 30m active across two sessions (2026-09-10 17:5x→18:21 and 2026-09-11 12:2x→13:23 +08:00; the process was interrupted overnight, so wall-clock spans ~19.5h)
- **Started:** 2026-09-10 (first 01-04 commit 18:11:12 +08:00)
- **Completed:** 2026-09-11T13:22:44+08:00 (last task commit)
- **Tasks:** 3/3 complete
- **Files changed:** 20 (+2042 / −155)
- **Commits:** 4 task commits (1 RED + 1 GREEN TDD pair) + 1 metadata commit

## Accomplishments

- **Task 1 — Full teleprompter UI (`32d810e`):** Replaced the 01-02 stub page with the real surface. `StatusCapsule` maps the four WS states to the locked copy (正在连接 / 实时同步中 / 正在自动重连 / 连接已断开) with a breathing red dot; `MobileTabs` is an ARIA tablist with roving tabindex and brand-filled active tabs (portalGreen for 字幕, mortyYellow for AI 辅助); `ChatBubble` renders the spoken language as the primary line and the translation as a colored subline (rickBlue for the interviewer, portalGreen for the user) with the typewriter; `StrategyCard` carries the yellow hard shadow; `GateScreen` pins 开始提词 / 暂停提词 in the thumb-reach zone with the language-cycle segment; `EmptyState` / `ErrorBanner` / `TypewriterDots` / `Toast` round out the component inventory. Tab state lives in `?tab=` next to `?token=`.
- **Task 2 — Hardened transport + wake lock (RED `51f041f` → GREEN `0d0d87f`):** `useWs` gained the locked backoff ladder (1s → 2s → 4s → 30s cap, jitterless, reset only on a healthy open), the `{t:resume,sinceSeq}` handshake on every (re)open with seq/strategy-id dedupe on the replay tail, `sendLanguagePref` emitting the exact `{t:control,language}` frame, and kept the `isServerEvent` gate + 64KB cap ahead of all state. New `useWakeLock` requests `screen` in a secure context and falls back to a 1×1 invisible muted looping `<video>` (paused while hidden, replayed on return) on the real LAN origin, raising 已启用防休眠回退模式 once.
- **Task 3 — End-to-end proof (`dc30106`):** `e2e/teleprompter.spec.ts` drives five sections against a real `ws` mock server on an ephemeral port (mock-token mount + 重扫码 error, the locked r1 answer typing out to an exact 20-character checkpoint and 开始提词 engaging the fallback, tab/mode control frames with no `language_pref` key, a terminated socket reconnecting with `sinceSeq:5` and a de-duplicated replay tail, and malformed frames never rendering).
- **Verification:** 27/27 vitest; 5/5 new e2e (10/10 under `--repeat-each=2`, no flaky timeout asserts); 26/26 full Playwright suite across both projects; build green at 93.30 kB gz JS / 4.48 kB gz CSS (budgets 300 kB / 50 kB); the loop asset is emitted as a real file and the dist has zero external http(s) references.

## Task Commits

| # | Task | Phase | Commit | Type |
|---|------|-------|--------|------|
| 1 | Full phone teleprompter UI + page/tab/gate specs | 01-04 | `32d810e` | feat |
| 2a | Failing reconnect/resume/control + wake-lock specs | 01-04 | `51f041f` | test (RED) |
| 2b | Hardened useWs + useWakeLock + page wiring | 01-04 | `0d0d87f` | feat (GREEN) |
| 3 | teleprompter e2e (mock WS server, 5 sections) | 01-04 | `dc30106` | test |

**Plan metadata:** `(this commit)` (docs: complete plan)

## Files Created / Modified

**Created (16):** the nine phone components (`StatusCapsule`, `MobileTabs`, `ChatBubble`, `StrategyCard`, `GateScreen`, `Toast`, `EmptyState`, `ErrorBanner`, `TypewriterDots`), `hooks/useWakeLock.ts`, `hooks/useWakeLock.test.tsx`, `hooks/useWs.test.tsx`, `pages/TeleprompterPage.test.tsx`, `assets/blank-loop.mp4`, `vite-env.d.ts`, `e2e/teleprompter.spec.ts`.

**Modified (4):** `hooks/useWs.ts` (reconnect + resume + control on top of the 01-02 client), `pages/TeleprompterPage.tsx` (stub → full surface + hook wiring), `App.tsx` (pairing-ticket gate + `ws=` override, pair-failure screen), `playwright.config.ts` (route the new spec to the teleprompter project).

## Decisions Made

1. **One session-level language mode on the phone.** The prefs are a session mode pushed to the desktop as a single typed `ClientMessage`; per-bubble toggles remain a desktop-only affordance per 01-03. This keeps the phone's control model to one tap and one frame.
2. **Tab state is URL state.** `?tab=ai` is written with `history.replaceState` while preserving `?token=`, so a phone waking from sleep (or a reload after a browser eviction) returns to the tab the user was reading.
3. **The bottom bar is a flex-none footer, not a fixed overlay.** The plan suggested `pb-28` under a fixed bar; a footer in the flex column keeps the stream's scroll area honest and cannot overlap the last bubble on a short viewport.
4. **The fallback asset is a bundled 977-byte H.264 loop** fetched with Vite's `?no-inline` query, so the file stays a real cached asset instead of a base64 blob — zero CDN, matching the project's offline constraint.
5. **`useWakeLock` reports `isFallback` but the page does not branch on it.** The toast is fired through an `onFallbackEngaged` callback so the hook stays presentation-free.
6. **Inbound `language` ServerEvents render nothing on the phone.** The phone owns its mode display locally; the variant is the desktop's observation channel (01-05 emits it).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 2's file list omitted the wiring the task requires** (Task 2)
- **Issue:** Task 2 named only the two hooks and their specs, but backoff/resume/control and the wake-lock gesture are only observable through `TeleprompterPage`. Left unimplemented, the task's own `<verify>` could not pass and the hooks would be dead code.
- **Fix:** Task 1 rendered the controls with real local behaviour; Task 2's GREEN commit added the hook instantiation and the `sendLanguagePref` / activate / deactivate wiring to the page.
- **Files modified:** `apps/teleprompter/src/pages/TeleprompterPage.tsx`
- **Verification:** 27 vitest green, including the control-frame and wake-fallback page specs
- **Committed in:** `0d0d87f`

**2. [Rule 1 - Bug] Missing test cleanup leaked `visibilitychange` listeners between specs** (Task 2 GREEN)
- **Issue:** `expected "vi.fn()" to be called 1 times, but got 3 times` — with vitest globals off, Testing Library never self-registers cleanup, so hooks from earlier renders stayed subscribed and every dispatched event hit all of them.
- **Fix:** explicit `afterEach(cleanup)` in `useWakeLock.test.tsx` and `useWs.test.tsx` with a comment explaining why.
- **Files modified:** `apps/teleprompter/src/hooks/useWakeLock.test.tsx`, `apps/teleprompter/src/hooks/useWs.test.tsx`
- **Verification:** all 25 specs in the two files pass
- **Committed in:** `0d0d87f`

**3. [Rule 3 - Blocking] The `?no-inline` asset import had no type declaration** (Task 2)
- **Issue:** `import blankLoopUrl from '../assets/blank-loop.mp4?no-inline'` fails type resolution and leaves the module untyped outside the Vite pipeline.
- **Fix:** added `apps/teleprompter/src/vite-env.d.ts` with the `vite/client` reference.
- **Files modified:** `apps/teleprompter/src/vite-env.d.ts`
- **Verification:** build green; the asset is emitted as `blank-loop-*.mp4`
- **Committed in:** `0d0d87f`

**4. [Rule 3 - Blocking] No local tool could produce the fallback video asset** (Task 2)
- **Issue:** Playwright's bundled ffmpeg only has `webm` muxers (`Unrecognized option 'movflags'`), and the asset must be an MP4 the iOS/Android browser can loop.
- **Fix:** generated the 16×16 single-frame H.264 MP4 with a throwaway script driving Chromium's `MediaRecorder`, then verified playback in chromium (16×16, duration 0.033s, `paused: false`) before deleting the script.
- **Files modified:** `apps/teleprompter/src/assets/blank-loop.mp4` (977 bytes)
- **Verification:** e2e asserts the element is muted/looping/playsInline and Playwright confirms `play()` succeeded
- **Committed in:** `0d0d87f`

**5. [Rule 1 - Bug] The plan's bubble color spec contradicted the UI-SPEC contrast contract** (Task 1)
- **Issue:** the plan said "message text 15px/600 ink on light bubbles", but the UI-SPEC contrast contract (and the shipped 01-03 desktop `ChatBubble`) put bubbles on dark neutrals with white text — ink-on-dark would have been unreadable.
- **Fix:** followed the UI-SPEC: `bg-slate-800` / `bg-green-900` bubbles with white primary text and color-coded translation sublines.
- **Files modified:** `apps/teleprompter/src/components/ChatBubble.tsx`
- **Verification:** rendered and asserted in the page spec and the e2e
- **Committed in:** `32d810e`

**6. [Rule 1 - Bug] `page.clock.install()` alone is not deterministic** (Task 3)
- **Issue:** the plan prescribed `install()` before load + `runFor(20 * 40)`; because `install()` still lets real time through, a tick or two leaked while the bubble mounted and the checkpoint read 21 characters instead of 20.
- **Fix:** pause the clock before navigation (`install()` + `pauseAt(new Date())`), and add a Node-side DOM poll because Playwright's own auto-waiting polls with page rAF and would stall against a paused clock.
- **Files modified:** `e2e/teleprompter.spec.ts`
- **Verification:** the exact-equality checkpoint assertion passes on every repeat (10/10 under `--repeat-each=2`)
- **Committed in:** `dc30106`

**7. [Rule 3 - Blocking] The desktop Playwright project collected the new spec** (Task 3)
- **Issue:** both projects share `testDir: './e2e'`, so `teleprompter.spec.ts` would also have run against the 1420 desktop preview with the wrong baseURL.
- **Fix:** added `**/teleprompter.spec.ts` to the desktop project's `testIgnore` (same pattern 01-03 established for `desktop.spec.ts`).
- **Files modified:** `playwright.config.ts`
- **Verification:** 26/26 full suite, each spec running under exactly one project
- **Committed in:** `dc30106`

**8. [Rule 2 - Missing critical functionality] Supporting components the plan's file list did not name** (Task 1)
- **Issue:** the plan's copy contract requires empty states, the pairing-failure banner and the generating indicator, but the Task 1 file list stopped at seven components; inlining those in the page would have duplicated markup and broken the DRY convention set by 01-03.
- **Fix:** extracted `EmptyState`, `ErrorBanner` and `TypewriterDots` (plus `Toast` for the fallback announcement) as their own components.
- **Files modified:** `apps/teleprompter/src/components/{EmptyState,ErrorBanner,TypewriterDots,Toast}.tsx`
- **Verification:** every locked string is asserted in a spec
- **Committed in:** `32d810e` (Toast in `0d0d87f`)

---

**Total deviations:** 8 auto-fixed (4 Rule 3 blocking, 3 Rule 1 bugs, 1 Rule 2 missing critical)
**Impact on plan:** All eight were necessary for correctness or for the task's own verification to be meaningful. No scope creep — every added file is either an extracted primitive from the UI-SPEC component inventory or a test fixture. The transport contract (`language`, `sinceSeq`, the 30s cap, the 64KB gate) follows the plan and the protocol package exactly.

## TDD Gate Compliance

Task 2 is the plan's only `tdd="true"` task; the gate sequence is intact in `git log`:

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `51f041f` | `test(01-04): add failing reconnect/resume/control and wake-lock specs` — 8 of 10 new specs fail against the 01-02 hooks |
| GREEN | `0d0d87f` | `feat(01-04): harden useWs and add the wake-lock fallback` — all 25 specs in the two files pass |
| REFACTOR | — | not needed; the GREEN implementation was already the final shape |

No RED commit was skipped and no test passed before its implementation existed.

## Issues Encountered

- **Task 1's e2e mode-control assertion initially targeted the wrong label.** The spec clicked `语言模式 中，` first, but the phone's session default is `bilingual` (EN+中) — the first tap moves to 中. Fixed by clicking the committed label at each step (EN+中 → 中 → EN), asserting the label between taps so the cycle can never read a stale `languagePref`, and asserting the final frame is `{t:'control',language:'all-en'}`.
- **The `ws=` e2e override is load-bearing on this machine.** Port 8787 is held by an unrelated local tool, so every spec points the page at its own ephemeral mock server; the product default port is untouched.
- **Zero-CDN gate re-checked.** `dist/` contains only XML namespaces and core-js/React error-message URLs — no external fetches; fonts and the loop asset are served from the bundle.

## Threat Flags

No new threat surface. The plan's mitigations are implemented and regression-tested: T-01-02 (`isServerEvent` field validation before any state, 64KB cap, e2e section 5), T-01-10 (jitterless ladder capped at 30s, reset only on a healthy open), T-01-12 (React text nodes only — zero `dangerouslySetInnerHTML` in the H5). The wake-lock fallback adds a same-origin DOM element, not an endpoint.

No hardcoded stubs, placeholder copy or unwired data sources were introduced; the two empty states are the locked pre-session copy, not stand-ins.

## User Setup Required

**Real-device manual pass — NOT PERFORMED on this machine (no phone, no camera, no GUI session).** This is the plan's Task 3 `<human-check>` and the VALIDATION.md Manual-Only rows for SYNC-04 / SYNC-01. Both the automated equivalents are green (the mock-WS e2e covers the pairing mount, the wake-fallback engagement and the reconnect/resume path end to end), but the hardware behaviour is unverified.

To complete it (needs the desktop app running, so 8787 must be free first — stop `tools/jd-inbox-server.mjs`):

1. **QR pairing (SYNC-01):** `pnpm --filter @nextalk/desktop tauri dev` → 控制台 shows the pairing QR → scan it with a real phone camera on the same Wi-Fi → the browser opens `http://<lan-ip>:8787/?token=…` and the capsule shows 实时同步中.
2. **Wake lock (SYNC-04):** tap 开始提词 → the phone must stay awake for **≥ 2 minutes** untouched (expect the 已启用防休眠回退模式 toast on the http origin), then lock the screen / switch apps and return — the screen must stay awake again.
3. **Reconnect resume (SYNC-05):** start a simulated session, kill Wi-Fi for ~10s mid-stream → the capsule shows 正在自动重连 → restore Wi-Fi → the stream resumes from where it stopped with **no duplicated bubbles**.

Record pass/fail notes in 01-VALIDATION.md's Manual-Only table when run.

## Next Phase Readiness

- The phone surface is feature-complete for Phase 1: subtitle + strategy streams, reconnect/resume, language control and the wake lock are all live on the LAN.
- 01-05 can rely on `useWs`'s `sendLanguagePref` for the desktop-side `language` event and on the `?ws=` override for any additional mock-server specs.
- Two handoffs stay open for the verifier: the real-device pass above, and the day-0 `?tab=` URL rewrite must keep `?token=` (covered by a spec, worth re-checking once 01-05 touches the URL).

---

*Phase: 01-foundation-simulation-mode*
*Completed: 2026-09-11*

## Self-Check: PASSED

- All 16 created files and 4 modified files exist on disk.
- All four task commits exist: `32d810e`, `51f041f`, `0d0d87f`, `dc30106`.
- 27/27 vitest green, 5/5 new e2e (10/10 with `--repeat-each=2`), 26/26 full Playwright suite.
- No stubs, TODOs or `console.log` in any file this plan touched.
