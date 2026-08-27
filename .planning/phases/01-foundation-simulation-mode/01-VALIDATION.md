---
phase: 1
slug: foundation-simulation-mode
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-27
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Vitest 4.1.11 (+ @testing-library/react 16.3.2, jsdom 30.0.1); Playwright 1.62.1 for E2E |
| **Config file** | `vitest.config.ts` per app + `playwright.config.ts` at root (none exist — Wave 0) |
| **Quick run command** | `pnpm -r test` (unit; < 30s) |
| **Full suite command** | `pnpm -r test && pnpm exec playwright test` (unit + E2E) |
| **Estimated runtime** | ~60-120 seconds |

---

## Sampling Rate

- **After every task commit:** `pnpm --filter <changed-pkg> test` (fast, targeted)
- **After every plan wave:** `pnpm -r test` (all units)
- **Before `/gsd:verify-work`:** Full suite must be green (`pnpm -r test && pnpm exec playwright test`) + manual device verification (H5 + wake lock)
- **Max feedback latency:** < 30 seconds (unit) / ~2 minutes (full)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (planner fills) | 01 | 0 | UI-01 | — | N/A | unit | `pnpm --filter @nextalk/design-tokens test` | ❌ W0 | ⬜ pending |
| | 01 | 0 | UI-02 | — | N/A | e2e | Playwright per-page render at 340×680 / 860×680 | ❌ W0 | ⬜ pending |
| | 01 | 0 | UI-03 | — | N/A | unit (build) | grep dist for external http(s) refs | ❌ W0 | ⬜ pending |
| | 01 | 0 | SYNC-01 | — | WS token auth | unit (Rust) | `cargo test --workspace` lan::tests | ❌ W0 | ⬜ pending |
| | 01 | 0 | SYNC-02 | — | N/A | e2e | test WS client → served H5 render assert | ❌ W0 | ⬜ pending |
| | 01 | 0 | SYNC-03 | — | N/A | unit + e2e | protocol narrowing + toggle flow | ❌ W0 | ⬜ pending |
| | 01 | 0 | SYNC-04 | — | N/A | unit + manual | hook calls video fallback; real phone manual step | ❌ W0 | ⬜ pending |
| | 01 | 0 | SYNC-05 | — | N/A | unit | fake timers typewriter cadence + reduced-motion | ❌ W0 | ⬜ pending |
| | 01 | 0 | DSK-01 | — | N/A | e2e + manual | Playwright 340×680 + `tauri dev` smoke | ❌ W0 | ⬜ pending |
| | 01 | 0 | DSK-02 | — | N/A | e2e + manual | Playwright 860×680 + window open from 扩展视图 | ❌ W0 | ⬜ pending |
| | 01 | 0 | DSK-04 | — | N/A | unit + e2e | ChatBubble/LanguageToggle + dual-pane flow | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `packages/design-tokens/vitest.config.ts` + `tokens.test.ts` — covers UI-01
- [ ] `packages/protocol/index.test.ts` — covers SYNC-03/SYNC-01 message narrowing
- [ ] `apps/desktop/src/hooks/useTypewriter.test.tsx` — covers SYNC-05
- [ ] `apps/teleprompter/src/hooks/useWakeLock.test.tsx` — covers SYNC-04 hook logic
- [ ] `apps/desktop/src-tauri/src/lan/server_test.rs` — covers SYNC-01 token auth
- [ ] `playwright.config.ts` + `e2e/` per-app specs — covers UI-02/DSK-01/DSK-02/SYNC-02
- [ ] Framework install: vitest, @testing-library/react, jsdom, @playwright/test (+ `playwright install chromium`)
- [ ] Rust test harness: `cargo test` needs only std + dev-dependencies (tokio test feature)
- [ ] Toolchain install: rustup + pnpm (not installed on dev machine) + `cargo build` smoke test

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Wake lock 常亮（真实手机） | SYNC-04 | 需要真实 iOS/Android 设备与用户手势；jsdom 只能测 hook 逻辑 | 扫码打开 H5 → 点击「开始提词」→ 屏幕保持常亮 ≥ 2 分钟；锁屏/切后台验证恢复 |
| 透明窗口渲染（macOS 12.7 Intel） | UI-01 | 透明无边框窗口在老系统上的渲染行为无法自动化断言 | `tauri dev` 打开控制台与双栏窗口，目测四角圆角无黑晕（transparent + macos-private-api + shadow:false） |
| 二维码真机配对 | SYNC-01 | 局域网环境与真实手机扫描无法在 CI 复现 | 桌面端展示二维码 → 手机相机扫码 → 浏览器打开并建立 WS 连接 → 断网重连恢复会话 |

---

*Phase: 01-foundation-simulation-mode*
*Validation strategy created: 2026-08-27 (from 01-RESEARCH.md Validation Architecture)*
