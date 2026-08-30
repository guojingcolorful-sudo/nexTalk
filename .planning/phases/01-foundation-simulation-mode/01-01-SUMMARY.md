---
phase: 01-foundation-simulation-mode
plan: 01
subsystem: infra
tags: [pnpm-workspace, vite7, vitest4, tailwind3.4, typescript, design-tokens, websocket-protocol, rustup, playwright]
requires: []
provides:
  - "pnpm workspace (4 packages, single lockfile) with Safari-15-safe build configs"
  - "packages/design-tokens: tokens.css + Tailwind preset + contract snapshot test"
  - "packages/protocol: ServerEvent/ClientMessage closed unions + isServerEvent narrowing"
  - "Test harness: vitest per package, Playwright config + chromium, eslint/prettier flat configs"
affects: [01-02, 01-03, 01-04, 01-05, phase-2]
tech-stack:
  added:
    - "Rust 1.98.0 stable via rustup (Xcode 14.2 compatible — no pin needed)"
    - "pnpm 11.24.0 via corepack (locked packageManager pin)"
    - "vite 7.3.6 (build.target ['safari15','es2022']), @vitejs/plugin-react 5.1.4"
    - "tailwindcss 3.4.19, postcss 8.5.26, autoprefixer 10.5.4"
    - "vitest 4.1.11, @testing-library/react 16.3.2, jsdom 30.0.1"
    - "@playwright/test 1.53.2 + chromium (macOS-12-compatible line)"
    - "react 19.2.8, typescript 5.9.3, @fortawesome/* 6.7.2, @fontsource/space-grotesk 5.3.0, qrcode 1.5.4, react-router-dom 7.18.2, @tauri-apps/api 2.11.1 + cli 2.11.4, core-js 3.50.0, ws 8.21.3, concurrently 10.0.5"
  patterns:
    - "Contract-first: design tokens and WS protocol exist and are tested before any UI/Rust consumes them"
    - "CJS tailwind-preset.js consumed via require() from both apps' tailwind.config.js"
    - "Protocol narrowing guard (isServerEvent) as the security boundary for all inbound WS payloads"
key-files:
  created:
    - "pnpm-workspace.yaml, package.json, pnpm-lock.yaml, eslint.config.js, .prettierrc, playwright.config.ts, .gitignore"
    - "apps/desktop/{package.json,vite.config.ts,tsconfig.json,vitest.config.ts,tailwind.config.js,postcss.config.js}"
    - "apps/teleprompter/{package.json,vite.config.ts,tsconfig.json,vitest.config.ts,tailwind.config.js,postcss.config.js}"
    - "packages/design-tokens/{package.json,vitest.config.ts,src/tokens.css,src/tailwind-preset.js,src/tokens.test.ts}"
    - "packages/protocol/{package.json,vitest.config.ts,src/index.ts,src/index.test.ts}"
  modified: []
key-decisions:
  - "Task 1 gate APPROVED: user approved all 20 npm packages after re-verifying @fortawesome/fontawesome-free 6.7.2 (registry: repo matches FortAwesome/Font-Awesome, scripts={}, publisher fortawesome-admin, dist.integrity present; weekly downloads 2.5M vs audit ~15M — same magnitude, not a risk)"
  - "Rust stable 1.98.0 accepted Xcode 14.2 (smoke crate built) — research Open Question A2 resolved favorably, no 1.85 pin needed"
  - "@vitejs/plugin-react pinned 5.1.4: the plan-pinned 6.1.0 requires vite 8, contradicting the plan's own vite 7.3.6 pin (research: do NOT upgrade to v8)"
  - "@playwright/test pinned 1.53.2: 1.62.1 refuses to install browsers on macOS 12; 1.53.2 is the newest mac12-compatible line"
  - "pnpm 11.24.0 allowBuilds approved for core-js + esbuild postinstall scripts (both pre-audited packages; esbuild binary verification is build-critical)"
  - "Contract tests read tokens.css via node:fs — vitest 4 stubs .css imports (incl. ?raw) by default"
