---
phase: 02-real-cloud-pipeline-audio-core
plan: 01
subsystem: latency-instrumentation
tags: [rust, tauri, latency, waterfall, budget-gate, react, vitest, playwright, github-actions, ci]

# Dependency graph
requires:
  - phase: 01-foundation-simulation-mode
    provides: crate::sim::source::TimeSource (the crate's one injectable clock trait) and RealClock; the SimSource scheduler that proves the same clock-injection pattern; the desktop shell (/console + 8 routes), the vitest + testing-library harness with globals off, and the playwright config whose webServer entries run vite preview
provides:
  - "pipeline::budget — Stage (five streaming-TTFB boundaries), LatencyMark, Waterfall (from_marks / from_marks_with_durations, stage_alerts), BudgetVerdict, BudgetBreach, assert_within_budget, WaterfallRecorder, WaterfallAggregator with bounded cold/warm rings, WaterfallReport with nearest-rank p50/p95, E2E_BUDGET_MS = 2000"
  - "tests/latency_rig.rs — the integration lane that hard-fails on any budget breach, plus the #[ignore]d live variant that refuses to pass without credentials or without the real stages"
  - "useLatencyWaterfall + LatencyWaterfall + DiagnosticsPage — the developer-facing waterfall panel at /diagnostics, with the payload-narrowing gate for the latency event"
  - ".github/workflows/ci.yml — four parallel lanes (unit-web, e2e, rust, latency-rig) where the rig is a blocking gate; root scripts test:full / test:rig"
affects: [02-02, 02-03, 02-04, 02-05, phase-3, phase-6]

# Tech tracking
tech-stack:
  added: []          # zero new dependencies, front or back (plan constraint honoured)
  patterns:
    - "Streaming-TTFB instrumentation: five boundary instants per segment, each stage marks its own first byte"
    - "Overlap proved arithmetically: serial_sum_ms (sum of per-stage service durations) vs e2e_ms (stopwatch), overlap_ms = the difference"
    - "Cold and warm are separate classes end to end — separate verdicts, separate bounded rings, a cold segment can never be averaged into the warm claim"
    - "Bounded ring aggregation (MAX_TRACKED_SEGMENTS = 512) with nearest-rank percentiles, O(1)-shaped report"
    - "Typed payload narrowing on the Tauri event bus (extends the 01 isServerEvent habit to timing data)"
    - "Degraded mode is labelled, not faked: no IPC bridge → preview fixture + 预览数据 badge; live with no data → 暂无测量数据"

key-files:
  created:
    - apps/desktop/src-tauri/src/pipeline/mod.rs
    - apps/desktop/src-tauri/src/pipeline/budget.rs
    - apps/desktop/src-tauri/src/pipeline/budget_test.rs
    - apps/desktop/src-tauri/tests/latency_rig.rs
    - apps/desktop/src/hooks/useLatencyWaterfall.ts
    - apps/desktop/src/components/LatencyWaterfall.tsx
    - apps/desktop/src/components/LatencyWaterfall.test.tsx
    - apps/desktop/src/pages/DiagnosticsPage.tsx
    - .github/workflows/ci.yml
  modified:
    - apps/desktop/src-tauri/src/lib.rs
    - apps/desktop/src/App.tsx
    - package.json

key-decisions:
  - "The five boundaries are streaming first-byte instants, not whole-request latencies — vendor numbers enter only as per-stage service durations"
  - "from_marks derives durations as adjacent boundary gaps (serial reading); from_marks_with_durations takes the stages' own durations so the 2223ms naive sum can be compared against a ≤2000ms stopwatch"
  - "OverBudget is an Err with stage attribution, never a warning; no 'warn only' switch exists (the gate semantics of AUDI-04)"
  - "Clock injection reuses crate::sim::source::TimeSource — no second clock trait; cold comes from the session-start path, never from an elapsed-time heuristic"
  - "Requirement numbering: ROADMAP AUDI-04 (延迟测量装置) is REQUIREMENTS.md AUDI-06 (marked complete); REQUIREMENTS.md AUDI-04 is the cascade itself and stays open for 02-02/02-03"
  - "The panel never shows fake measurements: the preview fixture is reachable only when the IPC bridge is absent, and it is labelled on screen"

patterns-established:
  - "Interface contract for 02-02: each stage calls WaterfallRecorder::mark exactly once per segment at its own first byte; cold is passed in by the session-start path"
  - "Deviation-free fmt discipline: `cargo fmt` is run, but unrelated formatting churn is reverted to keep task commits scoped (see deferred-items.md)"
  - "CI lanes carry the 'no Cargo workspace' correction: every cargo command uses --manifest-path"

requirements-completed: [AUDI-04]  # ROADMAP success-criteria numbering (= 延迟测量装置). In REQUIREMENTS.md the same capability is AUDI-06 (now checked); REQUIREMENTS.md AUDI-04 (the cascade pipeline itself) remains open for 02-02/02-03.

# Metrics
duration: ~30min (single session, 2 RED + 2 GREEN + 1 chore commit)
completed: 2026-10-03
---

# Phase 2 Plan 01: Latency Measurement Rig Summary

**A five-boundary streaming latency waterfall that hard-fails any segment over 2000ms and names the blamed stage — proving via `overlap_ms` that the 2223ms naive vendor sum fits inside a ≤2000ms stopwatch, with a `/diagnostics` panel and four CI lanes putting the budget assertion in the gate**

## Performance

- **Duration:** ~30 min (one session; 5 commits)
- **Started:** 2026-10-03T21:53Z (phase 2 execution begins commit)
- **Completed:** 2026-10-03T14:19Z (metadata commit follows this file)
- **Tasks:** 3 of 3 complete (Task 1 T1.1+T1.2, Task 2 T1.3, Task 3 T1.4)
- **Files modified:** 13 (9 created, 3 modified in-repo, plus deferred-items.md)

## Accomplishments

- **The gate exists and it bites.** `assert_within_budget` returns `Err(BudgetBreach)` — not a warning — and the breach Display names the segment, the e2e number, the overage and the blamed stage (中文锁定). A 2001ms segment is caught and attributed; no "warn only" switch was built.
- **Research correction 2 is now an assertion, not a belief.** 讯飞 0.7s + 翻译 0.223s + 火山 1.3s = 2223ms of stage work passes the ≤2000ms stopwatch because the stages genuinely overlap — `serial_sum_ms = 2223`, `e2e_ms ≤ 2000`, `overlap_ms ≥ 223`, verdict `WithinBudget`. The rig prints the overlap so a human can see it too.
- **Cold and warm can never be laundered into one number.** Separate verdicts, separate bounded rings, separate p50/p95. The scripted run reports 冷 1 片段 (1180ms) and 热 4 片段 (p50 1225ms / p95 1300ms) with no cross-class averaging anywhere.
- **The aggregator is bounded.** 600 pushed segments report exactly `MAX_TRACKED_SEGMENTS = 512` and drop the oldest (T-02-03), with nearest-rank p50/p95 asserted on both e2e and per-stage.
- **The panel is real and honest.** `/diagnostics` renders the five stage bars (width = share of the stopwatch), the e2e headline, p50/p95 per class, the 冷启动/热路径 segmented switch carrying each class's own number, and the red attribution badge on breach. Where there is no bridge it says 预览数据; where there is no data it says 暂无测量数据.
- **CI from this wave on treats the budget as a gate.** Four parallel lanes on `macos-13`; the `latency-rig` lane is deterministic and zero-network (no vendor key) and hard-fails on breach.

## Task Commits

Each task was committed atomically (TDD tasks carry a RED and a GREEN commit):

1. **Task 1 RED: failing latency rig and budget gate tests** - `abc129e` (test) — confirmed failing with `error[E0583]: file not found for module 'budget'`
2. **Task 1 GREEN: implement latency budget gate (AUDI-04)** - `ceed329` (feat)
3. **Task 2 RED: failing latency diagnostics panel tests** - `b2c3756` (test) — confirmed failing with `Failed to resolve import "./LatencyWaterfall"`
4. **Task 2 GREEN: add the latency waterfall diagnostics panel** - `1a6bc55` (feat)
5. **Task 3: wire the CI lanes and the full-test scripts** - `fadd9ef` (chore)

**Plan metadata:** the commit carrying this file (docs)

## Files Created/Modified

- `apps/desktop/src-tauri/src/pipeline/budget.rs` (588 lines) — the rig core: five-boundary `Stage`, `LatencyMark`, `Waterfall` (two constructors + `stage_alerts`), `BudgetVerdict`/`BudgetBreach`, `assert_within_budget`, `WaterfallRecorder` over `TimeSource`, `WaterfallAggregator` with cold/warm bounded rings, nearest-rank percentiles. Module docs carry the 02-02 interface contract, the streaming-vs-whole-request warning, and the privacy rule (no URL/headers/keys in timing records).
- `apps/desktop/src-tauri/src/pipeline/budget_test.rs` — the 8 behaviour tests (the 7 planned behaviours, with missing/duplicate/out-of-order marks split into hard-error assertions).
- `apps/desktop/src-tauri/src/pipeline/mod.rs` — pipeline module root; the mount point for 02-02 `stages/`, 02-03 `cascade.rs`/`breaker.rs`.
- `apps/desktop/src-tauri/src/lib.rs` — `pub mod pipeline;`.
- `apps/desktop/src-tauri/tests/latency_rig.rs` (236 lines) — the scripted-stage-double integration lane: five scripted segments pass the gate, the vendor arithmetic passes once stages overlap, the live precondition never passes silently, and `latency_e2e_cold` is `#[ignore]`d with an explicit unimplemented body that 02-02/02-03 must replace.
- `apps/desktop/src/hooks/useLatencyWaterfall.ts` (290 lines) — the `latency` event subscription, the deep payload-narrowing gate, and the preview fixture (built from the same script the Rust rig prints).
- `apps/desktop/src/components/LatencyWaterfall.tsx` (137 lines) — the presentational waterfall panel.
- `apps/desktop/src/components/LatencyWaterfall.test.tsx` — four panel behaviours + three hook behaviours (payload narrowing, malformed drop, preview fallback).
- `apps/desktop/src/pages/DiagnosticsPage.tsx` (40 lines) — the 诊断面板 page shell; cost panel slot marked by comment for 02-03 T3.7.
- `apps/desktop/src/App.tsx` — `/diagnostics` route.
- `.github/workflows/ci.yml` (73 lines) — four lanes; header comment carries the no-Cargo-workspace correction.
- `package.json` — `test:full` and `test:rig`.
- `.planning/phases/02-real-cloud-pipeline-audio-core/deferred-items.md` — out-of-scope findings from this plan.

## Decisions Made

1. **Two constructors, because one cannot express the truth.** `Waterfall::from_marks` derives each stage's duration as the gap to the previous boundary — an honest serial reading, but it mathematically forces `serial_sum_ms == e2e_ms`, which makes the overlap proof (Test 2/3) impossible. `from_marks_with_durations` takes the stages' own service durations, which is where the whole-request vendor numbers belong. `from_marks` is now `from_marks_with_durations(marks, boundary_gaps(marks))` — one code path, two entry points.
2. **Attribution is deterministic.** `heaviest_stage` walks `Stage::ALL` and keeps the first maximum (a `max_by_key` would silently pick the last), so "which stage caused this breach" never depends on map iteration order.
3. **Requirement numbering reconciled.** The plan frontmatter's `requirements: [AUDI-04]` follows the ROADMAP success-criteria numbering (延迟测量装置). In REQUIREMENTS.md that capability is **AUDI-06**, which is now checked. REQUIREMENTS.md's AUDI-04 is the cascade pipeline plus the "partial 渲染、final 发声" gate — it is *not* satisfied by this plan and stays Pending for 02-02/02-03. Marking AUDI-04 complete in REQUIREMENTS.md would have been a false claim.
4. **The panel's degraded modes are labelled, not simulated.** The preview fixture appears only when `listen()` rejects (no IPC bridge at all); a real Tauri run with no data yet shows 暂无测量数据. This keeps a browser preview reviewable without ever letting fake numbers pass as a measurement.
5. **`--workspace` is absent from ci.yml as a literal string.** The plan's own verify command asserts the file does not contain it, so the header comment explains the hazard ("任何假定存在 workspace 根清单的调用都会直接失败") without writing the flag itself.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] e2e CI lane builds before running playwright**
- **Found during:** Task 3 (CI integration)
- **Issue:** `playwright.config.ts` starts both webServers with `vite preview`, which only serves an existing `dist`. The plan's lane was `playwright install chromium` → `playwright test`, which would have failed on a clean runner with no build output.
- **Fix:** inserted `pnpm build` (the root script builds teleprompter + desktop) between the browser install and the test run.
- **Files modified:** `.github/workflows/ci.yml`
- **Verification:** locally reproduced the full lane order (`pnpm install` already done → `pnpm build` → `pnpm exec playwright test`) — 33 specs passed in 50.4s.
- **Committed in:** `fadd9ef`

**2. [Rule 1 - Bug] Reverted unrelated `cargo fmt` churn**
- **Found during:** Task 1 (GREEN)
- **Issue:** `cargo fmt` over the crate reflowed three expressions in `apps/desktop/src-tauri/src/sim/source_test.rs` — a file with no relationship to this plan. Landing it would have put unrelated churn in the feature commit.
- **Fix:** reverted that one file with `git checkout -- <file>`; logged the underlying rustfmt drift (and the pre-existing `unused variable: zh` warning at its line 248) to `deferred-items.md` instead of fixing them (SCOPE BOUNDARY).
- **Files modified:** `apps/desktop/src-tauri/src/sim/source_test.rs` (reverted, net zero), deferred-items.md (new)
- **Verification:** `git status --short` shows only the intended files; the full cargo suite stays green.
- **Committed in:** `ceed329` (the deferral log; the revert itself is a non-change)

**3. [Rule 3 - Blocking] `pnpm/action-setup` pinned by `packageManager`, not by an input**
- **Found during:** Task 3 (CI integration)
- **Issue:** the repo already locks `"packageManager": "pnpm@11.24.0"`; passing `with: version:` as well makes `pnpm/action-setup` refuse to run ("multiple versions of pnpm specified").
- **Fix:** omit the input and let the action read `packageManager`.
- **Files modified:** `.github/workflows/ci.yml`
- **Committed in:** `fadd9ef`

**4. [Rule 1 - Bug] Corrected a failing assertion written in the RED commit**
- **Found during:** Task 2 (GREEN)
- **Issue:** the preview-fixture hook test asserted every stage's p50 > 0, but `MicCallback` is the stopwatch origin and is 0ms by construction — the assertion could only pass if the fixture lied about the origin.
- **Fix:** assert all five stage keys exist, that a streaming stage carries a real number, and that the fixture's verdict is `withinBudget`.
- **Files modified:** `apps/desktop/src/components/LatencyWaterfall.test.tsx`
- **Verification:** vitest 35/35 green.
- **Committed in:** `1a6bc55`

---

**Total deviations:** 4 auto-fixed (3 blocking, 1 bug) + 5 documented design decisions
**Impact on plan:** All auto-fixes are correctness/CI-viability fixes with no scope creep; the design decisions implement the plan's intent where the plan's literal API list was self-contradictory (decision 1) or where following it would have produced a false completion claim (decision 3). No new dependencies were added, front or back.

## Issues Encountered

- The plan's `<action>` item 3 lists `Waterfall::from_marks` as the only constructor while behaviours Test 2/3 require a serial sum that exceeds the stopwatch — resolved by the two-constructor design above (documented in Decisions and in the module docs so 02-02 does not re-derive it).
- `cargo fmt` is not idempotent with the repo's current state: the crate was not fmt-clean at HEAD, so any repo-wide fmt run re-touches `sim/source_test.rs`. Handled by scoped revert + deferral rather than by expanding this plan's diff.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| The `latency` Tauri event has no producer yet | `apps/desktop/src/hooks/useLatencyWaterfall.ts` (consumer) | 02-01's file list contains no Rust emit site; the stage marks that would feed it are 02-02's work (`WaterfallRecorder::mark` at each stage's first byte). Until then a real run shows 暂无测量数据. The panel, the narrowing gate, the aggregation and the CI gate are all complete and exercised by tests. |
| Cost panel slot | `apps/desktop/src/pages/DiagnosticsPage.tsx` | Explicitly reserved by the plan for 02-03 T3.7 ("不要伪造数据"); no placeholder UI was rendered. |
| `latency_e2e_cold` has an `unimplemented!` body | `apps/desktop/src-tauri/tests/latency_rig.rs` | The live variant is `#[ignore]`d and its precondition already refuses to pass silently. 02-02/02-03 replace the body with the real cascade; it is not reachable in any default run. |

## User Setup Required

None at runtime — no new dependencies, no external service configuration, no secrets. CI needs nothing beyond the repo (the rig lane is zero-network by design). Running the *live* rig variant later will need the vendor credentials already documented in `tools/vendor-experiments/.env.example`.

## Next Phase Readiness

- **02-02 has a typed contract to build against, not a guess.** Every provider stage calls `WaterfallRecorder::mark` exactly once per segment at its own first streamed byte; `cold` is passed down by the session-start path. The runtime path should emit a `latency` event carrying `WaterfallReport` (camelCase serde) — the panel's narrowing gate already pins that shape, field by field.
- **02-03 inherits the gate.** The stability gate, jitter buffer and barge-in queue will be judged by the same ≤2000ms assertion; the `latency-rig` CI lane is already blocking.
- **02-03 also inherits a marked slot** for the per-stage cost breakdown on `/diagnostics`.
- **Watch item:** the rig currently has no producer on the Rust side, so the panel shows the empty state in a real run. This is expected until 02-02, not a regression — but it must be closed there, otherwise the gate would be measured only by the scripted double.
- **Deferred:** pre-existing rustfmt drift and an unused-variable warning in `sim/source_test.rs` (see `deferred-items.md`).

## Self-Check: PASSED

All 9 created artifacts present on disk (`budget.rs`, `mod.rs`, `budget_test.rs`, `latency_rig.rs`, `useLatencyWaterfall.ts`, `LatencyWaterfall.tsx`, `LatencyWaterfall.test.tsx`, `DiagnosticsPage.tsx` at 40 lines, `ci.yml`); all 5 task commits present in history (`abc129e`, `ceed329`, `b2c3756`, `1a6bc55`, `fadd9ef`).

---
*Phase: 02-real-cloud-pipeline-audio-core*
*Completed: 2026-10-03*
