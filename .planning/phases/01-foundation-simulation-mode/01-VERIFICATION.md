---
phase: 01-foundation-simulation-mode
verified: 2026-09-11T06:28:09Z
status: human_needed
score: 22/22 must-haves verified
overrides_applied: 0
human_verification:
  - test: "01-03 desktop walkthrough — run `pnpm --filter @nextalk/desktop tauri dev` on macOS 12.7, click through the console hub into every page (扩展视图, 设置, 术语表, 音色注册, 简历导入, 本地资产→录音资产/复盘报告) and back"
    expected: "Both windows frame correctly: 340×680 console + 860×680 dual, frameless, transparent, rounded corners, NO black halo/shimmer; every route reachable with a working back affordance; six missing pages show empty and populated states with 模拟数据 badges"
    why_human: "Transparent-window rendering on macOS 12.7 WKWebView cannot be judged from source or headless Chromium (Playwright runs in a browser, not Tauri). Recorded in 01-VALIDATION.md as manual-only."
  - test: "01-04 real-device phone pass — free port 8787 (see prerequisite), scan the console QR with a real phone, tap 开始提词, keep the screen on ≥2 min, kill wifi for ~10 s, restore it"
    expected: "H5 loads over plain http://192.168.x.x; 开始提词 engages the wake lock fallback (screen stays awake past the iOS/Android auto-dim window); wifi kill shows 正在自动重连; on restore the stream resumes from the last seen seq with no duplicate or lost subtitles"
    why_human: "Screen-wake behavior, real-device browser quirks, and physical wifi interruption cannot be simulated headlessly. Recorded in 01-VALIDATION.md (SYNC-04) and STATE.md blockers."
  - test: "01-05 full interactive demo — launch the desktop app with port 8787 free, scan QR from a phone, 开始模拟会话, let r1 play to console + dual + phone, watch the phone count flip, switch the phone language mode, press 打断, then 重听, then let the session end"
    expected: "All three surfaces stay in sync through 4 rounds; status pill moves idle→listening→generating→ended; QrCodeCard flips 等待扫码 → 已连接 1 台设备 and back to 0 on disconnect; phone language mode changes re-render the desktop's un-toggled bubbles; 打断 cuts to the next round and 重听 replays the current round with fresh seq on every surface"
    why_human: "Needs a GUI session + a phone + a camera; the executor has neither. Every leg has a green automated equivalent (see Behavioral Spot-Checks), but the end-to-end physical demo is the phase's stated deliverable."
---

# Phase 1: Foundation + Simulation Mode Verification Report

**Phase Goal:** As a 求职者 preparing for an English interview, I want to run the complete NexTalk product — desktop app, LAN-synced phone teleprompter, and full simulated event flow — on mock audio, so that I can demo the entire experience end-to-end before any real audio pipeline exists.
**Verified:** 2026-09-11T06:28:09Z
**Status:** human_needed
**Re-verification:** No — initial verification

MVP-mode note: the phase goal is a User Story and passes the format guard (`/^As a .+, I want to .+, so that .+\.$/` → `valid: true`). The outcome clause verified here is "can demo the entire experience end-to-end before any real audio pipeline exists."

## Goal Achievement

### Observable Truths

Merged from the five PLAN `must_haves` blocks and the five ROADMAP Success Criteria (SCs; no plan truth was dropped — the roadmap contract is fully covered by the plan truths below). Evidence was collected first-hand against the codebase at HEAD; every documented verification command was re-run rather than trusted from SUMMARY.md.