patterns-established:
  - "Exact-version pins in every package.json (no ranges), single pnpm lockfile"
  - "passWithNoTests: true in every vitest config so pre-test-file windows stay green"
  - "Safari 15.6 floor enforced at build-target level (safari15/es2022) + plain-hex tokens"
requirements-completed: [UI-01, UI-03, SYNC-01, SYNC-03]
duration: 26h
completed: 2026-08-29
---

# Phase 1 Plan 1: Workspace + Contract Packages Summary

**pnpm workspace with Safari-15-safe Vite 7 configs, UI-SPEC-exact design tokens (portalGreen #97ce4c etc.) with contract snapshot tests, and a narrowed ServerEvent/ClientMessage WS protocol package — all green under `pnpm -r test`**

## Performance

- **Duration:** ~26h wall clock (two installs ran overnight on a slow connection; active work ~3h)
- **Started:** 2026-08-28T06:29:41Z
- **Completed:** 2026-08-29T08:14:08Z
- **Tasks:** 3 (1 checkpoint-approved + 2 executed; Task 3 TDD = 2 commits)
- **Files modified:** 27

## Accomplishments

- Toolchain blockers cleared: rustup stable 1.98.0 (cargo smoke build passed against Xcode 14.2 — no toolchain pin required) and pnpm 11.24.0 via corepack, locked in `packageManager`
- 4-package pnpm workspace with single lockfile: apps/desktop, apps/teleprompter, packages/design-tokens, packages/protocol; every dependency exact-version pinned
- Both apps build-target `['safari15','es2022']` (UI-03: macOS 12.7 WKWebView floor); desktop dev server 1420 strictPort; teleprompter `base: './'` for LAN static serving
- design-tokens package: full UI-SPEC token set in `tokens.css` (11 colors, 5 hard shadows, 5-size type scale, 8-step spacing scale, radii, dot-matrix) + CJS Tailwind preset wired into BOTH apps' tailwind.config.js (downstream plans' portalGreen/mortyYellow/rickBlue classes resolve)
- protocol package: `ServerEvent` closed union (subtitle/strategy/status/language/timeline) + `ClientMessage` (control/resume) + `isServerEvent` per-field narrowing — the security boundary for WS payloads (threat T-01-02) and the cross-plan contract 01-02/01-04/01-05 all consume
- Wave 0 harness: vitest with `passWithNoTests` everywhere, Playwright config + chromium downloaded, eslint flat config + prettier, .gitignore

## Task Commits

Each task was committed atomically:

1. **Task 1: Approve npm package legitimacy** - checkpoint:human-verify, no code commit (approval recorded in STATE.md decisions)
2. **Task 2: Install toolchain and scaffold the pnpm workspace** - `d1690b7` (chore)
3. **Task 3: Design-token and protocol contract packages (TDD)** - `f9c417b` (test, RED) + `c72cbff` (feat, GREEN)

**Plan metadata:** `docs(01-01): complete` (final commit with SUMMARY/STATE/ROADMAP)

## Files Created/Modified

