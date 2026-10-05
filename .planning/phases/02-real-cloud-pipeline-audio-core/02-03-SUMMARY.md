---
phase: 02-real-cloud-pipeline-audio-core
plan: 03
subsystem: pipeline (stability gate + trace + cost/user panels)
tags: [cascade, commit-gate, segment-aggregation, vad, barge-in, playout, retry, breaker, degraded-display, abstention, numeric-validation, jsonl-trace, cost-metering, failure-cases, ci]

# Dependency graph
requires:
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 02
    provides: Stage contracts (SttSource/Translator/TtsSink) + StageError/RetryClass + the D-07 confidence/trace/abstained wire extension on both ends
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 01
    provides: The five Stage latency marks + assert_within_budget rig the cascade feeds, and the /diagnostics slot the cost panel fills
provides:
  - "Commit gate (Cascade/CascadeConfig): the single enforcement point for GOV-15 spoken ⊆ committed — partials render, only committed finals go downstream"
  - "Sentence aggregation (segment.rs) + local-energy VAD (vad.rs) for silence-closed segment boundaries"
  - "Epoch-guarded playout queue (audio/playout.rs): barge-in interrupts with capped fade; the base 02-05's jitter buffer extends"
  - "Fragment retry (exactly 2, 100→200ms, 500ms budget) + per-vendor breaker (2 fails → 120s open → half-open probe) — breaker.rs"
  - "Degraded display on both frontends (红色微章 + 锁定文案 + 原文 + 正在重试) and silent abstention (待翻译) — ChatBubble desktop + teleprompter"
  - "Deterministic output validation (validate.rs): numbers/units/dates across every spoken candidate, mismatch → reject → fallback to the original"
  - "Single-writer JSONL trace (trace/*): TraceRecord/TraceWriter + UsageSummary/CostReport + the usage_summary command"
  - "Failure-case library runner (--check/--run) + 20 executable regression cases + the CI failure-cases lane + root test:full wiring"
  - "UsageMinutesPanel (user view, /setup) — used/remaining minutes with an 80% non-blocking hint"