| #   | Truth (source) | Status     | Evidence       |
| --- | -------------- | ---------- | -------------- |
| 1   | `pnpm -r test` runs all unit suites green (01-01) | ✓ VERIFIED | Re-ran: protocol 20 + design-tokens 13 + desktop 11 + teleprompter 27 = **71 passed** |
| 2   | Both apps build to ES2022/Safari-15-compatible bundles (01-01; SC1/UI-03) | ✓ VERIFIED | `pnpm run build` green; both `vite.config.*` set `build.target: ['safari15','es2022']`; desktop 129.39 kB gz JS / 5.37 kB gz CSS, teleprompter 93.30 kB gz JS |
| 3   | Design token values match UI-SPEC exactly (01-01; SC1) | ✓ VERIFIED | `packages/design-tokens/src/tokens.css` holds the plain-hex spec (`--color-portal-green: #97ce4c`, `--color-morty-yellow: #fbf061`, `--color-rick-blue: #00b5cc`, `--color-darker-space: #151519`, 4px/6px colored hard shadows, type/space/radius scales); `tokens.test.ts` snapshots the CSS (13 green) |
| 4   | Protocol messages narrow correctly at runtime (01-01; T-01-02) | ✓ VERIFIED | `packages/protocol/src/index.ts` closed unions + `isServerEvent` per-field validation; 20 protocol vitests assert narrowing; both consumers (`useWs.ts`, `useTauriEvents.ts`) gate every inbound frame through it |
| 5   | Desktop opens TWO independent windows: 340×680 console (visible) + 860×680 dual (hidden), frameless transparent, no black halo (01-02; SC1) | ✓ VERIFIED | `tauri.conf.json` inspected: `console` 340×680 `visible:true`, `dual` 860×680 `visible:false`, both `decorations:false`, `transparent:true`, `shadow:false`, `macOSPrivateApi:true`, `minimumSystemVersion:"12.0"` (visual halo judgment routed to human check) |
| 6   | Console renders the design system + a scannable QR encoding the pairing URL (01-02; SC1/SC2) | ✓ VERIFIED | `ConsolePage.tsx` + `QrCodeCard.tsx`: `invoke('get_pairing_info')` → `QRCode.toDataURL(info.url, {width:144, margin:1})`; 19 desktop e2e specs green |
| 7   | A phone opening `http://<lan-ip>:8787/?token=X` connects over WS; a wrong token gets 401 (01-02; SC2; T-01-01) | ✓ VERIFIED | `lan/server.rs` `ws_handler` returns `(StatusCode::UNAUTHORIZED, "invalid pairing token")` on mismatch; 128-bit `SysRng` token (rand 0.10); real-WS integration test pairs with the real token and drives real frames; real-device scan is the human check |
| 8   | Starting a session emits one simulated subtitle that renders on the H5 with typewriter animation (01-02; SC2; SYNC-05) | ✓ VERIFIED | `useTypewriter` 40 ms cadence + reduced-motion collapse (vitest-proven); `e2e/teleprompter.spec.ts` mock-WS flow green (typewriter checkpoints at 20 chars under `page.clock`) |
| 9   | From the console the user reaches every page and returns with a back affordance (01-03; SC5) | ✓ VERIFIED | `App.tsx` routes `/console /dual /setup /voice /glossary /resume /recordings /review` (+ `*` fallback); desktop e2e walks the navigation, all specs green |
| 10  | Dual-pane shows bilingual subtitles left + AI strategy timeline right; per-bubble toggle switches 中/EN/EN+中 independently per speaker (01-03; SC1/SC3) | ✓ VERIFIED | `DualPanePage.tsx` (实时字幕 / AI 辅助 panes), `ChatBubble.tsx` resolves `localPref ?? mode ?? SPEAKER_DEFAULT_PREF[speaker]` (interviewer bilingual, user all-zh); `LanguageToggle.tsx` + `LanguagePref` from protocol; desktop vitest + e2e cover independent toggles |
| 11  | All six missing pages render empty AND populated states, Chinese copy per UI-SPEC, mock content labeled 模拟数据 (01-03; SC5; UI-02) | ✓ VERIFIED | All six pages exist with real content (SetupWizardPage 180 lines, VoiceEnrollmentPage 184, GlossaryPage 153, ResumeImportPage 137, RecordingsPage 76, ReviewPage 78), `MOCK_BADGE_LABEL = '模拟数据'`; PageStub deleted; e2e covers both states |
| 12  | On the phone the user lands on the H5, reads the token'd URL, starts with 开始提词 (01-04; SC2) | ✓ VERIFIED | `App.tsx` `?token=` gate + `GateScreen.tsx` gesture-bound 开始提词 (contains 开始提词); invalid pairing shows 连接已失效，请重新扫码; e2e teleprompter project green |
| 13  | A subtitle broadcast from the desktop renders on the phone with typewriter (01-04; SC2; SYNC-02) | ✓ VERIFIED | `useWs.ts` narrows via `isServerEvent` and feeds subtitle deltas to `useTypewriter`; teleprompter vitest 27 green; mock-WS e2e renders live subtitles |
| 14  | Phone survives an interrupted connection: backoff reconnect + resume from last seen seq (01-04; SYNC-05; T-01-10) | ✓ VERIFIED | `BACKOFF_LADDER_MS = [1000, 2000, 4000, 30_000]`; resume `{t:'resume', sinceSeq}` on every open; seq/strategy-id dedupe; e2e kills the mock WS and asserts 正在自动重连 → resume without duplicates |
| 15  | Screen stays awake during 提词 on plain `http://192.168.x.x` (01-04; SC4; SYNC-04) | ✓ VERIFIED | `useWakeLock.ts`: secure-context `navigator.wakeLock.request('screen')` + hidden 1×1 muted looping `<video>` fallback (bundled 977-byte asset, `?no-inline`), visibilitychange re-engage, gesture-bound; e2e wake-fallback path green — real-phone pass routed to human verification |
| 16  | Phone switches 字幕 / AI 辅助 and sends language preference back to the desktop (01-04; SC3; SYNC-03) | ✓ VERIFIED | `MobileTabs.tsx` aria-selected tablist; `sendLanguagePref` emits `{t:'control', language}`; URL state `?tab=ai` via `history.replaceState` preserving `?token=`; e2e covers tab switch + control send |
| 17  | 开始模拟会话 plays a 4-round simulated interview streaming in sync to console + dual + phone (01-05; SC2/SC5; SYNC-01) | ✓ VERIFIED | `sim/script.rs` 4 rounds (r1 byte-exact vs `reference-mockup.html`: 慢查询日志 strategy, 模拟数据 tag); `sim/source.rs` pure `script_state(elapsed_ms)` + injectable `TimeSource`; `tests/session_integration.rs` drives the real router + real `tokio-tungstenite` client through r1/phone sync; `e2e/demo.spec.ts` asserts both surfaces |
| 18  | AI strategy card updates per round; status pill moves idle→listening→generating→ended (01-05; UI-02) | ✓ VERIFIED | Per-round strategy nodes in `script.rs`; status events published through `SessionState`; console CTA state machine 开始模拟会话 / 回答生成中 / 会话进行中; demo e2e asserts transitions |
| 19  | During an answer the user can 打断 and 重听 — timeline stays consistent on all surfaces (01-05; D-03) | ✓ VERIFIED | `interrupt_session`/`repeat_session` commands; repeats under `-r{n}` ids with fresh seq (`INTERRUPT_LEAD_MS = 1000`); integration test asserts `-r{n}` ids and consistent replay; demo e2e walks CTA through 停止/打断/重听 |
| 20  | Phone language-mode change takes effect on the desktop's next render (01-05; SC3; SYNC-03) | ✓ VERIFIED | `ws_handler` writes `set_language_prefs` and republishes `ServerEvent::Language` on the same broadcast; `useTauriEvents` derives `languageMode`; dual-pane e2e "follows the phone mode" green |
| 21  | QrCodeCard shows 已连接 N 台设备 vs 等待扫码 driven by live phone_count (01-05) | ✓ VERIFIED | `client_connected()/client_disconnected()` emit `phone_count`; QrCodeCard status pill; integration test asserts counter 0→1→2→1→0; desktop e2e "shows the live phone count" green |
| 22  | Vendor experiment framework exists with no-keys-in-Phase-1 policy (01-05; D-04) | ✓ VERIFIED | `tools/vendor-experiments/` README (policy at line 11), STT A/B protocol, blind-test schema (parses), `rtt/measure.mjs` (`--help` exit 0, TLS-only, `--auth-env` takes the env var NAME); grep found zero secrets/keys |