- `pnpm-workspace.yaml` - workspace packages + pnpm 11 allowBuilds (core-js, esbuild)
- `package.json` - root scripts (dev/build/test/test:e2e), packageManager pnpm@11.24.0
- `pnpm-lock.yaml` - single exact-version lockfile (402 resolved packages)
- `eslint.config.js` - flat config: @eslint/js + typescript-eslint + react-hooks; CJS globals for tailwind/postcss configs
- `.prettierrc` - singleQuote, semi, 100 printWidth
- `playwright.config.ts` - desktop (1420) + teleprompter (8787) projects with preview webServers
- `apps/desktop/{package.json,vite.config.ts,tsconfig.json,vitest.config.ts,tailwind.config.js,postcss.config.js}` - desktop app shell; vite 1420 strictPort + safari15; jsdom vitest; design-tokens preset
- `apps/teleprompter/{package.json,vite.config.ts,tsconfig.json,vitest.config.ts,tailwind.config.js,postcss.config.js}` - H5 app shell; base './' + safari15; ws devDep (mock WS server for e2e)
- `packages/design-tokens/src/tokens.css` - UI-SPEC V1.0 token set under `:root`, plain hex, Safari 15.6-safe
- `packages/design-tokens/src/tailwind-preset.js` - CJS preset: portalGreen/mortyYellow/rickBlue/darkerSpace/spaceDark, cartoon-* shadows, Space Grotesk, dot-matrix
- `packages/design-tokens/src/tokens.test.ts` - 13-test contract snapshot (colors/shadows/type/spacing/radii/dot-matrix) read via node:fs
- `packages/protocol/src/index.ts` - LanguagePref/Speaker/ServerEvent/ClientMessage + isServerEvent narrowing
- `packages/protocol/src/index.test.ts` - 20-test narrowing suite incl. language-mode variant

## Decisions Made

- Task 1 gate: user approved all 20 audit-table packages after re-verifying @fortawesome/fontawesome-free 6.7.2 (npm registry: repo `git+https://github.com/FortAwesome/Font-Awesome.git` matches audit, license CC-BY-4.0 AND OFL-1.1 AND MIT, no postinstall scripts, publisher fortawesome-admin, dist.integrity present; observed 2.5M weekly downloads vs the audit's ~15M — same magnitude, not a risk)
- Rust stable 1.98.0 (2026-08-18) builds fine against Xcode 14.2 — research Open Question A2's pin-1.85 fallback not needed
- pnpm 11.24.0 activated via corepack; `packageManager: "pnpm@11.24.0"` locks the toolchain for the repo
- @vitejs/plugin-react 5.1.4 (vite-7-compatible line) instead of the plan's 6.1.0, which requires vite 8
- @playwright/test 1.53.2 (newest macOS-12-compatible line) instead of 1.62.1, which cannot install browsers on this machine
- Contract tests use node:fs to read tokens.css (vitest 4 stubs .css imports)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] @vitejs/plugin-react version conflict with vite 7**
- **Found during:** Task 2 (workspace scaffold)
- **Issue:** Plan pins vite 7.3.6 (research: v8 rejected) but @vitejs/plugin-react 6.1.0 declares peer `vite ^8.0.0` — pnpm install would fail `pnpm peers check`
- **Fix:** Pinned @vitejs/plugin-react 5.1.4 (peer `^4.2 || ^5 || ^6 || ^7`), the latest plugin line compatible with vite 7
- **Files modified:** apps/desktop/package.json, apps/teleprompter/package.json, pnpm-lock.yaml
- **Verification:** `pnpm peers check` → "No peer dependency issues found"; full install clean
- **Committed in:** d1690b7

**2. [Rule 3 - Blocking] @playwright/test 1.62.1 unsupported on macOS 12**
- **Found during:** Task 2 (playwright install)
- **Issue:** `playwright install chromium` fails with "Playwright does not support chromium on mac12"; research's Environment Availability table anticipated a browser-download fallback
- **Fix:** Pinned @playwright/test 1.53.2 — empirically verified as a macOS-12-supported line (chromium 1179 + ffmpeg_mac12_special downloaded); the download itself needed one retry after an ECONNRESET
- **Files modified:** package.json, pnpm-lock.yaml
- **Verification:** `pnpm exec playwright install chromium` exits 0; chromium-1179 + headless shell present in the browser cache
- **Committed in:** d1690b7

**3. [Rule 1 - Bug] Contract test parser produced undefined token values**
- **Found during:** Task 3 GREEN (design-tokens suite)
- **Issue:** Two defects: (a) vitest 4 stubs `.css` imports — even `?raw` returns an empty string, so the test read nothing; (b) after switching to node:fs, declarations immediately following a `/* comment */` were glued to the comment by `split(';')` and missed by the regex — exactly the 6 tokens after section comments failed
- **Fix:** Read tokens.css via `readFileSync(new URL(...))`; strip `/* ... */` comments before splitting declarations
- **Files modified:** packages/design-tokens/src/tokens.test.ts
- **Verification:** 13/13 tokens tests pass; wave gate green
- **Committed in:** c72cbff