affects: [02-04 live probes (usage metering attach point, cross-lingual clone risk), 02-05 audio core (playout queue is the jitter-buffer base), Phase 4 termHits fill, Phase 5 strategy cards (confidence presentation moved there), Phase 8 usage aggregation (GOV-10 six classes on every trace line)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Single-writer mpsc trace pipeline: producers try_send and never block; a full bounded queue (1024) is counted (dropped_records) rather than grown unboundedly"
    - "Locked degraded copy reuse: numeric-validation rejections and vendor failures share one degraded form (T-02-14) — no second wording"
    - "Failure-case regression targets must be executable test identifiers (cargo:/vitest:) — the runner greps for existence before executing, so a case can never point at thin air"
    - "Frontend panels read Rust-produced numbers only (no recomputation); preview fixtures are typed and always explicitly labelled 预览数据"

key-files:
  created:
    - apps/desktop/src-tauri/src/pipeline/cascade.rs
    - apps/desktop/src-tauri/src/pipeline/segment.rs
    - apps/desktop/src-tauri/src/pipeline/vad.rs
    - apps/desktop/src-tauri/src/pipeline/validate.rs
    - apps/desktop/src-tauri/src/pipeline/breaker.rs
    - apps/desktop/src-tauri/src/audio/playout.rs
    - apps/desktop/src-tauri/src/trace/mod.rs
    - apps/desktop/src-tauri/src/trace/jsonl.rs
    - apps/desktop/src-tauri/tests/cascade_integration.rs
    - apps/desktop/src-tauri/tests/stability.rs
    - apps/desktop/src/components/UsageMinutesPanel.tsx
    - apps/desktop/src/components/UsageMinutesPanel.test.tsx
    - tools/vendor-experiments/failure-cases/run.mjs
    - "tools/vendor-experiments/failure-cases/0003..0020-*.json (18 new cases)"
  modified:
    - apps/desktop/src-tauri/src/pipeline/{mod,confidence}.rs
    - apps/desktop/src-tauri/src/audio/mod.rs
    - apps/desktop/src-tauri/src/state.rs
    - apps/desktop/src-tauri/src/lib.rs
    - apps/desktop/src/components/ChatBubble.tsx (+test)
    - apps/desktop/src/pages/{DiagnosticsPage,SetupWizardPage}.tsx (+ DiagnosticsPage.test)
    - apps/teleprompter/src/components/ChatBubble.tsx (+test)
    - apps/teleprompter/src/pages/TeleprompterPage.tsx
    - e2e/abstention.spec.ts
    - e2e/degraded.spec.ts
    - tools/vendor-experiments/failure-cases/000{1,2}-*.json (prose targets → executable ids)
    - .github/workflows/ci.yml
    - package.json

key-decisions:
  - "提交门是 GOV-15「spoken English ⊆ committed finals」的唯一执法点：Cascade 只在 committed 文本上向下游放行（T3.1+T3.2 同一提交门的两半，签名测试）"
  - "抢话：interrupt 先自增 session_epoch 再清 playout——未播出音频丢弃、已播出部分带上限淡出（~100ms）；最小语音门限 300–350ms 防自我打断循环（T3.3）"
  - "片段级重试恰好 2 次、退避 100→200ms、总预算 500ms 内放弃该片段流下一片段；供应商连续 2 次失败 → 熔断 120s → 半开探测（T3.4/T3.5）"
  - "弃权只发生在「无有效文本」（低置信永不弃权）；字幕不做置信标记（GOV-01/02 2026-09-30 修订，置信呈现归 Phase 5）；数字校验拦截复用锁定降级文案（T-02-14），不新造措辞；置信来源三值内部枚举（Vendor/Proxy/ProxyUnavailable）落 JSONL、线上仍两值（T3.6+GOV-04）"
  - "JSONL 单写者：有界 mpsc 队列（1024，满即计数丢弃）、8MB 滚动、0600、手写 civil 日期（不引入 chrono）；费率表为具名常量（火山无公开费率，用 Fish S2.1 Pro CJK 参考价并注明）（T3.7）"
  - "失败案例 regression_test 升级为可执行的 `cargo:`/`vitest:` 测试标识；枚举校验容忍「枚举（细节）：说明」的既有写法；0001/0002 的散文目标重写为真实测试（T3.8）"
  - "用量面板提示式：80% 阈值只提示不阻断（D-15/GOV-17）；分阶段明细留在 /diagnostics（D-13 分层）；无 IPC 桥时类型化 fixture + 显式「预览数据」标注；挂载点 /setup 向导区之外（T3.9）"

patterns-established:
  - "TDD 每任务 RED/GREEN 两提交（test → feat）；样例驱动的 runner 自身也进 CI 车道并接进 test:full"
  - "面板数据单来源：Rust 命令/事件是唯一数字来源，前端只做展示（诊断页费率标注「估算」，用量页只显示分钟数）"
  - "预览 fixture 必须带显式标注（预览数据：未连接桌面测量装置）——假数字永不冒充实测"

requirements-completed: [AUDI-06, GOV-01, GOV-02, GOV-03, GOV-04, GOV-06, GOV-07, GOV-09, GOV-10, GOV-12, GOV-13, GOV-14, GOV-15, GOV-17, GOV-19, GOV-20]

# Metrics
duration: "~5h active over three sessions (~28h wall; 2026-10-04 16:59 → 2026-10-05 21:13 +0800, idle gaps included)"
completed: 2026-10-05
---

# Phase 2 Plan 03: Stability Gate Summary

**The cascaded pipeline becomes trustworthy end to end: spoken English ⊆ committed finals enforced at one gate, barge-in/retry/breaker/abstention behaving, every sentence landing as one JSONL trace line with staged usage, a 20-case failure library running in CI, and the user-facing minutes panel on the setup page**

## Performance

- **Duration:** ~5h active over three sessions (~28h wall; 2026-10-04 16:59 → 2026-10-05 21:13 +0800, idle gaps included)
- **Started:** 2026-10-04T08:59:00Z
- **Completed:** 2026-10-05T13:13:00Z
- **Tasks:** 7/7 auto tasks (no checkpoints — autonomous plan)
- **Files touched:** 62 unique (57 code/tool + 5 planning metadata)

## Accomplishments
- The signature invariant holds: `Cascade` admits only committed finals downstream (GOV-15, spoken ⊆ committed zero-violation), with sentence aggregation and silence-closed segment boundaries on top of a local-energy VAD.
- Barge-in is deterministic: the epoch-guarded playout queue drops unplayed audio, caps the fade on what already played, and a 300–350ms minimum-speech gate keeps self-interrupt loops out (stability.rs).
- The fault path is a first-class citizen: fragment retry (2, 100→200ms, 500ms budget), per-vendor breaker (2 fails → 120s → half-open), and a Chinese-locked degraded form (红色微章 + 「翻译服务暂时不可用」 + 原文 + 「正在重试」) rendered identically on desktop and phone.
- Abstention is silent and exact: only "no valid text" abstains (低置信永不弃权) and the subtitle surface carries no confidence badges (GOV-01/02 2026-09-30 revision — confidence presentation belongs to Phase 5 strategy cards); confidence provenance lives on the wire/trace via the three-value internal `ConfidenceSource`.
- Deterministic numeric validation covers every spoken candidate: any number/unit/date mismatch rejects the translation and falls back to the original through the existing locked degraded copy (T-02-14) — no second wording invented.
- Every sentence lands as one JSONL record (single writer, bounded queue with drop accounting, 8MB rolling, 0600, `traces/<date>/<sessionId>.jsonl`) carrying the D-05/D-08/GOV-10 fields and the three-stage usage shape; `UsageSummary`/`CostReport` aggregate the month through a named rate table (火山 uses the Fish S2.1 Pro CJK reference — no published rate) and the /diagnostics cost panel renders it with an 估算 chip and 超预算 badge.
- The failure-case library is executable: `run.mjs --check` validates schema/enums/4-digit ids/≥20 cases and greps every regression target; `--run` executes each target (cargo filter / vitest `-t`) — 20/20 green with the two legacy prose targets rewritten to real tests; the CI `failure-cases` lane and root `test:full` run both.
- The user view is live on /setup: 本月已用 X 分钟 / 剩余 Y 分钟, an 80% non-blocking hint (用量已接近本月额度，请留意), 暂无用量数据 instead of 0/NaN, a labelled preview fixture when the command is unreachable, and the 纯本地 provenance note — staged detail stays in /diagnostics (D-13 layering).
- Full gates green: cargo 232 passing (165 lib + 8 cascade_integration + 3 latency_rig + 31 mock_vendors + 5 session_integration + 19 stability + 1 doctest; 1 live `#[ignore]`), vitest 162 across 4 packages, playwright 37 passed / 4 skipped, failure-cases 20/20, desktop build 428.06 kB JS / 135.26 kB gz (safari15 target), zero vendor keys required.

## Task Commits

Each task was committed atomically (TDD pairs; Task 6 is a non-TDD auto task):

1. **Task 1 (T3.1+T3.2): 提交门、句子聚合与本地能量 VAD** — `1dd36c6` (test, 2026-10-04) → `c6755f3` (feat)
2. **Task 2 (T3.3): 抢话中断（epoch 守卫队列 + 最小语音门限）** — `02e0bb6` (test) → `83746d9` (feat)
3. **Task 3 (T3.4+T3.5): 片段级重试、熔断与降级展示** — `e8c0596` (test) → `8d15bbf` (feat)
4. **Task 4 (T3.6 + GOV-04): 三因子置信、低置信标记与确定性输出校验层** — `aa8c415` (test) → `20b9193` (feat)
5. **Task 5 (T3.7): JSONL 溯源落盘、分阶段成本计量与开发者成本面板** — `2d02a8e` (test) → `b1a196e` (feat) + `6f313df` (pin test: bounded writer queue drop accounting)
6. **Task 6 (T3.8): 失败案例库 runner 与 20 例回归集** — `637d4af` (feat; no `tdd` attribute — single commit)
7. **Task 7 (T3.9): 设置页「剩余分钟数」面板** — `7a31a34` (test) → `8de3346` (feat)

**Plan metadata:** (this summary commit) + the follow-up state commit

## Files Created/Modified
- `apps/desktop/src-tauri/src/pipeline/cascade.rs` — the commit gate: partials render, only committed finals ($\subseteq$) reach translator/TTS; the GOV-15 enforcement point
- `apps/desktop/src-tauri/src/pipeline/segment.rs` / `vad.rs` — sentence aggregation and local-energy VAD (silence closes, transients discarded)
- `apps/desktop/src-tauri/src/audio/playout.rs` / `audio/mod.rs` — epoch-guarded playout queue, interrupt drops unplayed audio and fades the tail with a cap
- `apps/desktop/src-tauri/src/pipeline/breaker.rs` — `RetryPolicy`/`CircuitBreaker`/`BreakerState` (2 retries, 100→200ms, 500ms budget; 2 fails → 120s → half-open)
- `apps/desktop/src-tauri/src/pipeline/validate.rs` — deterministic number/unit/date consistency over every spoken candidate; mismatch → original
- `apps/desktop/src-tauri/src/pipeline/confidence.rs` — internal `ConfidenceSource` gains `Serialize`/`Deserialize` (snake_case; `Default` = `ProxyUnavailable`) so JSONL records carry the three-value provenance while the wire enum stays two-value
- `apps/desktop/src-tauri/src/trace/mod.rs` / `jsonl.rs` — `TraceRecord` (D-05/D-08/GOV-10 fields + `StageUsage`), `TraceWriter` (single writer task, bounded queue 1024 with `dropped_records`, 8MB roll, 0600, slug-guarded paths), `UsageSummary`/`CostReport` (named rate table, civil-date dirs, month prefix aggregation)
- `apps/desktop/src-tauri/src/state.rs` — `append_event` mirrors every sentence event to the writer (timeline ⟷ JSONL isomorphism, D-18/GOV-10); session start/stop open/close the writer
- `apps/desktop/src-tauri/src/lib.rs` — `usage_summary` command (usage + cost + quota) registered; trace dir wired in setup
- `apps/desktop/src/pages/DiagnosticsPage.tsx` (+test) — the 02-01 cost slot filled: 成本明细 with STT/翻译/TTS/合计 rows, 估算 chip, 超预算 badge, 暂无用量数据 empty state
- `apps/desktop/src/components/UsageMinutesPanel.tsx` (+test) — the user view: used/remaining minutes, 80% non-blocking hint, empty state, labelled preview fallback, 纯本地 note
- `apps/desktop/src/pages/SetupWizardPage.tsx` — 用量 section mounted outside the wizard steps
- `apps/desktop/src/components/ChatBubble.tsx` + `apps/teleprompter/src/components/ChatBubble.tsx` (+tests) — degraded form and 待翻译 marker on both surfaces
- `apps/desktop/src-tauri/tests/cascade_integration.rs` / `stability.rs` — the committed-vs-partial, VAD, barge-in, breaker and abstention integration proof
- `tools/vendor-experiments/failure-cases/run.mjs` + 18 new cases + 2 rewritten — the executable failure library
- `.github/workflows/ci.yml` (failure-cases lane; header comment corrected to 五条车道) + `package.json` (`test:full` appends check+run)
- `e2e/abstention.spec.ts` / `e2e/degraded.spec.ts` — the end-to-end assertions for the two failure displays

## Decisions Made
See `key-decisions` in the frontmatter — the commit-gate single enforcement point, barge-in epoch semantics, the retry/breaker numbers, abstention/no-badge/numeric-fallback rules, the trace pipeline's bounded single-writer shape and rate table, the executable failure-case targets, and the prompt-only usage panel with /diagnostics layering.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `UsageSummary` re-export was private (E0603)**
- **Found during:** Task 5 (`state.rs` wiring)
- **Issue:** `trace/mod.rs` had `use jsonl::UsageSummary;` (private), so `state.rs`/`lib.rs` could not import it.
- **Fix:** Promoted to `pub use jsonl::UsageSummary;`.
- **Files modified:** `apps/desktop/src-tauri/src/trace/mod.rs`
- **Committed in:** `b1a196e`

**2. [Rule 1 - Bug] Forward-compatible trace reads: `status` needed a field-level serde default**
- **Found during:** Task 5 (RED suite, `partial_lines_read_with_field_defaults`)
- **Issue:** A partial/older JSONL line (`{}`) failed deserialization with "missing field `status`" — the plan requires `#[serde(default)]` forward compatibility on the persisted struct.
- **Fix:** Added `#[serde(default)]` to `status` (field-level only). The struct deliberately keeps **no** `Default` derive so the GOV-10 "missing class doesn't compile" doctest witness stays intact (the doctest passes).
- **Files modified:** `apps/desktop/src-tauri/src/trace/jsonl.rs`
- **Committed in:** `b1a196e`

**3. [Rule 3 - Blocking] RTL auto-cleanup is inactive in this repo's vitest**
- **Found during:** Task 5 (empty-state test inherited the previous test's 超预算 badge DOM)
- **Issue:** `vitest.config.ts` sets no `globals: true`, so Testing Library's auto-cleanup never runs; DOM leaks between tests in one file.
- **Fix:** Explicit `afterEach(() => cleanup())` in the new test files (repo precedent: `DualPanePage.test.tsx`). Test-isolation wiring only; no assertion changed.
- **Files modified:** `apps/desktop/src/pages/DiagnosticsPage.test.tsx`, `apps/desktop/src/components/UsageMinutesPanel.test.tsx`
- **Committed in:** `b1a196e` / `7a31a34`

**4. [Rule 3 - Blocking] Plan text says "CI 第四车道" but ci.yml already carried four lanes**
- **Found during:** Task 6
- **Issue:** unit-web / e2e / rust / latency-rig were already four parallel lanes; the failure-cases job is the fifth.
- **Fix:** Added the job as its own lane and corrected the header comment (四条 → 五条). The plan's done criterion — the lane exists and runs `--check && --run` — is met unchanged.
- **Files modified:** `.github/workflows/ci.yml`
- **Committed in:** `637d4af`

**5. [Rule 2 - Missing critical coverage] failure-case 0016 had no executable regression target**
- **Found during:** Task 6 (mapping cases to real tests)
- **Issue:** The 队列无界增长 case needs a test proving the bounded queue counts drops instead of growing — none existed (the writer's `try_send` + drop counter had no focused test).
- **Fix:** Added `a_full_queue_drops_counted_records_instead_of_blocking` (capacity-2 config; three back-to-back `append`s on a current-thread runtime deterministically refuse the third; flush lands exactly two lines). Pinned in its own commit before the case JSON references it.
- **Files modified:** `apps/desktop/src-tauri/src/trace/jsonl.rs`
- **Committed in:** `6f313df`

### Plan revisions honored (not deviations)

- The 2026-09-30 GOV-01/02 revision (confidence presentation moved to Phase 5 strategy cards) is already encoded in the plan's must_haves: the subtitle surface ships **no** confidence badges, and the abstention e2e (`e2e/abstention.spec.ts`) carries the 待翻译 assertions.
- Numeric-validation rejections reuse the locked degraded copy (T-02-14) rather than inventing a second wording — `numeric_mismatch` renders through the same degraded form as vendor failures.

---

**Total deviations:** 5 auto-fixed (3 Rule 3 blocking, 1 Rule 1 bug, 1 Rule 2 coverage; no scope creep, no architectural changes)
**Impact on plan:** None — all five were required to compile, to keep test isolation honest, to keep documentation truthful, or to give the plan's own case library a real target.

## Issues Encountered
- The plan's verify `pnpm --filter @nextalk/desktop test -- UsageMinutesPanel` does not actually narrow vitest to the file — it runs the full desktop suite (10 files / 49 tests) and both legs pass; noted so the pattern is not mistaken for a per-file filter.
- The fail-fast TDD rule did not fire anywhere: every RED suite failed for the intended reason (missing APIs / unresolved import), every GREEN passed on the first run after implementation.

## Known Stubs
- **Live trace usage values are zeros until stage-level counters exist.** `TraceRecord::from_event` builds records whose `StageUsage` (STT audio ms / translate tokens / TTS chars) defaults to zero; the record schema, single-writer persistence, `UsageSummary`/`CostReport` aggregation, rate table, `usage_summary` command and both panels (成本明细 + UsageMinutesPanel) are complete and tested against synthetic values. The attach point is `TraceRecord::with_usage` — wire the real counters there once the cascade stages meter them (02-04 live assembly / Phase 3 telemetry). Consequence today: a real session's cost panel and 剩余分钟数 read zero. The plan's Task 5 criteria (fields assertable, aggregation + panel complete) are met; this is the deliberate boundary the plan draws.

## User Setup Required
None new. All suites (cargo/vitest/playwright/failure-cases `--check`) run keyless; the real-key smoke paths remain `#[ignore]`d. The failure-cases `--run` leg needs only `cargo` + `pnpm` on PATH (the runner adds `~/.cargo/bin` when present).

## Next Phase Readiness
- 02-04's live probes now drive an assembled cascade: commit gate, retry/breaker, degraded/abstain paths and the trace writer are on by default, and the flagged 火山 cross-lingual clone risk note still rides in the client for T4.0 to arbitrate.
- 02-05's jitter buffer extends `audio/playout.rs` — epoch/interrupt semantics and the capped fade are pinned by stability tests.
- Phase 5 strategy cards consume `confidenceSource` (trace + wire) for their presentation; the subtitle surface intentionally shows no badges.
- Phase 8 aggregation inherits the GOV-10 six classes on every JSONL line and the local-only, 0600 trace layout.

---
*Phase: 02-real-cloud-pipeline-audio-core*
*Completed: 2026-10-05*

## Self-Check: PASSED

- Summary file: FOUND (`.planning/phases/02-real-cloud-pipeline-audio-core/02-03-SUMMARY.md`)
- Task commits: FOUND all 14 (`1dd36c6`, `c6755f3`, `02e0bb6`, `83746d9`, `e8c0596`, `8d15bbf`, `aa8c415`, `20b9193`, `2d02a8e`, `b1a196e`, `6f313df`, `637d4af`, `7a31a34`, `8de3346`)
- Key files: FOUND all 16 checked artifacts (cascade/segment/vad/validate/breaker/playout/trace mod+jsonl/cascade_integration/stability/UsageMinutesPanel(+test)/run.mjs/abstention.spec/degraded.spec)