**Score:** 22/22 truths verified

### T-01-06 Judgment (EoP on the Tauri command surface)

The plans originally scoped T-01-06 to `capabilities/default.json`. That is **not physically possible in Tauri 2.11**: `apps/desktop/src-tauri/gen/schemas/acl-manifests.json` lists only the `core*` namespaces (`core`, `core:app`, `core:event`, `core:image`, `core:menu`, `core:path`, `core:resources`, `core:tray`, `core:webview`, `core:window`) — there is no app-defined command namespace to grant or deny. **Judgment: the shipped alternative is an acceptable equivalent mitigation.** `state.rs` enforces the guard in the command bodies themselves: `start_session` returns `Err("a simulated session is already running")` while Listening/Generating; `interrupt_session` / `repeat_session` return `Err` unless Generating. These are asserted by unit tests in `state.rs` and driven through the real command surface by `tests/session_integration.rs`. Residual exposure: both desktop windows can invoke every command — but the commands only mutate local simulated-session state (no privilege boundary is crossed between the two windows of one app), and the H5 has zero Tauri IPC surface. The mitigation is substantive, tested, and documented in STATE.md; not a gap.

### Required Artifacts

Every must_haves artifact exists, is substantive (not a stub), and is wired into a live consumer.

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `packages/design-tokens/src/tokens.css` | UI-SPEC tokens | ✓ VERIFIED | Plain hex, no oklch/color-mix; imported by both app entries |
| `packages/protocol/src/index.ts` | Shared WS contract | ✓ VERIFIED | Closed unions + `isServerEvent`; consumed by both hooks and tests |
| `apps/desktop/vite.config.ts`, `apps/teleprompter/vite.config.ts` | safari15 target | ✓ VERIFIED | Both contain `safari15` + `es2022` |
| `apps/desktop/src-tauri/tauri.conf.json` | Two fixed windows | ✓ VERIFIED | Exact labels/sizes/visibility per must-have |
| `apps/desktop/src-tauri/src/lan/server.rs` | axum LAN server + token WS | ✓ VERIFIED | `ws_handler`, 401 on bad token, 64 KiB frame cap, `deny_unknown_fields`, resume replay, no-store |
| `apps/desktop/src-tauri/src/state.rs` | SessionState + timeline | ✓ VERIFIED | Pairing token, status gating, publish/broadcast, phone counter |
| `apps/desktop/src-tauri/src/sim/script.rs` | 4-round script | ✓ VERIFIED | r1 byte-exact vs reference mockup; 模拟数据 tag; 32,400 ms total |
| `apps/desktop/src-tauri/src/sim/source.rs` | Deterministic SimSource | ✓ VERIFIED | Pure `script_state`, injectable clock, epoch-ticket scheduler |
| `apps/desktop/src-tauri/src/lib.rs` | Session lifecycle commands | ✓ VERIFIED | 5 commands registered (get_pairing_info, start/stop/interrupt/repeat) |
| `apps/desktop/src-tauri/tests/session_integration.rs` | Real-WS integration test | ✓ VERIFIED | Spawns real router on ephemeral port, real token, real frames |
| `apps/desktop/src/pages/ConsolePage.tsx` | Console hub | ✓ VERIFIED | Nav + CTA state machine + QR card; contains 开始模拟会话 |
| `apps/desktop/src/pages/DualPanePage.tsx` | Dual-pane live view | ✓ VERIFIED | 实时字幕 left + AI 辅助 right; header 打断/重听 when listening |
| `apps/desktop/src/components/ChatBubble.tsx` | Per-speaker bubble | ✓ VERIFIED | `localPref ?? mode ?? speaker default` resolution |
| `apps/desktop/src/components/LanguageToggle.tsx` | Per-bubble toggle | ✓ VERIFIED | Typed against `LanguagePref` |
| `apps/desktop/src/components/QrCodeCard.tsx` | QR + phone count | ✓ VERIFIED | `get_pairing_info` → QR; 已连接 N 台设备 / 等待扫码 |
| `apps/desktop/src/hooks/useTauriEvents.ts` | Tauri event bridge | ✓ VERIFIED | Listens session/session_status/phone_count, narrows via `isServerEvent` |
| Six pages: `setup/voice/glossary/resume/recordings/review` | UI-02 missing pages | ✓ VERIFIED | 76–184 lines each, locked copy, 模拟数据 badges |
| `apps/desktop/src/data/mock-data.ts` | Mock data source | ✓ VERIFIED | Contains 模拟数据 labeling |
| `apps/teleprompter/src/hooks/useTypewriter.ts` | Typewriter hook | ✓ VERIFIED | 40 ms cadence + reduced-motion |
| `apps/teleprompter/src/hooks/useWs.ts` | WS client | ✓ VERIFIED | Backoff, resume, dedupe, control send, narrowing |
| `apps/teleprompter/src/hooks/useWakeLock.ts` | Wake lock + fallback | ✓ VERIFIED | Secure-context API + hidden video fallback |
| `apps/teleprompter/src/components/{StatusCapsule,MobileTabs,GateScreen}.tsx` | H5 surfaces | ✓ VERIFIED | 正在自动重连 / AI 辅助 / 开始提词 present |
| `e2e/teleprompter.spec.ts` | H5 flow e2e | ✓ VERIFIED | Mock-WS server on ephemeral port via `?ws=` override |
| `tools/vendor-experiments/README.md` | D-04 framework | ✓ VERIFIED | No-keys policy; zero secrets in tree |