**4. [Rule 2 - Missing Critical] pnpm 11 build-script gate would break the toolchain**
- **Found during:** Task 2 (pnpm install)
- **Issue:** pnpm 11.24 blocks postinstall scripts by default (`ERR_PNPM_IGNORED_BUILDS` for core-js and esbuild); esbuild's postinstall is build-critical (binary verification)
- **Fix:** Added `allowBuilds: { core-js: true, esbuild: true }` to pnpm-workspace.yaml (both are pre-audited, user-approved packages; core-js's script is a funding banner, esbuild's is its binary check)
- **Files modified:** pnpm-workspace.yaml
- **Verification:** postinstalls ran cleanly; `pnpm -r test` and vite pipeline green
- **Committed in:** d1690b7

**5. [Rule 2 - Missing Critical] ESLint flagged CJS config files as undefined globals**
- **Found during:** Task 2 (eslint smoke check)
- **Issue:** `module`/`require` in tailwind.config.js and postcss.config.js flagged no-undef under js.configs.recommended
- **Fix:** Added a flat-config override granting `module`/`require` globals to those two CJS file patterns
- **Files modified:** eslint.config.js
- **Verification:** `pnpm exec eslint .` exits 0
- **Committed in:** d1690b7

---

**Total deviations:** 5 auto-fixed (3 blocking, 2 missing-critical)
**Impact on plan:** All fixes keep the plan's locked decisions intact (vite 7, Safari 15.6, FA 6.7.2, tailwind 3.4.19). Version pins 5.1.4/1.53.2 are documented for downstream plans. No scope creep.

## Issues Encountered

- **Rustup download stalled twice on the slow connection** — moved to background, completed over ~2h; no code impact
- **Playwright chromium download hit an ECONNRESET at 40%** — retry resumed and completed (exit 0)
- **verify gate literal** — the plan's `v.packageManager !== 'pnpm'` check is incompatible with its own `"pnpm@<version>"` instruction; verified `/^pnpm@/` instead (intent: packageManager is a pnpm pin)
- **Entry-CSS token import** — Task 3 says "confirm each app's entry CSS imports tokens.css", but neither app has an entry CSS yet (src/ ships in 01-03/01-04); those plans must `import '@nextalk/design-tokens'` in their entry CSS — flagged here so it is not forgotten

## User Setup Required

None — no external service configuration required. Toolchain (Rust, pnpm) installed on this machine; package approval gate (Task 1) already passed.

## Next Phase Readiness

- **01-02 (walking skeleton)** can start immediately: protocol package is the WS contract for the Rust axum server (`isServerEvent` shapes match serde `deny_unknown_fields` structs); teleprompter devDep `ws` is present for the mock WS server; playwright.config.ts webServers expect desktop preview 1420 + teleprompter preview 8787
- **01-03/01-04 (UI)** will consume `@nextalk/design-tokens` (already wired into both tailwind configs) — token classes portalGreen/mortyYellow/rickBlue resolve; entry CSS must `import '@nextalk/design-tokens'` and `import 'core-js/proposals/promise-with-resolvers'` (Pitfall 5)
- **Blockers:** none. Watch item: Playwright line is capped at 1.53.2 while this machine stays on macOS 12 (upgrading the OS unblocks newer Playwright)

---
*Phase: 01-foundation-simulation-mode*
*Completed: 2026-08-29*

## Self-Check: PASSED

Verified 2026-08-29: all 11 key files exist (tokens.css, tailwind-preset.js, tokens.test.ts, protocol index.ts/index.test.ts, both vite configs, playwright.config.ts, eslint.config.js, pnpm-workspace.yaml, SUMMARY); all 4 commits present in git history (d1690b7, f9c417b, c72cbff, 0f75e39).
