# Phase 1: Foundation + Simulation Mode - Research

**Researched:** 2026-08-27
**Domain:** Tauri 2 desktop shell + React 19 workspace, LAN WebSocket sync, SimSource simulated audio pipeline, neobrutalism design system, Safari 15.6 compatibility
**Confidence:** HIGH (Tauri/monorepo/build-target claims verified against official docs and registries; SimSource/vendor-framework sections are design recommendations within locked decisions)

## Summary

Phase 1 is a greenfield build of the complete product UI on simulated audio: a Tauri 2 desktop app with two independent windows (340×680 mini console + 860×680 dual-pane), a phone H5 teleprompter served over LAN, a WebSocket pairing protocol, and a SimSource event engine — no real audio driver, no cloud APIs. All locked decisions (React 19, dual windows, multi-round script, vendor-experiment-skeleton-only) are researchable and implementable; the biggest planning risks are environmental (Rust toolchain and pnpm are NOT installed on this machine) and compatibility (Vite 7's default build target is Safari 16+, so `build.target: 'safari15'` must be explicit or the WKWebView breaks; Promise.withResolvers and similar ES2023 APIs need a polyfill on Safari 15.6).

The dev machine itself is a macOS 12.7.6 Intel Mac — exactly the declared compatibility floor — so Safari 15.6/WKWebView behavior is directly testable in-place with zero extra harness. The workspace should be a pnpm monorepo (`apps/desktop`, `apps/teleprompter`, `packages/protocol`, `packages/design-tokens`) with the H5 served by the embedded Rust axum server (single-origin QR: `http://<lan-ip>:<port>/?token=X`), which eliminates cross-origin complexity in dev and prod alike. FontAwesome must be pinned to 6.7.2 (FA7 is current and renames icons the UI-SPEC locks), fonts come from `@fontsource/space-grotesk` (self-hosted WOFF2), and the transparent frameless window look requires `macos-private-api` + `shadow: false` to avoid the black-halo bug.

**Primary recommendation:** Scaffold as a pnpm workspace with source-consumed shared packages (no build step for `packages/*`), two hash-routed windows in one desktop app, axum (with `ws` feature) as the single LAN server serving both H5 static files and the token-checked WebSocket, and a two-part SimSource (pure script evaluator + wall-clock scheduler) for test determinism. Install Rust via rustup and pnpm via corepack as Wave 0 prerequisites.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### 前端技术栈
- **D-01:** 前端框架锁定 **React 19 + TypeScript**（+ Vite 7 构建）。需求文档曾提 "React/Vue" 二选一，经确认选 React（与 STACK.md 研究一致，桌面端与手机 H5 共享组件与类型）。不用 Vue。

#### 桌面窗口形态
- **D-02:** **双独立 Tauri 窗口**：340×680 微型控制台与 860×680 双栏视图是两个独立窗口，各自管理。控制台为中枢（导航入口），双栏承载会话直播。与参考 HTML 双屏视觉一致；Phase 4 隐形模式将隐藏所有窗口（orderOut），音频引擎独立进程持续运行。

#### 仿真演示脚本
- **D-03:** 仿真会话使用**多轮英文面试脚本**（3-4 个技术问题），完整演示：面试官英文提问 → 双语字幕 → AI 策略卡片联动 → 用户中文回答 → 打字机渲染 → 生成中指示 → 打断/重说等状态。参考 HTML 的「数据库优化」场景作为其中第一轮。脚本为 Phase 1 内置 mock 数据（标注「模拟数据」），不做外部可配置（配置 UI 属后续阶段）。

#### 供应商决定性实验
- **D-04:** Phase 1 **只搭建实验框架**（评估脚本、盲测集设计、RTT 测量工具骨架），真实 API 实验（中文 STT A/B、克隆音色盲测、网络 RTT）留到 Phase 2 规划前执行——届时需要用户提供 API keys。本阶段不阻塞 UI 开发。

### Claude's Discretion
用户将以下实现细节交由 Claude 决定（研究 + 规划代理自行决策，无需再问）：
- 工作区结构（Tauri app + 共享协议包 + 手机 H5 的组织方式，monorepo 方案）
- TypeScript 严格模式配置、测试框架选择（倾向 Vitest）、ESLint/格式化配置
- 组件文件组织与命名（按 UI-SPEC 组件清单：NexTalkBrand、StealthCard、ChatBubble 等）
- WebSocket 协议消息格式的具体设计（在 UI-SPEC 约束的语义下）
- 仿真事件流的节奏控制参数（打字机速度、间隔时长等微调值）

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| UI-01 | 新粗野主义设计系统落地 | `packages/design-tokens` (tokens.css + Tailwind preset, plain hex, no v4 features), Tailwind 3.4.19, FontAwesome 6.7.2 SVG-core, Space Grotesk via `@fontsource/space-grotesk` 5.3.0 — see Standard Stack + Design Tokens pattern |
| UI-02 | 参考 HTML 4 屏实现 + 缺页补齐 (6 pages) | React 19.2.8 + react-router-dom 7.18.2 HashRouter in the console window; component inventory per UI-SPEC; reference HTML CDN classes reimplemented as local tokens (see Anti-Pattern: no CDN classes) |
| UI-03 | 零 CDN 本地打包 (Safari 15.6 / WKWebView) | `build.target: ['safari15', 'es2022']` mandatory (Vite 7 default is Safari 16+), core-js promise-with-resolvers polyfill, Tailwind v3.4.19 pinned (v4 requires Safari 16.4+ CSS), FA 6.7.2 tree-shaken, fontsource self-hosted WOFF2 |
| SYNC-01 | 局域网 WebSocket 服务 + 二维码配对 (token 认证) | axum 0.8.9 `ws` feature + tower-http ServeDir + rand 0.10.2 token + local-ip-address 0.6.13; pairing-as-auth pattern (QR URL carries token, WS upgrade checks it); JS `qrcode` 1.5.4 renders in webview |
| SYNC-02 | 手机 H5 提词器 (上字幕下策略) | `apps/teleprompter` standalone React+Vite static build served by axum over LAN; browser-native WebSocket; protocol package shared with desktop |
| SYNC-03 | 语言切换 (全中/全英/双语, 逐气泡) | Protocol `ControlMessage: { language_prefs }` + UI-SPEC LanguageToggle component; re-render on event, no refetch (see Code Example: protocol types) |
| SYNC-04 | 手机屏幕常亮 (Wake Lock 回退) | Native Wake Lock API unavailable on `http://192.168.x.x` (needs secure context; iOS 16.4+); hidden looping muted video fallback (NoSleep.js technique); user gesture required on iOS even for the video → 「开始提词」 button is the gesture (UI-SPEC copy already locked) |
| SYNC-05 | 打字机流式渲染 | 30-50ms/char interval on committed text (UI-SPEC); instant render under `prefers-reduced-motion`; deterministic for tests via injected interval + Vitest fake timers (see Pattern: Typewriter) |
| DSK-01 | 微型控制台 340×680 | Tauri window config (label `console`, fixed 340×680, non-resizable, frameless transparent + `shadow:false`); components per UI-SPEC (NexTalkBrand, StealthCard, QrCodeCard, KnowledgeRow, bottom action bar) |
| DSK-02 | 双栏扩展视图 860×680 | Tauri window config (label `dual`, fixed 860×680); hash route `#/dual` in same app; both windows subscribe to Rust event stream (separate JS contexts — state flows through Rust, not shared stores) |
| DSK-04 | 桌面双语字幕 + 单语/双语切换 | ChatBubble + LanguageToggle components consuming subtitle events emitted from Rust; same event semantics as H5 WS (one event model, two transports) |

</phase_requirements>

## Project Constraints (from CLAUDE.md)

Directives extracted from `<home>/nexTalk/CLAUDE.md` that the planner must honor:

- **GSD workflow enforcement:** No direct repo edits outside a GSD workflow — work must start through `/gsd:quick`, `/gsd:debug`, or `/gsd:execute-phase`. Planning artifacts must stay in sync with execution.
- **Tech stack (constraints section):** Tauri + Rust desktop; mobile H5 (React + WebSocket); AI capabilities all cloud-API (none in Phase 1 — SimSource only); macOS 12.7 compatibility; neobrutalism design system per spec V1.0 (green=user/pronunciation, yellow=AI/strategy, blue=translation/system); reference HTML is the visual baseline.
- **Privacy:** local-only storage (no own cloud backend).
- **Core Value:** ≤2s end-to-end latency is the invariant all later phases serve — Phase 1's SimSource pacing (typewriter 30-50ms/char) must not pre-commit any real-pipeline pacing decision.
- Project skills: none registered (`.claude/skills/`, `.agents/skills/` absent — verified).
- Stack/architecture/pitfalls research (STACK.md, ARCHITECTURE.md, PITFALLS.md) is authoritative project research — this document extends it with Phase-1-specific, version-verified detail; do not contradict it.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Design system tokens (colors, shadows, type scale) | Shared package (browser) | — | Same tokens consumed by desktop webview and H5; one source in `packages/design-tokens` |
| Desktop window management (two windows, transparency, frameless) | Rust core (Tauri) | — | Window config lives in `tauri.conf.json` + Rust; webview has no OS window authority |
| IPC between webview and Rust (events, commands) | Rust core (Tauri) | Browser (listen/emit) | Tauri event bus is the only channel between the two separate webview contexts |
| LAN WebSocket server + pairing | Rust core (backend) | Browser (WS client) | axum embedded in the Tauri Rust process; H5 and future pipeline stages are clients |
| QR pairing (token generation + URL encoding) | Rust core (token) | Browser (QR render) | Token must be generated server-side (rand); rendering is a pure browser concern (qrcode lib) |
| H5 static serving | Rust core (backend) | — | axum ServeDir serves `apps/teleprompter/dist`; keeps QR single-origin |
| SimSource event engine (script → events) | Rust core (backend) | — | Substitutes for the audio pipeline layer; feeds the same event model the real pipeline will emit |
| Typewriter rendering + reduced-motion collapse | Browser | — | Pure rendering concern; identical hook in desktop and H5 |
| Wake lock + fallback | Browser | — | Web API concern on the phone only |
| Vendor experiment framework | Tooling (repo-level scripts) | — | Standalone scripts + JSON test-set format, no runtime code |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tauri (Rust crate) | 2.11.5 | Desktop shell (Rust core + WKWebView) | Locked by STACK.md; MSRV 1.77.2 (verified via crates.io API); needs `features = ["macos-private-api"]` for transparent windows |
| @tauri-apps/api | 2.11.1 | Webview IPC (events, window ops) | Official companion to tauri 2.11 [ASSUMED — npm registry confirmed, slopcheck unavailable] |
| @tauri-apps/cli | 2.11.4 | `tauri dev` / `tauri build` | Official CLI [ASSUMED — npm registry confirmed] |
| react / react-dom | 19.2.8 | UI framework (desktop + H5 shared) | Locked D-01; React supports Safari 12+ [ASSUMED — npm registry confirmed] |
| vite | 7.3.6 | Build tool both apps | Locked "Vite 7" by STACK.md; latest v7 line (v8.2.2 exists — do NOT upgrade, see State of the Art) [ASSUMED — npm registry confirmed] |
| @vitejs/plugin-react | 6.1.0 | React fast-refresh/transform | Standard Vite React plugin [ASSUMED — npm registry confirmed] |
| typescript | 5.9.3 | Type checking | Latest 5.x line (TS 7.0.2 is the Go-port — avoid, ecosystem risk) [ASSUMED — npm registry confirmed] |
| tailwindcss | 3.4.19 | Utility CSS | Locked v3.4 (4.3.3 is current — do NOT upgrade; v4 requires Safari 16.4+ CSS features); 3.4.19 published 2026-08-14, still maintained [ASSUMED — npm registry confirmed] |
| postcss + autoprefixer | 8.5.26 / 10.5.4 | Tailwind v3 pipeline | Required peer pipeline for tailwindcss@3 [ASSUMED — npm registry confirmed] |
| axum | 0.8.9 | LAN HTTP + WebSocket server (embedded) | Standard tokio-native server; `ws` feature provides WebSocketUpgrade (uses tokio-tungstenite internally) [VERIFIED: crates.io API] |
| tower-http | 0.7.0 | ServeDir static H5 hosting | Standard static-file middleware for axum [VERIFIED: crates.io API] |
| tokio | 1.53.1 | Async runtime (full features) | De-facto async runtime; Tauri brings its own tokio — reuse it [VERIFIED: crates.io API] |
| futures-util | 0.3.34 | Stream/sink split for WS | Required for concurrent WS read/write with axum's WebSocket [VERIFIED: crates.io API] |
| rand | 0.10.2 | Pairing token generation | Standard RNG; `rand::rngs::OsRng` for token bytes [VERIFIED: crates.io API] |
| local-ip-address | 0.6.13 | LAN IP discovery | Small maintained crate wrapping getifaddrs [VERIFIED: crates.io API] |
| serde / serde_json | 1.0.229 / 1.0.151 | Protocol message serialization (Rust mirror) | Standard for JSON over WS [VERIFIED: crates.io API] |
| @fontsource/space-grotesk | 5.3.0 | Self-hosted Space Grotesk WOFF2 (400/600/700) | Standard self-hosting package; imports `400.css`/`600.css`/`700.css`; zero CDN [ASSUMED — npm registry confirmed] |
| @fortawesome/fontawesome-free | **6.7.2** (pin @6) | Icon set (UI-SPEC locks FA6 naming) | FA7.3.1 is current and renames icons — pin 6.7.2 (latest v6) [ASSUMED — npm registry confirmed] |
| @fortawesome/free-solid-svg-icons + fontawesome-svg-core + react-fontawesome | 6.7.2 + 6.7.2 + 3.5.0 | Tree-shaken solid icons in React | Official FA React path; react-fontawesome 3.5.0 peer-dep `~6 \|\| ~7` — works with FA6 [ASSUMED — npm registry confirmed] |
| qrcode (npm) | 1.5.4 | QR rendering in webview (canvas/dataURL) | Pure-JS, no Rust image encoding needed; render from pairing URL string [ASSUMED — npm registry confirmed, github.com/soldair/node-qrcode] |
| react-router-dom | 7.18.2 | Hash routing in desktop app (`#/console`, `#/dual`, `#/glossary`…) | Battle-tested; HashRouter fits Tauri SPA fallback; v8.3.0 exists — stay on v7 line [ASSUMED — npm registry confirmed] |
| vitest | 4.1.11 | Unit tests (both apps + packages) | Standard Vite-native test runner (discretion: "倾向 Vitest" confirmed) [ASSUMED — npm registry confirmed] |
| @testing-library/react | 16.3.2 | Component tests (typewriter, bubbles, toggles) | Standard React test utilities, supports React 19 [ASSUMED — npm registry confirmed] |
| jsdom | 30.0.1 | DOM environment for vitest | Standard [ASSUMED — npm registry confirmed] |
| @playwright/test | 1.62.1 | E2E + visual regression (desktop UI in browser, H5 over LAN server) | Standard E2E; test the React apps in real browsers (Tauri shell itself is manually smoke-tested) [ASSUMED — npm registry confirmed] |

### Supporting

| Library | Purpose | When to Use |
|---------|---------|-------------|
| core-js (proposal/promise-with-resolvers) | Polyfill ES2023 `Promise.withResolvers` (absent in Safari ≤17.3) | At every app entry; only polyfill needed if deps use it — add defensively, ~small size [ASSUMED] |
| @tauri-apps/plugin-global-shortcut 2.3.2 | Cmd+Shift+H registration | Optional in Phase 1 (stealth transition feedback only; real hiding is Phase 4) — only if the demo wants keyboard-triggered transition; otherwise trigger from StealthCard click [ASSUMED — npm registry confirmed] |
| tauri-plugin-single-instance | — | Not Phase 1 (would prevent double-launch demo windows) — skip |
| concurrently | — | Root dev script: `tauri dev` + teleprompter `vite build --watch` [ASSUMED] |
| eslint / prettier | — | Formatting/lint per project conventions; versions at planner's discretion |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| FA SVG-core (tree-shaken) | `fontawesome-subset` webfont tool (subset woff2 + CSS classes) | SVG-core is the official React path, no font assets, automatic subsetting; webfont keeps `fa-*` class strings from reference HTML but adds a third-party build tool [ASSUMED] |
| JS `qrcode` in webview | Rust `qrcode` crate (0.14.1) + PNG encode → Tauri asset | JS is ~10 lines and no Rust image dependencies; Rust path matters only if QR must be generated off-webview |
| axum (single server: static + WS) | tokio-tungstenite standalone + separate static server | axum's ws feature wraps tokio-tungstenite; one process, one port, one token check — fewer moving parts |
| pnpm workspaces | npm workspaces | pnpm is locked by STACK.md and faster; npm 11 workspaces would work but pnpm is the decision |
| react-router-dom v7 HashRouter | Hand-rolled `useHashRoute` hook | Router is battle-tested, tiny tree-shaken, zero-config back/link semantics; a hand-rolled one would need tests for the same behavior |
| react-router-dom v7 | react-router v8 | v8 is new (8.3.0); v7 is the stable, documented line — no feature in Phase 1 needs v8 |

**Installation:**
```bash
# 0. Toolchain (blockers — see Environment Availability)
rustup default stable
corepack enable && corepack prepare pnpm@latest --activate   # or: npm install -g pnpm

# 1. Workspace deps (single lockfile)
pnpm install

# 2. Desktop app (apps/desktop)
pnpm add react react-dom react-router-dom @tauri-apps/api @tauri-apps/cli -w
pnpm add tailwindcss@3.4.19 postcss autoprefixer -w
pnpm add -D typescript@5.9.3 vite@7.3.6 @vitejs/plugin-react vitest @testing-library/react jsdom @playwright/test

# 3. Shared packages consumed by both apps (source imports, no build step)
pnpm --filter @nextalk/desktop add @nextalk/protocol @nextalk/design-tokens --workspace
pnpm --filter @nextalk/teleprompter add @nextalk/protocol @nextalk/design-tokens --workspace

# 4. Rust deps (apps/desktop/src-tauri)
cargo add tauri --features macos-private-api
cargo add axum --features ws
cargo add tower-http --features fs
cargo add tokio --features full
cargo add futures-util rand serde serde_json local-ip-address
```

**Version verification performed (2026-08-27):** All npm versions above confirmed via `npm view` (registry); all Rust versions confirmed via crates.io API. Tailwind 3.4.19 modified 2026-08-14 (maintained v3 line). Tauri 2.11.5 MSRV 1.77.2, published 2026-07-01. `react-fontawesome@3.5.0` peer-dep `@fortawesome/fontawesome-svg-core: ~6 || ~7` — compatible with pinned FA6.

## Package Legitimacy Audit

> slopcheck could NOT be installed (pip 21.2.4 too old; install failed). Per protocol graceful degradation, ALL npm packages below are tagged `[ASSUMED]` and the planner MUST gate each install behind a `checkpoint:human-verify` task. Rust crates were verified against the crates.io official API (the authoritative registry for Rust).

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| tauri / tauri-build 2.11.5 / 2.6.3 | crates.io | ~3 yrs | — | github.com/tauri-apps/tauri | unavailable | Approved (crates.io API verified) |
| axum 0.8.9 | crates.io | ~5 yrs | — | github.com/tokio-rs/axum | unavailable | Approved (crates.io API verified) |
| tower-http 0.7.0 | crates.io | ~4 yrs | — | github.com/tower-rs/tower-http | unavailable | Approved (crates.io API verified) |
| tokio 1.53.1 | crates.io | ~8 yrs | — | github.com/tokio-rs/tokio | unavailable | Approved (crates.io API verified) |
| futures-util 0.3.34 | crates.io | ~8 yrs | — | github.com/rust-lang/futures-rs | unavailable | Approved (crates.io API verified) |
| rand 0.10.2 | crates.io | ~10 yrs | — | github.com/rust-random/rand | unavailable | Approved (crates.io API verified) |
| local-ip-address 0.6.13 | crates.io | ~3 yrs | — | github.com/EstebanBorai/local-ip-address | unavailable | Approved (crates.io API verified) |
| serde / serde_json | crates.io | ~10 yrs | — | github.com/serde-rs | unavailable | Approved (crates.io API verified) |
| react / react-dom 19.2.8 | npm | 13 yrs | ~40M/wk | github.com/facebook/react | unavailable | Approved — gate behind human-verify checkpoint (per degradation rule) |
| vite 7.3.6 | npm | 7 yrs | ~30M/wk | github.com/vitejs/vite | unavailable | Approved — gate behind checkpoint |
| @vitejs/plugin-react 6.1.0 | npm | 6 yrs | ~15M/wk | github.com/vitejs/vite-plugin-react | unavailable | Approved — gate behind checkpoint |
| tailwindcss 3.4.19 | npm | 7 yrs | ~35M/wk | github.com/tailwindlabs/tailwindcss | unavailable | Approved — gate behind checkpoint |
| @fontsource/space-grotesk 5.3.0 | npm | 6 yrs | ~600K/wk | github.com/fontsource/font-files | unavailable | Approved — gate behind checkpoint |
| @fortawesome/fontawesome-free 6.7.2 | npm | 12 yrs | ~15M/wk | github.com/FortAwesome/Font-Awesome | unavailable | Approved — gate behind checkpoint |
| @fortawesome/free-solid-svg-icons 6.7.2 | npm | 7 yrs | ~8M/wk | github.com/FortAwesome/Font-Awesome | unavailable | Approved — gate behind checkpoint |
| @fortawesome/fontawesome-svg-core 6.7.2 | npm | 7 yrs | ~10M/wk | github.com/FortAwesome/Font-Awesome | unavailable | Approved — gate behind checkpoint |
| @fortawesome/react-fontawesome 3.5.0 | npm | 7 yrs | ~5M/wk | github.com/FortAwesome/react-fontawesome | unavailable | Approved — gate behind checkpoint |
| @tauri-apps/api 2.11.1 / cli 2.11.4 | npm | 3 yrs | ~1M/wk | github.com/tauri-apps/tauri | unavailable | Approved — gate behind checkpoint |
| qrcode 1.5.4 | npm | 10 yrs | ~2M/wk | github.com/soldair/node-qrcode | unavailable | Approved — gate behind checkpoint (no postinstall script — verified) |
| react-router-dom 7.18.2 | npm | 9 yrs | ~8M/wk | github.com/remix-run/react-router | unavailable | Approved — gate behind checkpoint |
| vitest 4.1.11 | npm | 5 yrs | ~6M/wk | github.com/vitest-dev/vitest | unavailable | Approved — gate behind checkpoint |
| @testing-library/react 16.3.2 | npm | 8 yrs | ~8M/wk | github.com/testing-library/react-testing-library | unavailable | Approved — gate behind checkpoint |
| jsdom 30.0.1 | npm | 14 yrs | ~25M/wk | github.com/jsdom/jsdom | unavailable | Approved — gate behind checkpoint |
| @playwright/test 1.62.1 | npm | 5 yrs | ~4M/wk | github.com/microsoft/playwright | unavailable | Approved — gate behind checkpoint |
| core-js | npm | 11 yrs | ~25M/wk | github.com/zloirock/core-js | unavailable | Approved — gate behind checkpoint |
| @tauri-apps/plugin-global-shortcut 2.3.2 | npm | 3 yrs | — | github.com/tauri-apps/plugins-workspace | unavailable | Approved (optional) — gate behind checkpoint |
| concurrently | npm | 9 yrs | ~5M/wk | github.com/open-cli-tools/concurrently | unavailable | Approved (dev) — gate behind checkpoint |
| fontawesome-subset | npm | — | low | github.com/StudioJanSchubert/fontawesome-subset | unavailable | NOT recommended (SVG-core chosen) — no checkpoint needed |
| qrcode (Rust crate) 0.14.1 | crates.io | 10 yrs | — | github.com/kennytm/qrcode-rust | unavailable | Optional fallback — not in recommended path |

**Packages removed due to slopcheck [SLOP] verdict:** none (slopcheck unavailable).
**Packages flagged as suspicious [SUS]:** none — all packages are long-established with source repos and no postinstall scripts (postinstall verified for FA, qrcode, fontsource).
**Note:** Because slopcheck was unavailable, the planner MUST insert a `checkpoint:human-verify` task before the first install of each npm package batch (a single checkpoint before the workspace install step is acceptable).

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Rust core (single process, apps/desktop/src-tauri)                          │
│                                                                              │
│  ┌──────────────┐   ┌───────────────────────┐   ┌────────────────────────┐  │
│  │ SessionState  │   │ SimSource             │   │ LAN server (axum)      │  │
│  │ (Arc<RwLock>) │◀──│  script.json          │   │  GET / → H5 static     │  │
│  │  • timeline    │   │  pure evaluator      │   │  GET /ws?token= → WS   │  │
│  │  • token       │   │  + wall-clock sched. │   │  (token check on       │  │
│  │  • language    │   │  → typed events      │   │   upgrade)             │  │
│  │   prefs        │   └──────────┬───────────┘   └───────┬───────────────┘  │
│  └──────┬────────┘              │                        │                  │
│         │ emit (Tauri events)   │ WS broadcast (same types)                 │
├─────────┼───────────────────────┼───────────────────────────────────────────┤
│         ▼                       ▼                        ▼                  │
│  ┌──────────────┐      ┌────────────────┐      ┌──────────────────────┐     │
│  │ Webview:      │      │ Webview:       │      │ Phone browser (H5):  │     │
│  │ console 340×680│     │ dual 860×680   │      │ http://<lan-ip>:8787  │     │
│  │ hub nav + QR  │      │ subtitles + AI │      │ ?token=X → WS same-  │     │
│  │ + missing pages│     │ pane           │      │ origin, typewriter,  │     │
│  │ (hash routes) │      │ (hash #/dual)  │      │ tabs, wake-lock       │     │
│  └──────────────┘      └────────────────┘      └──────────────────────┘     │
│                                                                              │
│  QR URL: http://<lan-ip>:8787/?token=<rand>   ← rendered by qrcode in console│
└─────────────────────────────────────────────────────────────────────────────┘

Event flow (one demo round):
SimSource script → evaluator emits [QuestionEvent → SubtitleDeltas → StrategyCard → UserAnswerDeltas → GeneratingDots]
→ SessionState timeline append → (a) Tauri emit to console+dual webviews, (b) WS broadcast to H5
→ both surfaces render with the SAME useTypewriter hook (30-50ms/char)
→ H5 sends ControlMessage (language change) → SessionState → re-render on all surfaces
```

### Recommended Project Structure

```
nexTalk/
├── pnpm-workspace.yaml            # packages: apps/*, packages/*
├── package.json                   # root scripts: dev (concurrently), build, test
├── apps/
│   ├── desktop/                   # Tauri 2 + React 19 + Vite 7 (both windows)
│   │   ├── package.json
│   │   ├── vite.config.ts         # build.target: ['safari15','es2022']
│   │   ├── tailwind.config.js     # preset: @nextalk/design-tokens
│   │   ├── index.html
│   │   ├── src/
│   │   │   ├── main.tsx           # core-js polyfill import first, HashRouter
│   │   │   ├── App.tsx            # routes: /console /dual /setup /voice /glossary /resume /recordings /review
│   │   │   ├── pages/             # ConsolePage, DualPanePage, SetupWizardPage, VoiceEnrollmentPage,
│   │   │   │                      #   GlossaryPage, ResumeImportPage, RecordingsPage, ReviewPage
│   │   │   ├── components/        # per UI-SPEC inventory (NexTalkBrand, StealthCard, QrCodeCard,
│   │   │   │                      #   ChatBubble, LanguageToggle, AiTimeline, WizardShell, ...)
│   │   │   ├── hooks/             # useTauriEvents, useTypewriter, useHashRoute (thin wrapper)
│   │   │   └── styles/            # global.css (dot matrix, tokens import)
│   │   └── src-tauri/
│   │       ├── Cargo.toml         # tauri features=["macos-private-api"], axum(ws), ...
│   │       ├── tauri.conf.json    # app.windows[console|dual], bundle.minimumSystemVersion "12.0"
│   │       ├── capabilities/default.json
│   │       └── src/
│   │           ├── main.rs / lib.rs
│   │           ├── state.rs       # SessionState (token, timeline, language prefs)
│   │           ├── lan/
│   │           │   ├── server.rs  # axum router: ServeDir + /ws handler + IP discovery
│   │           │   └── protocol.rs# serde mirror of @nextalk/protocol types
│   │           └── sim/
│   │               ├── source.rs  # AudioSource trait (Phase 1: SimSource impl only)
│   │               ├── script.rs  # script.json schema + pure evaluator
│   │               └── scheduler.rs  # wall-clock driver (injectable time source)
│   └── teleprompter/              # Phone H5 (React 19 + Vite 7, static build)
│       ├── package.json
│       ├── vite.config.ts         # base:'./', build outDir
│       └── src/
│           ├── main.tsx           # core-js polyfill first
│           ├── App.tsx            # tabs: 字幕 / AI 辅助
│           ├── components/        # MobileTabs, StatusCapsule, ChatBubble, TypewriterDots, ...
│           ├── hooks/             # useWs, useWakeLock, useTypewriter
│           └── styles/            # global.css
└── packages/
    ├── protocol/                  # @nextalk/protocol — WS message types + narrowing (TS source)
    │   └── src/index.ts
    └── design-tokens/             # @nextalk/design-tokens — tokens.css + tailwind preset (TS/CSS source)
        └── src/
            ├── tokens.css         # CSS custom properties (plain hex, Safari 15.6)
            └── tailwind-preset.js # theme.extend colors/boxShadow/fontFamily
```

**Build order (root scripts):**
1. `pnpm install` (one lockfile; `workspace:*` links)
2. Dev: `concurrently "pnpm --filter @nextalk/teleprompter build --watch" "pnpm --filter @nextalk/desktop tauri dev"` — Rust axum serves `apps/teleprompter/dist` from disk in dev
3. Prod: `pnpm --filter @nextalk/teleprompter build` → `pnpm --filter @nextalk/desktop tauri build` (beforeBuildCommand runs desktop `vite build`)

### Pattern 1: Pairing-as-Auth (SYNC-01)
**What:** The QR code embeds a short-lived random token in the URL query; the WS upgrade handler rejects connections whose token does not match the session token. No user accounts, no TLS — the minimum security boundary for a LAN-only demo.
**When to use:** Any device pairing without an identity system.
**Example:**
```rust
// Source: verified pattern from ARCHITECTURE.md (research) + axum 0.8 docs pattern
let token: String = rand::rng().random::<[u8; 16]>() // hex-encode
// QR content: http://{lan_ip}:8787/?token={token}
async fn ws_handler(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<SessionState>>,
    ws: WebSocketUpgrade,
) -> Response {
    if params.get("token") != Some(&state.pairing_token) {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    ws.on_upgrade(|socket| handle_client(socket, state))
}
```

### Pattern 2: One Event Model, Two Transports (DSK-04 + SYNC-02)
**What:** The Rust core emits the same typed event (e.g., `SubtitleDelta`) through two transports: Tauri `emit` to the two desktop webviews and WS broadcast to the H5. Desktop and phone renderers consume identical shapes, so one protocol package + one hook set (typewriter, bubbles) is shared.
**When to use:** Any multi-surface app where renderers must not diverge.
**Key implication:** The two desktop windows are separate JS contexts — they CANNOT share stores; all cross-window state flows through Rust (`SessionState`).

### Pattern 3: Deterministic SimSource (D-03)
**What:** Split the simulator into (a) a **pure evaluator** `script_state(elapsed_ms) -> Vec<SimEvent>` (no I/O, no clock — unit-testable with any elapsed value) and (b) a **scheduler** that ticks a wall clock (or an injected fake time source in tests) and feeds the evaluator. Events carry explicit timestamps; the evaluator derives everything from `elapsed_ms`, so tests can fast-forward a 3-round script in milliseconds.
**When to use:** Any simulated event source that must be demoable AND testable.
**Script schema (mock data, marked 模拟数据):**
```jsonc
{
  "rounds": [
    {
      "id": "r1", "tag": "模拟数据",
      "interviewer_en": "Your database queries are slowing down under load...",
      "interviewer_zh": "你的数据库查询在负载下变慢了……",
      "user_zh": "我们当时从三方面入手……",
      "user_en_optional": null,
      "strategy": { "title": "数据库优化", "bullets": ["慢查询日志定位", "拆连表查询", "Redis 缓存层"] }
    }
    // r2..r4: additional technical questions; r1 MUST be the DB-optimization scenario (locked)
  ]
}
```

### Pattern 4: Wake Lock Progressive Enhancement (SYNC-04)
**What:** Feature-detect `navigator.wakeLock`; request inside the user gesture (「开始提词」); re-acquire on `visibilitychange`; on failure or absence (always the case on `http://192.168.x.x`), start a hidden looping muted video (`playsinline muted loop`, ~10KB data-URI or bundled tiny MP4). Show the 已启用防休眠回退模式 toast per UI-SPEC.
**When to use:** Any mobile web surface needing stay-awake outside secure contexts.
**Verified facts:** Wake Lock requires secure context + user activation (Safari 16.4+); broken in home-screen PWA mode on iOS 17.x (WebKit bug 205104); the hidden-video fallback itself requires a user gesture on iOS (Safari treats even muted video as audio playback).

### Anti-Patterns to Avoid
- **CDN class names in the reimplemented UI:** The reference HTML uses `cdn.tailwindcss.com`, FA `all.min.css` CDN, and Google Fonts. Reimplement with local tokens + local fonts — no `<link>` to any CDN anywhere (UI-03, verified in Phase 7).
- **One shared React store across windows:** Windows are separate webviews; a store would silently desync. All cross-window state via Rust events.
- **WS without replay:** If the H5 reconnects after the phone locks, it must receive the session timeline back (Rust keeps it in `SessionState`) — otherwise the screen comes back blank and the demo looks broken.
- **Vite 7 defaults:** `build.target` default `baseline-widely-available` = Safari 16.0 floor → MUST set `['safari15', 'es2022']` explicitly in both apps (verified: Vite 7 migration guide).
- **Splitting H5 serving origins in dev:** Don't point the QR at the Vite dev server in dev and axum in prod — serve the built H5 from axum in both modes (`vite build --watch` in dev). One code path, one origin.
- **`:has()` / `oklch` / `color-mix` / `@property`:** All forbidden by UI-SPEC compatibility contract (Safari 15.6 floor). Tailwind v3.4 does not emit them by default; keep token values plain hex.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WebSocket server (upgrade, framing, ping/pong) | Raw tokio-tungstenite accept loop | axum `ws` feature (WebSocketUpgrade) | axum wraps tokio-tungstenite with router integration, close handshake, ping handling; raw loops re-implement RFC-6455 edge cases |
| Static file serving for H5 | Custom file reader + content types | tower-http `ServeDir` | MIME types, range requests, path safety — solved |
| QR code generation | Custom matrix/error-correction | `qrcode` npm (1.5.4) | QR error correction levels, version selection, rendering — a spec-complete library is required for phone scanners to read it |
| Random session token | `std::time`-based "random" strings | rand crate (`OsRng`) | Predictable tokens are trivially forgeable — rand is audited |
| LAN IP discovery | Parsing `ifconfig` output | `local-ip-address` crate | Cross-interface parsing, IPv4/IPv6 handling — solved and maintained |
| Self-hosted font pipeline | Downloading WOFF2 by hand + hand-writing @font-face | `@fontsource/space-grotesk` | Correct unicode-range subsets, preload hints, versioned files, OFL licensing metadata |
| Icon subsetting | Hand-editing FA font files | `@fortawesome/*-svg-icons` tree-shaking (official FA React path) | Tree-shaking includes exactly the imported icons; no font assets at all |
| Typewriter animation | Custom rAF animation engine | 15-line interval hook on committed text | UI-SPEC contract is text-interval based; simpler is correct here (this one IS worth hand-rolling — it is the design contract, kept trivial) |

**Key insight:** Every "hard" dependency above (WS framing, QR encoding, file serving, token RNG) is a correctness-or-security-critical algorithm where the ecosystem solution is the de-facto standard and hand-rolling is a defect magnet. The design system, by contrast, is deliberately hand-rolled per the locked spec — that is the product's identity, not a solved problem.

## Common Pitfalls

### Pitfall 1: Vite 7 emits Safari-16+ JavaScript by default
**What goes wrong:** The WKWebView on macOS 12.7 (Safari 15.6) throws syntax errors on load; the app window is blank in the packaged build while `tauri dev` looks fine on modern dev browsers.
**Why it happens:** Vite 7 changed `build.target` default from `'modules'` to `baseline-widely-available` (Safari 16.0 floor) — verified in the Vite 7 migration guide.
**How to avoid:** `build: { target: ['safari15', 'es2022'] }` in BOTH apps' vite.config.ts; add a CI-ish smoke check that the built bundle contains no `??=`-class newer syntax (or just verify the packaged app opens on this machine — it IS the floor).
**Warning signs:** Blank window in `tauri build` output; dev tools console shows `SyntaxError: Unexpected token`.

### Pitfall 2: Transparent frameless windows render a black rectangle halo
**What goes wrong:** The 340×680 and 860×680 windows with `rounded-3xl` CSS corners show a black box around the rounded corners — the macOS window server draws a default shadow behind the rectangular window.
**Why it happens:** Transparent windows require `transparent: true` + `macos-private-api` feature (config + Cargo) AND `shadow: false`; missing any of the three breaks the look (verified: community reports + tauri config docs).
**How to avoid:** Set all three in tauri.conf.json (`decorations: false, transparent: true, shadow: false`) + `tauri = { features = ["macos-private-api"] }`; CSS `html, body { background: transparent }` and root container `rounded-3xl overflow-hidden`. App Store note: private API disqualifies MAS — irrelevant here (direct distribution), but record it.
**Warning signs:** Black rectangle corners visible on desktop; window looks square.

### Pitfall 3: Wake Lock silently no-ops on the phone (SYNC-04)
**What goes wrong:** `navigator.wakeLock.request('screen')` returns a promise that rejects (NotAllowedError) or the property doesn't exist; screen locks mid-demo.
**Why it happens:** `http://192.168.x.x` is not a secure context → API absent; even where present (iOS 16.4+), it requires user activation and fails in home-screen PWA mode (WebKit bug).
**How to avoid:** Feature-detect + call inside the 开始提词 click handler; catch rejection → start hidden looping muted video (also needs the gesture); toast 已启用防休眠回退模式; re-request on visibilitychange.
**Warning signs:** Screen locks during demo; console shows `NotAllowedError` on wakeLock.request.

### Pitfall 4: Two windows desync (DSK-01 + DSK-02)
**What goes wrong:** Console shows session started while dual-pane shows stale state; language toggle in one window doesn't affect the other.
**Why it happens:** Each window is a separate webview/JS context; local stores do not synchronize.
**How to avoid:** All cross-window state through Rust `SessionState` + Tauri events; renderers subscribe and re-render. No window-local mutable session state.
**Warning signs:** UI state differs between windows after any interaction.

### Pitfall 5: ES2023 APIs break Safari 15.6 at runtime
**What goes wrong:** "Promise.withResolvers is not a function" — typically from a dependency (React-PDF case documented), crashing at runtime.
**Why it happens:** esbuild transpiles syntax but never polyfills APIs; `Promise.withResolvers` arrived in Safari 17.4 (verified via caniuse).
**How to avoid:** `import 'core-js/proposals/promise-with-resolvers'` first in both entries; keep code to ES2022 features; grep-check deps for withResolvers usage in the lockfile at scaffold time.
**Warning signs:** Runtime TypeError only on the WKWebView/old phone, works in Chrome.

### Pitfall 6: Toolchain blockers (pnpm, Rust) not installed
**What goes wrong:** Plans assume `pnpm` and `cargo` exist; first `pnpm install` / `cargo build` fails.
**Why it happens:** This machine has npm 11.12.1 + Node 24.15.0 + corepack 0.34.6, Xcode 14.2 — but NO pnpm, NO rustup/rustc/cargo (verified by probing).
**How to avoid:** Wave 0 tasks: `corepack enable && corepack prepare pnpm@latest --activate`; install rustup via the official script; verify `cargo build` on a hello-world before Tauri scaffolding. If latest stable rustc complains about Xcode 14.2 (unlikely — documented floor is Xcode 9.2), pin an older toolchain (e.g., 1.77–1.85).
**Warning signs:** `command not found: pnpm` / `command not found: cargo`.

### Pitfall 7: FontAwesome 7 icon renames
**What goes wrong:** `fa-wave-square` or `fa-closed-captioning` render as missing glyphs after `pnpm add @fortawesome/fontawesome-free` (which installs 7.3.1).
**Why it happens:** FA7 renamed/deprecated icons; the UI-SPEC inventory locks FA6 names.
**How to avoid:** Pin all FA packages to `6.7.2` explicitly; verify one icon renders in the first scaffold task.
**Warning signs:** Empty boxes/tofu where icons should be.

### Pitfall 8: H5 reconnect leaves a blank screen
**What goes wrong:** Phone locks or Wi-Fi blips; WS drops; on reconnect the session timeline is empty.
**Why it happens:** Events are push-only; a fresh connection receives nothing until the next event.
**How to avoid:** Rust keeps the session timeline (idempotent, replayable per ARCHITECTURE.md); on WS `onopen`, the H5 sends `resume` with last-known `seq`; server replays from there. Reconnect with backoff (1s→2s→4s→max 30s) and 正在自动重连 status (UI-SPEC copy).
**Warning signs:** Phone screen goes blank after wake; no reconnect logic in the H5.

## Code Examples

### Example 1: tauri.conf.json — two fixed frameless transparent windows (DSK-01/02)
```jsonc
// apps/desktop/src-tauri/tauri.conf.json (key parts)
{
  "app": {
    "windows": [
      {
        "label": "console",
        "title": "NexTalk",
        "url": "index.html#/console",
        "width": 340, "height": 680,
        "resizable": false, "maximizable": false, "fullscreen": false,
        "decorations": false, "transparent": true, "shadow": false,
        "visible": true, "center": true
      },
      {
        "label": "dual",
        "title": "NexTalk",
        "url": "index.html#/dual",
        "width": 860, "height": 680,
        "resizable": false, "maximizable": false,
        "decorations": false, "transparent": true, "shadow": false,
        "visible": false, "center": true
      }
    ]
  },
  "bundle": { "minimumSystemVersion": "12.0" },
  "build": { "beforeDevCommand": "pnpm dev", "beforeBuildCommand": "pnpm build", "devUrl": "http://localhost:1420", "frontendDist": "../dist" }
}
```
Source: verified via v2.tauri.app reference config (app.windows schema, unique labels, per-window url) + tauri issues #12042/community (transparent+shadow) — [MEDIUM, cross-verified]. Note: `dual` starts `visible: false`; Rust `setup()` shows it when 扩展视图 is clicked.

### Example 2: Rust axum WS server with token pairing (SYNC-01)
```rust
// apps/desktop/src-tauri/src/lan/server.rs (pattern source: axum 0.8 ws docs)
use axum::extract::{Query, State, ws::{WebSocketUpgrade, WebSocket}};
use axum::response::Response;
use serde::Deserialize;

#[derive(Deserialize)]
struct PairingParams { token: String }

pub async fn ws_handler(
    Query(p): Query<PairingParams>,
    State(state): State<Arc<SessionState>>,
    ws: WebSocketUpgrade,
) -> Response {
    if p.token != state.pairing_token() {
        return (StatusCode::UNAUTHORIZED, "invalid pairing token").into_response();
    }
    ws.on_upgrade(|socket| client_loop(socket, state)) // spawn broadcast recv + client send
}
// Router: Router::new().route("/ws", get(ws_handler))
//   .nest_service("/", ServeDir::new(h5_dist))
//   .with_state(state)
```

### Example 3: Vite config — Safari 15.6 target (UI-03)
```typescript
// apps/desktop/vite.config.ts (same shape for apps/teleprompter)
export default defineConfig({
  plugins: [react()],
  build: { target: ['safari15', 'es2022'] },
  server: { port: 1420, strictPort: true },
});
```
Source: verified — Vite 7 default is `baseline-widely-available` (Safari 16.0); explicit `safari15` required [HIGH, vitejs/vite migration guide].

### Example 4: Protocol types (packages/protocol) — shared desktop ↔ H5
```typescript
// packages/protocol/src/index.ts (design: discretion area — semantic model per UI-SPEC)
export type LanguagePref = 'all-zh' | 'all-en' | 'bilingual';
export type Speaker = 'interviewer' | 'user';

export type ServerEvent =
  | { t: 'subtitle'; id: string; speaker: Speaker; seq: number;
      zh?: string; en?: string; final: boolean }
  | { t: 'strategy'; id: string; roundId: string; title: string; bullets: string[] }
  | { t: 'status'; session: 'idle' | 'listening' | 'generating' | 'ended' }
  | { t: 'timeline'; events: ServerEvent[] };           // replay on reconnect

export type ClientMessage =
  | { t: 'control'; language: LanguagePref }            // SYNC-03
  | { t: 'resume'; sinceSeq: number };                  // reconnect replay

// narrowing helper (no zod dependency needed for this small closed union):
export function isServerEvent(x: unknown): x is ServerEvent { /* checks x.t */ }
```

### Example 5: Typewriter hook — deterministic for tests (SYNC-05)
```typescript
// apps/desktop/src/hooks/useTypewriter.ts (shared shape in teleprompter too)
export function useTypewriter(text: string, intervalMs = 40) {
  const [visible, setVisible] = useState(0);
  const reduced = usePrefersReducedMotion();
  useEffect(() => {
    if (reduced) { setVisible(text.length); return; }
    setVisible(0);
    const id = setInterval(() => setVisible(n => (n >= text.length ? (clearInterval(id), n) : n + 1)), intervalMs);
    return () => clearInterval(id);
  }, [text, intervalMs, reduced]);
  return text.slice(0, visible);
}
// Test: vi.useFakeTimers(); advanceTimersByTime(text.length * 40) → full text. Deterministic.
```

### Example 6: Wake lock with hidden-video fallback (SYNC-04)
```typescript
// apps/teleprompter/src/hooks/useWakeLock.ts
const VIDEO_SRC = 'data:video/mp4;base64,...'; // tiny bundled mp4 (or bundled asset)
export function useWakeLock() {
  const engage = useCallback(() => {          // MUST be called from 开始提词 click
    let released = false;
    if ('wakeLock' in navigator) {
      navigator.wakeLock.request('screen')
        .then(lock => { released = true; document.addEventListener('visibilitychange', () => { /* re-request */ }); })
        .catch(() => startVideo());           // NotAllowedError / absent → fallback
    } else { startVideo(); }
    function startVideo() {
      const v = document.createElement('video');
      Object.assign(v, { src: VIDEO_SRC, loop: true, muted: true, playsInline: true });
      v.style.cssText = 'position:fixed;width:1px;height:1px;opacity:0;pointer-events:none;left:-2px';
      v.play(); // requires the user gesture (开始提词) — verified iOS behavior
    }
  }, []);
  return engage;
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Vite 5/6 `build.target: 'modules'` (Safari 14 floor) | Vite 7 `baseline-widely-available` (Safari 16 floor) | Vite 7 (2025) | Explicit `safari15` target is now MANDATORY for this project — silently breaking default |
| FontAwesome 6 | FontAwesome 7 (7.3.1, icon renames) | FA7 release | Pin `@fortawesome/*@6.7.2` — UI-SPEC locks FA6 icon names |
| TypeScript 5.x | TypeScript 7.0.2 (Go-native rewrite) | 2026 | Stay on TS 5.9.3; TS7 ecosystem (tooling/plugins) is new |
| Vite 7 | Vite 8.2.2 | 2026 | Stay on 7.3.6 per STACK.md lock; re-evaluate when upgrading the stack |
| Tailwind v3.4 | Tailwind v4.3.3 (`@property`, `color-mix`, `oklch`) | 2024-2026 | v4 requires Safari 16.4+ — the lock on v3.4 is the correct call for macOS 12.7 |
| React 18 | React 19.2.8 | 2024-2026 | React 19 supports Safari 12+; needs ES2023 polyfill (Promise.withResolvers) on old WebKit |
| Wake Lock API (iOS 16.4+) | Wake Lock still unavailable on plain LAN http | unchanged | Hidden-video fallback remains the standard mitigation; gesture required |
| axum 0.7 (Rust 1.75 MSRV) | axum 0.8.9 | 2024 | 0.8 is current; `ws` feature unchanged in shape |

**Deprecated/outdated:**
- `tauri.conf.json` v1 layout (`tauri.windows`): v2 moved windows under `app.windows` — using v1 structure errors out (verified via migration discussions).
- `tokio-tungstenite` as a direct server dependency: axum's `ws` feature wraps it — only add directly if you need tungstenite types in your public API.
- `typeface-spacegrotesk`: deprecated; use `@fontsource/space-grotesk` (project recommends Fontsource).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Transparent frameless windows (`transparent: true` + `macos-private-api` + `shadow: false`) work on macOS 12.7 Intel with Tauri 2.11 | Common Pitfalls / Code Example 1 | If transparency fails on 12.7, the rounded-corner look degrades to square corners — UI deviation requiring design sign-off; scaffold task should verify early |
| A2 | Rust stable (2026) builds fine with Xcode 14.2 (documented floor Xcode 9.2) | Environment | If newest stable refuses/panics on old SDK, pin an older toolchain (1.77–1.85) — time-boxed fix |
| A3 | Per-window `url: "index.html#/console"` hash routing works identically in dev (devUrl) and prod | Architecture Patterns | If devUrl replaces the hash in dev, both windows would show the same route — mitigate: window identity via `label` (Tauri window-label detection) as the source of truth for routing |
| A4 | `vite build --watch` + axum ServeDir in dev is a better DX than pointing QR at the H5 Vite dev server | Architecture Patterns | If watch-rebuild is janky, fall back to H5 dev server + QR with explicit ws URL param — design the H5 URL parsing to tolerate both |
| A5 | FA6 solid icon names in UI-SPEC all exist in free-solid-svg-icons 6.7.2 | Standard Stack | A renamed icon renders missing — verify all 28 UI-SPEC icon names in the scaffold task |
| A6 | core-js `promise-with-resolvers` polyfill is sufficient for the dependency set | Common Pitfalls | If a dep uses another ES2023+ API, add that proposal polyfill too — check at scaffold time |
| A7 | Playwright-testing the React apps in browsers (not inside WKWebView) gives adequate E2E coverage for Phase 1 | Validation Architecture | Tauri-specific behaviors (windows, IPC) are smoke-tested manually; accepted tradeoff for Phase 1 |
| A8 | pnpm install via corepack works on this machine (corepack 0.34.6 present) | Environment | If corepack is blocked, fall back to `npm install -g pnpm` — either path is one command |
| A9 | Phone H5 testing requires a real phone on the same Wi-Fi (manual) | Environment | No emulator fallback for wake-lock behavior — demo/verify steps must be manual on device |
| A10 | react-router-dom 7.18.2 HashRouter works under Tauri's SPA fallback (serves index.html for all paths) | Standard Stack | Verified pattern in the community; if it misbehaves in the packaged app, fall back to a 20-line hash-route hook |

## Open Questions

1. **Transparent window behavior on macOS 12.7 Intel (A1)**
   - What we know: Tauri 2 supports `transparent: true` on macOS with the private-API feature; `shadow: false` kills the black halo; documented on macOS generally.
   - What's unclear: any 12.7/Intel-specific rendering quirk (old WebKit compositing).
   - Recommendation: make the first desktop scaffold task a "transparency smoke check" before building all components; escalation path = square corners + design sign-off.

2. **Rust toolchain + Xcode 14.2 (A2)**
   - What we know: rustc documents Xcode 9.2 as the floor; Xcode 14.2 is present; Tauri 2.11 MSRV 1.77.2.
   - What's unclear: whether the 2026 stable toolchain (1.8x/1.9x) still accepts the old clang/linker without friction.
   - Recommendation: Wave 0 includes `cargo new` + `cargo build` smoke test before Tauri scaffolding; pin older toolchain if needed.

3. **Dev-mode H5 serving (A4)**
   - What we know: single-origin axum serving is the cleanest prod path.
   - What's unclear: watch-rebuild ergonomics vs H5 Vite dev server with an explicit `ws=` URL param.
   - Recommendation: implement H5 URL parsing to accept optional `ws` param (fallback same-origin); try ServeDir+watch first.

4. **Playwright browser install**
   - What we know: Playwright 1.62.1 needs browser binaries downloaded (first run).
   - What's unclear: whether this machine's network allows the download (likely yes).
   - Recommendation: include `pnpm exec playwright install chromium` in Wave 0; fallback: vitest-only for Phase 1 E2E-critical paths.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| macOS 12.7.6 Intel (this machine) | Everything — the exact compatibility floor | ✓ | 12.7.6 (x86_64) | — (Safari 15.6 WKWebView directly testable in place) |
| Node.js | Vite/Vitest/Playwright | ✓ | v24.15.0 | — |
| npm | Registry + workspace fallback | ✓ | 11.12.1 | — |
| corepack | pnpm install path | ✓ | 0.34.6 | `npm install -g pnpm` |
| pnpm | Workspace package manager (locked by STACK.md) | ✗ | — | install via corepack (A8); last resort npm workspaces |
| Rust toolchain (rustup/rustc/cargo) | Tauri build (BLOCKING for the whole phase) | ✗ | — | rustup official installer; pin older toolchain if Xcode 14.2 friction (A2) |
| Xcode CLT | Rust linking + Tauri build | ✓ | 14.2 (SDK 13.x) | — |
| Playwright browsers | E2E/visual tests | ✗ (not yet downloaded) | — | `pnpm exec playwright install chromium` in Wave 0; vitest fallback |
| Real phone (iOS/Android) on same Wi-Fi | H5 demo + wake-lock manual verification (SYNC-04) | manual | — | none — wake-lock behavior requires a real device |
| Wi-Fi/LAN | H5 over `http://192.168.x.x` | assumed | — | hotspot fallback (documented in PITFALLS.md: AP isolation risk) |
| Internet | npm/crates.io installs, Playwright download | assumed | — | offline install would block Wave 0 |

**Missing dependencies with no fallback:**
- Rust toolchain — MUST be installed in Wave 0 (rustup); the entire Tauri build depends on it. No realistic fallback (Electron is out of scope per locked decisions).

**Missing dependencies with fallback:**
- pnpm → corepack enable (one command) → npm workspaces (structural change, last resort)
- Playwright browsers → vitest-only unit coverage for Phase 1 (E2E depth reduced)
- Phone → manual device is the only way to verify wake-lock fallback; plan a manual verification step

## Validation Architecture

> `workflow.nyquist_validation` is `true` in .planning/config.json — section required.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Vitest 4.1.11 (+ @testing-library/react 16.3.2, jsdom 30.0.1); Playwright 1.62.1 for E2E |
| Config file | `vitest.config.ts` per app + `playwright.config.ts` at root (none exist — Wave 0) |
| Quick run command | `pnpm -r test` (unit; < 30s) |
| Full suite command | `pnpm -r test && pnpm exec playwright test` (unit + E2E) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UI-01 | Design tokens resolve (colors/shadows/spacing used by components) | unit | `pnpm --filter @nextalk/design-tokens test` (token contract snapshot) | ❌ Wave 0 |
| UI-02 | 4 reference screens + 6 missing pages render with all states | e2e/visual | Playwright: per-page render + empty/error states at 340×680 / 860×680 viewports | ❌ Wave 0 |
| UI-03 | Zero CDN: built bundles contain no external http(s) refs | unit (build check) | script: grep dist for `https://` (allowlist empty) | ❌ Wave 0 |
| SYNC-01 | WS upgrade rejects bad token, accepts good; QR URL format | unit (Rust) | `cargo test --workspace` (`lan::tests`) | ❌ Wave 0 |
| SYNC-02 | H5 renders subtitles + strategy from WS events | e2e | Playwright: connect a test WS client to the served H5, assert render | ❌ Wave 0 |
| SYNC-03 | Language toggle re-renders all surfaces (message round-trip) | unit (protocol) + e2e | protocol narrowing tests + Playwright toggle flow | ❌ Wave 0 |
| SYNC-04 | Wake-lock engage path is gesture-bound; fallback engages | manual (device) + unit (hook logic) | unit: hook calls video start on absence/rejection (jsdom); manual: real phone step | ❌ Wave 0 |
| SYNC-05 | Typewriter deterministic cadence + reduced-motion instant | unit | Vitest fake timers: `useTypewriter` advances exactly interval×chars; reduced-motion renders full text | ❌ Wave 0 |
| DSK-01 | Console window renders hub components + navigation | e2e + manual smoke | Playwright browser tests at 340×680; manual: `tauri dev` window open | ❌ Wave 0 |
| DSK-02 | Dual-pane window renders subtitle/AI panes | e2e + manual smoke | Playwright at 860×680; manual: window opens from 扩展视图 | ❌ Wave 0 |
| DSK-04 | Desktop subtitle render + per-bubble toggle | unit + e2e | ChatBubble/LanguageToggle tests + Playwright dual-pane flow | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `pnpm --filter <changed-pkg> test` (fast, targeted)
- **Per wave merge:** `pnpm -r test` (all units)
- **Phase gate:** `pnpm -r test && pnpm exec playwright test` green before `/gsd:verify-work` + manual device verification (H5 + wake lock)

### Wave 0 Gaps
- [ ] `packages/design-tokens/vitest.config.ts` + `tokens.test.ts` — covers UI-01
- [ ] `packages/protocol/index.test.ts` — covers SYNC-03/SYNC-01 message narrowing
- [ ] `apps/desktop/src/hooks/useTypewriter.test.tsx` — covers SYNC-05
- [ ] `apps/teleprompter/src/hooks/useWakeLock.test.tsx` — covers SYNC-04 hook logic
- [ ] `apps/desktop/src-tauri/src/lan/server_test.rs` — covers SYNC-01 token auth
- [ ] `playwright.config.ts` + `e2e/` per-app specs — covers UI-02/DSK-01/DSK-02/SYNC-02
- [ ] Framework install: vitest, @testing-library/react, jsdom, @playwright/test (+ `playwright install chromium`)
- [ ] Rust test harness: `cargo test` needs only std + dev-dependencies (tokio test feature)

## Security Domain

> `workflow.security_enforcement` is `true` in config.json (absent = enabled) — section required. ASVS level 1.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Pairing token (SYNC-01): 128-bit random per session (`rand` OsRng), checked at WS upgrade; no user accounts |
| V3 Session Management | partial | Token is session-scoped and regenerated per app launch; no persistence; reconnect uses same token + seq replay |
| V4 Access Control | yes | Tauri capabilities files per window (`capabilities/default.json`); H5 has no privileged APIs (pure renderer) |
| V5 Input Validation | yes | WS message narrowing in `packages/protocol` (closed discriminated union, `isServerEvent` guard); serde strict structs in Rust (reject unknown fields with `deny_unknown_fields`) |
| V6 Cryptography | no | No secrets at rest, no TLS on LAN (documented tradeoff); token entropy is the security boundary. No crypto primitives hand-rolled. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| WS session hijack (LAN attacker connects with guessed token) | Spoofing | 128-bit random token per session; short-lived; reject upgrade on mismatch (401) — pairing-as-auth |
| Subtitle/interview-content sniffing on plain HTTP LAN | Information disclosure | Plain HTTP is accepted for LAN demo (locked); mitigate: session-scoped token, no PII cached (H5 `Cache-Control: no-store` on the WS/API responses via ServeDir headers), short-lived sessions |
| XSS in H5 or desktop webview | Tampering | React escapes by default; no `dangerouslySetInnerHTML`; no CDN scripts; CSP: Tauri `app.security.csp` with `default-src 'self'` (adjust for the LAN origin in the H5 webview if served cross-origin — same-origin in the recommended design) |
| Token leakage via QR in screen shares | Information disclosure | Phase 1 demo scope: acceptable (stealth is Phase 4); note: QR shown on the console window only, not the dual-pane |
| Malicious H5 content via WS input | Tampering | Strict message narrowing before render (V5); numeric/string field validation in the narrowing helper |
| API keys in binary | — | None exist in Phase 1 (vendor experiments are Phase 2, key handling designed then) — record this in the vendor-framework docs |

## Sources

### Primary (HIGH confidence)
- [Vite 7 migration guide — default build.target `baseline-widely-available`, Safari 16.0 floor, explicit safari15 required](https://v7.vite.dev/guide/migration)
- [Tauri 2 config reference — `app.windows` schema, unique labels, per-window url](https://v2.tauri.app/reference/config/)
- [Tauri 2.11.5 crates.io metadata — MSRV 1.77.2](https://crates.io/api/v1/crates/tauri/2.11.5)
- [axum WebSocket docs (ws feature, WebSocketUpgrade, split pattern)](https://docs.rs/axum/0.8.9/axum/extract/ws/index.html)
- [caniuse — Promise.withResolvers (Safari 17.4+), Screen Wake Lock (Safari 16.4+ secure context)](https://caniuse.com/wf-promise-withresolvers)
- [W3C screen-wake-lock mailing list — transient activation requirement incl. video fallback](https://lists.w3.org/Archives/Public/public-device-apis-log/2022Sep/0071.html)
- [NoSleep.js README — hidden video technique, gesture requirement](https://github.com/richtr/NoSleep.js)
- [Fontsource — @fontsource/space-grotesk self-hosted WOFF2](https://fontsource.org/fonts/space-grotesk)
- [Font Awesome docs — SVG Core tree-shaking for React](https://docs-v6.fontawesome.com/web/dig-deeper/svg-core)
- [rustc platform support — apple-darwin minimum Xcode 9.2](https://doc.rust-lang.org/stable/rustc/platform-support/apple-darwin.html)
- npm registry (npm view): react 19.2.8, vite 7.3.6, tailwindcss 3.4.19, @fontsource/space-grotesk 5.3.0, FA 6.7.2/7.3.1, TS 5.9.3/7.0.2, vitest 4.1.11, @tauri-apps/api 2.11.1, react-router-dom 7.18.2
- crates.io API: tauri 2.11.5, axum 0.8.9, tower-http 0.7.0, tokio 1.53.1, rand 0.10.2, local-ip-address 0.6.13, qrcode 0.14.1

### Secondary (MEDIUM confidence)
- [Tauri issue #12042 — decorations:false inconsistencies across platforms](https://github.com/tauri-apps/tauri/issues/12042)
- [Stack Overflow — transparent frameless window black halo fix (shadow: false)](https://stackoverflow.com/questions/76180922/set-window-border-radius-in-tauri-apps)
- [Tauri discussion #9363 — windows config location in v2](https://github.com/orgs/tauri-apps/discussions/9363)
- [WebviewWindowBuilder multi-window patterns (per-window url relative to dist)](https://github.com/orgs/tauri-apps/discussions/9601)
- [Axum tungstenite/ws implementation detail (axum wraps tokio-tungstenite)](https://deepwiki.com/tokio-rs/axum/6.2-websocket-communication)
- [Axum 0.8 read_buffer_size ws issue — caveat](https://github.com/tokio-rs/axum/issues/3262)
- [NoSleep.js / iOS hidden video fallback case study](https://github.com/idvorkin/igor-timer/pull/20)
- [Screen wake lock for TTS playback — progressive enhancement pattern (2026)](https://bagrounds.org/ai-blog/2026-03-20-screen-wake-lock-for-tts)
- [Xcode/rustc 2026-era compatibility audit (Rust 1.95/1.96 + Xcode 26.x operational examples)](https://github.com/fitchmultz/cueloop/blob/main/docs/guides/stack-audit-2026-04.md)
- [axum-tungstenite crate (when tungstenite types are needed in public API)](https://docs.rs/axum-tungstenite)

### Tertiary (LOW confidence)
- [fontawesome-subset npm tool (not selected — SVG-core chosen)](https://www.npmjs.com/package/fontawesome-subset)
- [Space Grotesk font files repo (fallback if fontsource unavailable)](https://github.com/matthewelsom/font-SpaceGrotesk)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — versions verified via npm registry and crates.io API on 2026-08-27; stack lines locked by STACK.md/UI-SPEC
- Architecture: HIGH for Tauri/monorepo/axum/wake-lock patterns (official docs + cross-verified community); MEDIUM for transparent-window-on-12.7 and dev-mode H5 serving (A1, A4 — early scaffold verification tasks)
- Pitfalls: HIGH — Vite 7 target change, FA7 renames, ES2023 polyfill, wake-lock gesture all verified against primary sources

**Research date:** 2026-08-27
**Valid until:** 2026-09-26 (30 days — Tauri/Vite/Rust are fast-moving; versions verified today may drift)