### Key Link Verification

All 16 must_haves key_links WIRED (verified by source inspection; the previously run test suites exercise each link).

| From | To  | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| `tailwind.config.js` (desktop) | `tailwind-preset.js` | preset import | ✓ WIRED | Build consumes preset; tailwind v3.4 |
| `tokens.test.ts` | `tokens.css` | contract snapshot | ✓ WIRED | Reads CSS via `node:fs` (vitest 4 stubs .css) |
| `protocol/index.test.ts` | `protocol/index.ts` | narrowing assertions | ✓ WIRED | `isServerEvent` asserted |
| `lan/server.rs` | `state.rs` | token check on WS upgrade | ✓ WIRED | 401 asserted in handler + integration test |
| `useWs.ts` (H5) | `protocol` | `isServerEvent` | ✓ WIRED | Every inbound frame gated |
| `QrCodeCard.tsx` | `lib.rs` | `get_pairing_info` | ✓ WIRED | invoke → QR render |
| `ConsolePage.tsx` | `lib.rs` | `invoke` start/stop/interrupt/repeat | ✓ WIRED | CTA state machine |
| `DualPanePage.tsx` | `useTauriEvents.ts` | subscribe `session` | ✓ WIRED | Subtitles/strategy/timeline events |
| `LanguageToggle.tsx` | `protocol` | `LanguagePref` | ✓ WIRED | Typed segments 中/EN/EN+中 |
| `useWs.ts` (H5) | `protocol` | `isServerEvent` | ✓ WIRED | Same guard client-side |
| `useWs.ts` (H5) | `useTypewriter.ts` | subtitle deltas | ✓ WIRED | Deltas feed the animation |
| `GateScreen.tsx` | `useWakeLock.ts` | gesture acquires lock | ✓ WIRED | `activate()` from 开始提词 |
| `sim/source.rs` | `state.rs` | timeline append + broadcast channel | ✓ WIRED | One event model, two transports |
| `ConsolePage.tsx` | `lib.rs` | invoke start/interrupt/repeat | ✓ WIRED | Buttons gated by status |
| `lan/server.rs` | `state.rs` | WS broadcast + resume replay | ✓ WIRED | `Resume{since_seq}` replayed as Timeline |
| `lan/server.rs` | `QrCodeCard.tsx` | `phone_count` emit | ✓ WIRED | Counter 0→1→2→1→0 in integration test |

