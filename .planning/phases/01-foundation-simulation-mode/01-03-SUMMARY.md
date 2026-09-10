---
phase: 01-foundation-simulation-mode
plan: 03
subsystem: ui
tags: [react, vite, tailwind, tauri, vitest, playwright, neobrutalism, tdd, websocket-events]

# Dependency graph
requires:
  - phase: 01-01
    provides: design tokens (@nextalk/design-tokens preset), locked protocol types (@nextalk/protocol), Space Grotesk bundling
  - phase: 01-02
    provides: Tauri two-window shell (#/console, #/dual), Rust event bus + sim script, capabilities, HeaderBar/NeobrutalismButton primitives
provides:
  - Console hub (340x680) with status capsule, dual-pane controls, QR pairing, stealth card, local asset nav
  - Dual-pane extended view (860x680) with live subtitle stream + AI timeline
  - useTauriEvents hook (event narrowing + session status + phone count)
  - ChatBubble / LanguageToggle per-bubble language switching
  - All six previously-missing pages (setup wizard, voice enrollment, glossary, resume import, recordings, review)
  - Shared component library (WizardShell, FormField, FileDropZone, GlossaryRow, RecordingAssetCard, ReviewReportSection, AiTimeline, PanelHeader, TypewriterDots, MicStatusPill, EmptyState, ErrorBanner, ConfirmModal, Skeleton, Toast, KnowledgeRow, StealthCard, QrCodeCard)
  - Mock data module with 模拟数据 labels
  - 19-spec desktop Playwright suite covering every route
affects: [01-04, 01-05]

# Tech tracking
tech-stack:
  added:
    - '@nextalk/protocol (workspace link into apps/desktop)'
  patterns:
    - 'Per-bubble local language state seeded from speaker defaults (interviewer=bilingual, user=all-zh)'
    - 'isServerEvent narrowing at the hook boundary; pages never see raw payloads'
    - 'Locked Chinese copy rendered verbatim from the UI-SPEC copy table; 模拟数据 / 模拟模式 badges on every non-real surface'
    - 'Layout-matched Skeleton placeholders instead of spinners for every async state'
    - 'WizardShell composition for multi-step flows (steps rail + footer action)'
    - 'Playwright mocks the Tauri IPC bridge (__tauriEmit / plugin:event|listen) instead of the app code'

key-files:
  created:
    - apps/desktop/src/hooks/useTauriEvents.ts
    - apps/desktop/src/hooks/useTauriEvents.test.ts
    - apps/desktop/src/components/ChatBubble.tsx
    - apps/desktop/src/components/ChatBubble.test.tsx
    - apps/desktop/src/components/LanguageToggle.tsx
    - apps/desktop/src/components/LanguageToggle.test.tsx
    - apps/desktop/src/components/AiTimeline.tsx
    - apps/desktop/src/components/WizardShell.tsx
    - apps/desktop/src/components/FormField.tsx
    - apps/desktop/src/components/FileDropZone.tsx
    - apps/desktop/src/components/GlossaryRow.tsx
    - apps/desktop/src/components/RecordingAssetCard.tsx
    - apps/desktop/src/components/ReviewReportSection.tsx
    - apps/desktop/src/components/PanelHeader.tsx
    - apps/desktop/src/components/TypewriterDots.tsx
    - apps/desktop/src/components/MicStatusPill.tsx
    - apps/desktop/src/pages/SetupWizardPage.tsx
    - apps/desktop/src/pages/VoiceEnrollmentPage.tsx
    - apps/desktop/src/pages/GlossaryPage.tsx
    - apps/desktop/src/pages/ResumeImportPage.tsx
    - apps/desktop/src/pages/RecordingsPage.tsx
    - apps/desktop/src/pages/ReviewPage.tsx
    - apps/desktop/src/data/mock-data.ts
    - e2e/desktop.spec.ts
  modified:
    - apps/desktop/src/App.tsx
    - apps/desktop/src/pages/ConsolePage.tsx
    - apps/desktop/src/pages/DualPanePage.tsx
    - apps/desktop/src/components/HeaderBar.tsx
    - apps/desktop/src/components/QrCodeCard.tsx
    - apps/desktop/package.json
    - playwright.config.ts
  removed:
    - apps/desktop/src/components/PageStub.tsx

key-decisions:
  - 'Per-bubble LanguagePref state seeded from speaker defaults; toggles are independent per bubble (SYNC-03)'
  - 'Locked protocol has no draft event variant, so the UI-SPEC green draft timeline node is not implemented; AiTimeline ships context + strategy nodes only'
  - 'Testing Library auto-cleanup is unavailable (vitest globals off) — every React spec calls afterEach(cleanup) explicitly'
  - 'Voice enrollment probes the real microphone via navigator.mediaDevices.getUserMedia and releases tracks on unmount; capture itself is Phase 2'
  - 'PageStub deleted rather than kept as dead code — all six routes render real pages'
  - 'FileDropZone owns its empty state so 简历导入 has exactly one 导入简历 CTA while the whole panel stays a drop target'
  - 'ReviewPage exposes 生成报告/重新生成 so both the empty state and the populated report are reachable'
  - 'e2e denies/grants the microphone through an injected navigator.mediaDevices instead of relying on headless-Chromium permission behavior'

patterns-established:
  - 'Page shell: dot-matrix-root + rounded-3xl border-4 border-black + shadow-cartoon-* + HeaderBar, then a scrollable main'
  - 'Motion contract applied uniformly: hover:translate-y-1 hover:shadow-none active:scale-[0.98], duration-150, motion-reduce:transition-none'
  - 'aria-live="polite" on both subtitle and strategy streams; role=status on live indicators; aria-pressed + role=group on language toggles'
  - 'Destructive actions always route through ConfirmModal (focus trap + Esc)'

requirements-completed: [UI-01, UI-02, DSK-01, DSK-02, DSK-04]

# Metrics
duration: ~1h 4m
completed: 2026-09-10
tasks: 4
files: 40
commits: 5
---

# Phase 1 Plan 03: Complete Desktop Surface Summary

**One-liner:** Console hub, dual-pane live view and all six missing pages built on the locked Chinese copy table — chat bubbles with independent per-bubble 中/EN/EN+中 toggles, a strategy timeline fed by the Rust event bus, layout-matched skeletons everywhere, and 19 Playwright specs walking every route.

## Performance

- **Duration:** ~1h 4m (commits 11:29:43 → 12:33:13 +08:00)
- **Tasks:** 4/4 complete
- **Files changed:** 40 (+3493 / -45)
- **Commits:** 5 (1 RED + 1 GREEN for the TDD pair, plus 3 task commits)

## Accomplishments

- **Task 1 — Console hub + shared components + event hook:** 340×680 hub with status capsule, 打开双屏 control that calls `WebviewWindow.getByLabel('dual').show()`, QR pairing card, stealth card, and navigation into all six local assets. `useTauriEvents` subscribes to the Tauri event bus, narrows payloads with `isServerEvent` (hostile payloads are dropped), flattens `timeline` events into the stream, and derives session status + phone count. New e2e helpers emit events through the real IPC bridge.
- **Task 2 — Dual-pane live view (TDD):** RED commit `e7c07c9` added failing `LanguageToggle` (3 tests) and `ChatBubble` (5 tests) specs; GREEN commit `86ae54a` made them pass. Bubbles render bilingual by default for the interviewer and Chinese-only for the user, each with an independent toggle; subtitles auto-scroll; the AI pane renders interviewer context cards and strategy cards with the yellow cartoon shadow.
- **Task 3 — Wizard, enrollment, glossary:** 4-step setup wizard with a simulated detection pass feeding layout-matched skeletons, a 3-step voice enrollment flow with a live countdown, mic permission error banner and an explicitly-labelled Phase-2 sample placeholder, and a glossary page with add/delete through ConfirmModal plus empty/duplicate validation.
- **Task 4 — Resume, recordings, review:** Drag-and-drop resume import with parsing skeletrons and a parsed-highlights view, recording asset cards with disabled export actions, and a review report with Action Items, key concerns and per-question replay. `PageStub` removed; all six routes render real pages.
- **Verification:** 11/11 vitest, 19/19 desktop e2e, 21/21 full e2e across both Playwright projects, `pnpm --filter @nextalk/desktop build` green (index JS 406.04 kB → 128.70 kB gz, CSS 25.25 kB → 5.37 kB gz, inside the 300 kB / 50 kB budgets), `eslint apps/desktop/src` clean, zero-CDN gate PASS.

## Task Commits

| # | Task | Phase | Commit | Type |
|---|------|-------|--------|------|
| 1 | Console hub + shared components + useTauriEvents + e2e | 01-03 | `e380db4` | feat |
| 2a | Failing specs for LanguageToggle + ChatBubble | 01-03 | `e7c07c9` | test (RED) |
| 2b | LanguageToggle + ChatBubble implementation | 01-03 | `86ae54a` | feat (GREEN) |
| 3 | Setup wizard / voice enrollment / glossary pages | 01-03 | `6ce98c5` | feat |
| 4 | Resume import / recordings / review pages + e2e | 01-03 | `9325d3f` | feat |

## Files Created / Modified

**Created (24):** `useTauriEvents.ts` + its spec, `ChatBubble.tsx` + spec, `LanguageToggle.tsx` + spec, `AiTimeline.tsx`, `WizardShell.tsx`, `FormField.tsx`, `FileDropZone.tsx`, `GlossaryRow.tsx`, `RecordingAssetCard.tsx`, `ReviewReportSection.tsx`, `PanelHeader.tsx`, `TypewriterDots.tsx`, `MicStatusPill.tsx`, the six pages under `src/pages/`, `src/data/mock-data.ts`, `e2e/desktop.spec.ts`.

**Modified (7):** `App.tsx` (6 mock routes → real pages), `ConsolePage.tsx`, `DualPanePage.tsx`, `HeaderBar.tsx` (onBack/actions slots), `QrCodeCard.tsx` (retry extraction), `apps/desktop/package.json`, `playwright.config.ts`.

**Removed (1):** `apps/desktop/src/components/PageStub.tsx`.

## Decisions Made

1. **Per-bubble language state, not per-session.** Each `ChatBubble` owns its own `LanguagePref`, seeded from the speaker default (interviewer `bilingual`, user `all-zh`). This satisfies SYNC-03 (independent toggling) without a global store.
2. **The draft timeline node is not built.** The locked `ServerEvent` union in `@nextalk/protocol` has no `draft` variant, so the UI-SPEC green draft node has no producer in Phase 1. `AiTimeline` implements context (interviewer) + strategy nodes, documented in the component JSDoc.
3. **Explicit `afterEach(cleanup)` in React specs.** Vitest globals are off, so Testing Library never self-registers cleanup; without it, earlier renders leaked into later queries.
4. **Real microphone probe for enrollment.** `navigator.mediaDevices.getUserMedia` is used as the Phase-1 permission probe, with all tracks released on stop/unmount, and the resulting error surfaces as the locked 麦克风不可用 copy.
5. **`PageStub` deleted** rather than left unreferenced — the plan requires no stub routes to remain.
6. **`FileDropZone` embeds its empty state** so 简历导入 has one CTA and one drop target, avoiding a duplicate 导入简历 button.
7. **Review report starts empty with a 生成报告 affordance** so the empty-state copy is not dead.
8. **e2e drives permissions deterministically** by redefining `navigator.mediaDevices` via `addInitScript` for both the deny and grant paths.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Playwright `teleprompter` project collected the desktop specs** (Task 1)
- **Issue:** `e2e/desktop.spec.ts` ran twice — once per project — with the wrong `baseURL`.
- **Fix:** added `testIgnore` to both projects in `playwright.config.ts`.
- **Commit:** `e380db4`

**2. [Rule 1 - Bug] `react-hooks/set-state-in-effect` in `QrCodeCard`** (Task 1)
- **Issue:** state was set synchronously inside an effect, failing the lint gate.
- **Fix:** extracted the generation into a `retry()` callback invoked from the effect.
- **Commit:** `e380db4`

**3. [Rule 3 - Blocking] Testing Library auto-cleanup missing** (Task 2)
- **Issue:** `Found multiple elements with the role button and name EN+中` — prior renders leaked because vitest globals are off.
- **Fix:** explicit `afterEach(cleanup)` in both new specs, with a comment explaining why.
- **Commit:** `86ae54a`

**4. [Rule 1 - Bug] `ChatBubble` rendered nothing for `all-en` with no English text** (Task 2)
- **Issue:** the fallback branch only considered the secondary line, so a user bubble with `en` absent returned `null`.
- **Fix:** fall back to the other language before giving up; covered by the `falls back to the Chinese line` test.
- **Commit:** `86ae54a`

**5. [Rule 3 - Blocking] e2e independence assertion matched the AI pane** (Task 2)
- **Issue:** `expect(page.getByText(QUESTION_EN)).toHaveCount(0)` timed out because the interviewer context card legitimately keeps the English original.
- **Fix:** scoped the assertion to the `实时字幕` region.
- **Commit:** `86ae54a`

**6. [Rule 2 - Missing critical functionality] `useTauriEvents` spec not in the plan's file list** (Task 1)
- **Issue:** Task 1's `<verify>` referenced behaviour that no test covered, making the gate vacuous.
- **Fix:** authored `useTauriEvents.test.ts` (11 cases) covering narrowing, hostile payloads, timeline flattening, status derivation and cleanup.
- **Commit:** `e380db4`

**7. [Rule 2 - Missing critical functionality] `HeaderBar` needed back/action slots** (Task 1)
- **Issue:** six pages require a back affordance and the dual pane needs a status pill; duplicating chrome per page would have violated DRY.
- **Fix:** added `onBack` / `backLabel` / `actions` props.
- **Commit:** `e380db4`

### Planned-file deviations

- `PageStub.tsx` was **deleted** in Task 4 (intentional; no longer referenced).
- `apps/desktop/src/data/mock-data.ts` was **extended twice** beyond its Task-1 shape as later tasks needed typed fixtures.

## Known Gaps

- **AI timeline draft node (UI-SPEC green node):** unimplemented — the locked `ServerEvent` union has no `draft` variant. Requires a protocol decision in a later phase; the strategy node covers the current data.
- **Voice sample playback:** the 试听 tile is an explicitly labelled placeholder (模拟数据 + 真实音色克隆与播放将在后续版本接入，当前样本不会被上传); real cloning is Phase 2+.
- **Recording export buttons:** rendered disabled per the UI-SPEC disabled contract; export lands in Phase 6.
- No hardcoded empty-value stubs that flow to UI rendering; the only placeholders are the labelled ones above.

## Issues Encountered

- **`tsc` is not usable as a gate yet:** `@types/react` / `@types/react-dom` are missing from the workspace, so typecheck errors are pre-existing and unrelated. Recorded in `deferred-items.md`.
- **Pre-existing lint errors outside `apps/desktop`:** out of scope for this plan (scope boundary) and recorded in `deferred-items.md`.
- **Port 8787 is occupied** by `tools/jd-inbox-server.mjs` on this machine, so e2e previews run on 8791 (`reuseExistingServer: true`).
- **Human check outstanding:** Task 4's `<human-check>` (`pnpm --filter @nextalk/desktop tauri dev` manual walkthrough) was not performed — it requires a GUI session. The automated suite walks all six pages through the real router and IPC bridge instead; the manual pass remains for the user.

## User Setup Required

None — no external service configuration or credentials were required for this plan.

## Next Phase Readiness

- Every desktop route renders real content with locked copy; the H5 teleprompter (01-04) can reuse `ChatBubble`, `LanguageToggle` and the same copy table.
- `useTauriEvents` is the single seam between the Rust event bus and the UI — 01-04 can push subtitles/strategies through the same channel for the phone surface.
- The desktop entry (`main.tsx`) was verified to keep `core-js/proposals/promise-with-resolvers` first and `@nextalk/design-tokens` in the import list, resolving the open STATE.md blocker about entry CSS imports.
- Bundle headroom: 128.70 kB gz JS / 5.37 kB gz CSS against 300 kB / 50 kB budgets — room for the teleprompter surface.

## Self-Check: PASSED

- `apps/desktop/src/hooks/useTauriEvents.ts`, `apps/desktop/src/components/ChatBubble.tsx`, `apps/desktop/src/components/LanguageToggle.tsx`, `apps/desktop/src/components/AiTimeline.tsx`: FOUND
- `apps/desktop/src/pages/{SetupWizardPage,VoiceEnrollmentPage,GlossaryPage,ResumeImportPage,RecordingsPage,ReviewPage}.tsx`: FOUND
- `apps/desktop/src/data/mock-data.ts`, `e2e/desktop.spec.ts`: FOUND
- `apps/desktop/src/components/PageStub.tsx`: correctly ABSENT (deleted)
- Commits `e380db4`, `e7c07c9`, `86ae54a`, `6ce98c5`, `9325d3f`: FOUND
