# Walking Skeleton — 极言 NexTalk

**Phase:** 1 (Foundation + Simulation Mode)
**Generated:** 2026-08-27

## Capability Proven End-to-End

"On a simulated audio session, the desktop app opens as a frameless transparent neobrutalism console window; the phone scans the on-screen QR code and joins the LAN WebSocket session; one simulated subtitle event flows desktop → phone and renders with typewriter animation."

This replaces the usual skeleton "DB read/write" with the **LAN WS round-trip** as the one real integration: the same typed `SubtitleDelta` event travels Rust `SessionState` → Tauri `emit` (desktop webviews) + WS broadcast (phone H5), and the phone renders it through the same `useTypewriter` hook the desktop uses. No real audio driver, no cloud APIs — per Phase 1 locked scope (CONTEXT.md D-03, D-04).

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Desktop shell | Tauri 2.11.5 (Rust) + `macos-private-api` | STACK.md lock; ~10MB bundle; WKWebView = Safari 15.6 on the macOS 12.7 floor; transparent frameless windows need the private-API feature |
| Frontend | React 19.2.8 + TypeScript 5.9.3 + Vite 7.3.6 (both apps) | Locked D-01; one shared codebase for desktop webview + phone H5; `build.target: ['safari15','es2022']` MANDATORY (Vite 7 default is Safari 16+) |
| Styling | Tailwind CSS 3.4.19 + hand-rolled tokens (`packages/design-tokens`) | v4 requires Safari 16.4 CSS (`oklch`, `@property`); UI-SPEC locks plain-hex custom properties |
| Window model | Two independent fixed windows: `console` 340×680 (visible) + `dual` 860×680 (hidden until 扩展视图) | Locked D-02; separate JS contexts — all cross-window state flows through Rust, never shared stores |
| "Data layer" | `SessionState` — in-memory `Arc<RwLock>` (pairing token, timeline, language prefs) | v1 is a pure local desktop app; no DB. Timeline doubles as the reconnect-replay buffer for the phone |
| Auth (pairing) | Pairing-as-auth: 128-bit `rand` OsRng token per launch, embedded in QR URL `http://<lan-ip>:8787/?token=X`, checked at WS upgrade (401 on mismatch) | Minimum security boundary for LAN-only demo; no user accounts, no TLS (documented tradeoff) |
| LAN server | axum 0.8.9 (`ws` feature) embedded in the Tauri process; `ServeDir` serves the H5 dist at `/` | Single origin (QR + WS + static), one port 8787, one token check; no cross-origin complexity in dev or prod |
| Mobile H5 | `apps/teleprompter` — React 19 + Vite 7 static build served by axum; browser-native WebSocket | Zero install; same protocol package as desktop |
| Shared protocol | `packages/protocol` — closed discriminated union `ServerEvent` / `ClientMessage` + `isServerEvent` narrowing | One event model, two transports (Tauri emit + WS); Rust mirrors types via serde `deny_unknown_fields` |
| Design tokens | `packages/design-tokens` — `tokens.css` custom properties + Tailwind preset | One source for both surfaces; neobrutalism spec V1.0 exact hex values |
| Icons / fonts | FontAwesome 6.7.2 SVG-core (tree-shaken, pinned — FA7 renames locked icon names) + `@fontsource/space-grotesk` 5.3.0 WOFF2 | Zero CDN (UI-03); offline-safe; Phase 7 re-verifies |
| Polyfills | `core-js/proposals/promise-with-resolvers` at both app entries | `Promise.withResolvers` is absent in Safari ≤17.3 — deps may call it at runtime |
| Test runner | Vitest 4.1.11 (unit, per package) + Playwright 1.62.1 (e2e in real browsers; Tauri shell smoke-tested manually) | A7 tradeoff accepted: WKWebView-specific behavior verified on the real machine, which IS the macOS 12.7 floor |
| Package manager | pnpm via corepack (workspace `apps/*`, `packages/*`) | STACK.md lock; one lockfile |
| Dev-run wiring | `tauri dev`'s `beforeDevCommand` = `concurrently "pnpm --filter @nextalk/desktop dev" "pnpm --filter @nextalk/teleprompter build --watch"` — starts the desktop Vite dev server (port 1420 strictPort, the devUrl source) AND the teleprompter watch build (axum serves its dist on 8787). Never `tauri dev` inside `beforeDevCommand` (self-recursion) | One command runs the whole stack in dev: `pnpm --filter @nextalk/desktop tauri dev` |
| Directory layout | `apps/desktop` (Tauri + both windows' React), `apps/teleprompter` (H5), `packages/protocol`, `packages/design-tokens`; Rust sim/ + lan/ modules under `src-tauri/src` | Source-consumed shared packages (no build step); feature-aligned modules |

## Stack Touched in Phase 1

- [x] Project scaffold — pnpm workspace, Vite 7 (safari15 target), Tailwind 3.4, TS strict, Vitest, Playwright, ESLint/Prettier
- [x] Routing — HashRouter in the desktop app: `#/console`, `#/dual`, + 6 missing pages; H5 tab routing
- [x] "Database read/write" → LAN WS round-trip — axum server + token-checked WS upgrade + broadcast + timeline replay
- [x] UI — at least one interactive element wired to the integration: 开始模拟会话 starts SimSource → subtitle renders on console + dual + phone
- [x] Deployment — documented local-run command exercising the full stack: `pnpm --filter @nextalk/desktop tauri dev` — `beforeDevCommand` concurrently starts the desktop Vite dev server on 1420 (the devUrl source) and the teleprompter watch build; axum serves the built H5 on port 8787

## Out of Scope (Deferred to Later Slices)

- Real audio pipeline, STT/translate/TTS providers, latency rig → Phase 2
- BlackHole virtual device install wizard logic → Phase 3 (UI-only 引导向导 page ships in Phase 1)
- Real window hiding (orderOut), Dock/tray stealth, Cmd+Shift+H real hide → Phase 4 (Phase 1 StealthCard is transitional feedback only)
- Resume indexing, glossary term protection, question-end detection, strategy generation → Phases 4-5 (glossary/resume/strategy UI pages ship in Phase 1 with mock data)
- Dual-track recording, transcript export, review report generation → Phase 6 (UI pages with 模拟数据 mock records ship in Phase 1)
- Vendor API experiments (STT A/B, clone blind tests, RTT) → run between Phase 1 and Phase 2 planning; Phase 1 ships only the experiment framework (D-04)
- Wake-lock/phone behavior beyond the Phase 1 H5 (real-device checks are manual)
- Any cloud backend, accounts, TLS (LAN-only demo; documented security tradeoff)

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions:

- Phase 2: Real cascaded streaming pipeline (STT→translate→TTS, cloned voice) behind the same event model, with the latency rig as the gate
- Phase 3: BlackHole virtual device install wizard + aggregate device + meeting-app audio loop
- Phase 4: Real orderOut stealth (Cmd+Shift+H) + glossary term protection wiring
- Phase 5: Question-end detection + resume-grounded streaming strategy cards
- Phase 6: Consent gate + dual-track recording + transcript export + review report
- Phase 7: Signed/notarized distribution + clean-machine install + offline bundle verification