### Data-Flow Trace (Level 4)

This phase's data source IS the deterministic simulator (the goal explicitly says "on mock audio"), so Level 4 verifies that real simulated data flows from the engine through to every renderer — not that a database exists.

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| `DualPanePage.tsx` | `subtitles`, `timeline`, `status` | `useTauriEvents` ← Rust `session`/`session_status` emits ← `SessionState.timeline` ← `SimSource` poll | Yes — 4 scripted rounds, byte-exact r1, monotonic seq | ✓ FLOWING |
| `ConsolePage.tsx` | `phoneCount`, CTA status | Tauri `phone_count` + `session_status` | Yes — driven by real WS connect/disconnect (`client_connected/disconnected`) | ✓ FLOWING |
| `QrCodeCard.tsx` | `info.url` | `invoke('get_pairing_info')` ← `SessionState` pairing token + LAN IP | Yes — real 128-bit token in URL | ✓ FLOWING |
| Teleprompter subtitles | WS frames | `useWs` ← axum broadcast ← `SessionState.timeline` | Yes — session_integration.rs proves the real wire end-to-end | ✓ FLOWING |
| `AiTimeline` | strategy nodes | `script.rs` per-round strategies | Yes — r1 数据库优化 card with 3 bullets | ✓ FLOWING |

Note: the desktop e2e mocks the Tauri bridge (headless Chromium cannot host a webview), but the mocked stream mirrors the same script and the real Rust→WS path is covered by `session_integration.rs`. No HOLLOW/ORPHANED/DISCONNECTED artifacts found.

### Behavioral Spot-Checks

All four documented commands were re-run at HEAD during this verification (not trusted from SUMMARY.md).

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| All unit suites green | `pnpm -r test` | 71 passed (protocol 20, design-tokens 13, desktop 11, teleprompter 27) | ✓ PASS |
| Rust engine + integration green | `source "$HOME/.cargo/env" && cargo test --workspace --manifest-path apps/desktop/src-tauri/Cargo.toml` | 29 passed (27 lib + 2 integration) | ✓ PASS |
| Both apps build for safari15 | `pnpm run build` | Green — desktop 129.39 kB gz JS / 5.37 kB gz CSS; teleprompter 93.30 kB gz JS | ✓ PASS |
| Browser e2e green | `pnpm exec playwright test` | 29 passed in 31.6 s (desktop + teleprompter projects; preview on 8791) | ✓ PASS |
| Vendor RTT tool runnable | `node tools/vendor-experiments/rtt/measure.mjs --help` | exit 0 | ✓ PASS |
| Blind-test schema valid JSON | `node -e "JSON.parse(fs.readFileSync(...))"` | Parses | ✓ PASS |
| MVP user story format | regex guard | `valid: true` | ✓ PASS |

### Probe Execution

SKIPPED — no probes declared in any PLAN/SUMMARY, and this is not a migration/tooling phase (no `scripts/*/tests/probe-*.sh` exist).

### Requirements Coverage

All 11 Phase 1 requirement IDs are claimed by plans (union of plan `requirements:` == the roadmap's Phase 1 list) and all are marked Complete in REQUIREMENTS.md. No orphans.

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| UI-01 | 01-01, 01-02, 01-03, 01-04 | Neobrutalism design system (4px black borders, hard-offset shadows, three functional colors, Space Grotesk, dot-matrix) | ✓ SATISFIED | tokens.css + preset instantiated in console/dual/H5; token tests green; visual halo check routed to human |
| UI-02 | 01-02, 01-03, 01-04, 01-05 | Reference HTML screens + six missing pages | ✓ SATISFIED | All 8 routes real; r1 byte-exact vs reference-mockup.html; e2e green |
| UI-03 | 01-01, 01-02 | Zero-CDN local bundles (Tailwind v3.4, safari15-safe) | ✓ SATISFIED | Both dist trees contain zero external http(s) refs; safari15 target; Tailwind 3.4 |
| SYNC-01 | 01-01, 01-02, 01-05 | LAN WS + QR pairing, no install | ✓ SATISFIED | axum 8787, pairing-as-auth, 401 on bad token; real-WS integration test replays 4 rounds |
| SYNC-02 | 01-02, 01-04 | Phone teleprompter: bilingual subtitles + AI strategy cards | ✓ SATISFIED | 字幕/AI 辅助 tabs; mock-WS e2e green |
| SYNC-03 | 01-01, 01-03, 01-04, 01-05 | 全中/全英/双语 toggles; interviewer/user bubbles independent | ✓ SATISFIED | Per-bubble local state (01-03) + phone session mode round-trip republished on same broadcast (01-05); `localPref ?? mode ?? default` |
| SYNC-04 | 01-04 | Screen stays awake with fallback | ✓ SATISFIED (code) | wakeLock.request + hidden-video fallback; real-device pass → human verification |
| SYNC-05 | 01-02, 01-04 | Typewriter streaming render | ✓ SATISFIED | 40 ms cadence + reduced-motion; vitest + e2e checkpoints |
| DSK-01 | 01-02, 01-03 | Mini console 340×680 | ✓ SATISFIED | Window config exact; console hub full |
| DSK-02 | 01-02, 01-03 | Dual-pane 860×680 | ✓ SATISFIED | Window config exact; both panes full |
| DSK-04 | 01-03, 01-05 | Desktop bilingual subtitles + toggle | ✓ SATISFIED | Live 4-round feed + per-bubble toggles; demo e2e green |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | Zero TBD/FIXME/XXX debt markers in phase-modified files | — | Debt-marker gate holds |
| — | — | Zero TODO/HACK/PLACEHOLDER/placeholder copy; zero console.log; zero dangerouslySetInnerHTML/innerHTML in app source; zero hardcoded secrets; zero external CDN refs in both dist trees | — | Clean |

Informational (pre-existing, out of phase scope, logged in `deferred-items.md`, suggested owner Phase 7): `tsc --noEmit` is not a usable gate (missing `@types/react`/`@types/react-dom`); repo-wide `eslint .` is red on pre-existing files outside the phase's changed set; `prettier --check .` flags pre-existing unformatted files. None of these block the phase goal; none are used as verify steps by any plan.

### Human Verification Required

Prerequisite for items 2 and 3: free port 8787 — the dev machine's `tools/jd-inbox-server.mjs` squats `127.0.0.1:8787` (confirmed live via `lsof` during this verification, PID 82377). The app degrades gracefully (bind failure logged, app continues) but real pairing needs the port.

1. **01-03 desktop walkthrough** (`pnpm --filter @nextalk/desktop tauri dev`)
   Test: click through the console hub into every page and back.
   Expected: both windows frame correctly on macOS 12.7 (340×680 + 860×680, transparent, rounded, no black halo); every route reachable with a back affordance; six missing pages show empty and populated states with 模拟数据 badges.
   Why human: transparent-window rendering in the real WKWebView cannot be judged from source or headless Chromium.

2. **01-04 real-device phone pass**
   Test: scan the QR with a real phone → 开始提词 → screen awake ≥2 min → wifi off ~10 s → restore.
   Expected: H5 works on plain `http://192.168.x.x`; wake fallback keeps the screen on; 正在自动重连 during the outage; clean resume with no duplicated/lost subtitles.
   Why human: physical device, real screen-wake behavior, real network interruption.

3. **01-05 full interactive demo**
   Test: QR scan → 开始模拟会话 → watch r1 sync across console + dual + phone → phone count flips to 已连接 1 台设备 → switch phone language mode → 打断 → 重听 → session ends.
   Expected: all surfaces in sync through 4 rounds; status pill idle→listening→generating→ended; per-round strategy cards; 打断 cuts to next round and 重听 replays with fresh seq; phone mode change re-renders un-toggled desktop bubbles.
   Why human: needs GUI + phone + camera; every leg has a green automated equivalent but the physical demo is the phase deliverable.

## Gaps Summary

No gaps. All 22 merged must-haves (plan truths + roadmap Success Criteria) resolve to VERIFIED with first-hand evidence; all 16 key links are WIRED; Level-4 data flow is live from the deterministic simulator to every surface; all 11 requirement IDs are SATISFIED; the anti-pattern and debt-marker gates are clean; and the T-01-06 deviation is an acceptable, tested equivalent (Tauri 2.11 exposes no app-defined ACL namespace — proven from `acl-manifests.json`). The only remaining checks are the three interactive human passes above, each of which already has a green automated equivalent, so the phase is not blocked on continued engineering — it is awaiting human confirmation.

---

_Verified: 2026-09-11T06:28:09Z_
_Verifier: Claude (gsd-verifier)_
