---
quick_task: stop-keeps-stream
status: complete
task: 停止后双栏保留字幕与策略（与手机端一致），仅在新会话 session_started 清空重来——撤销 246baae 的 ended 清空
date: 2026-09-30
commits:
  - 627c321 docs(quick): plan the stop-keeps-stream fix
  - a442841 fix(desktop): keep the rendered stream after 停止, clear only on the next session
files_modified:
  - apps/desktop/src/hooks/useTauriEvents.ts
  - apps/desktop/src/hooks/useTauriEvents.test.ts
  - apps/desktop/src/pages/DualPanePage.test.tsx
  - apps/desktop/src/pages/DualPanePage.tsx
  - e2e/desktop.spec.ts
rust_changes: none
new_dependencies: none
acceptance: A1 A2 A3 A4 A5 A6 A7
---

# Quick Task: 停止后双栏保留内容 (20260930-stop-keeps-stream)

**停止 no longer wipes the dual pane — the subtitles and strategy stay on screen like the phone, and only the next session's `session_started` clears and relocks them.**

## Performance

- **Duration:** ~30 min
- **Tasks:** 1/1
- **Files:** 0 created, 5 modified (0 Rust, 0 teleprompter). `DualPanePage.test.tsx` was the 4th file the task brief missed; `DualPanePage.tsx` is a 2-line comment fix (see Deviations).

## Task Commits

1. **Plan** — `627c321` `docs(quick): plan the stop-keeps-stream fix`
2. **Task 1 — retention fix (TDD: RED to GREEN)** — `a442841` `fix(desktop): keep the rendered stream after 停止, clear only on the next session`

Each commit staged by explicit path; the concurrent debug session's files (`.planning/ROADMAP.md`,
`.planning/ref/*`, `.planning/research/*`, `.planning/debug/`) were never staged. No Co-Authored-By
trailer (verified with `git log -1 --format='%B'`).

## What Changed

### `useTauriEvents.ts` — the `ended` clear is gone; `session_started` remains the only reset

`246baae` had routed both status channels through `applyStatus(next)`, which cleared `events` when
`next === 'ended'`. That whole helper is deleted; the two call sites are back to plain `setStatus`:

- `session` channel `{t:'status',session:'ended'}` — appended to the stream, no longer followed by a clear.
- `session_status` channel — sets status only.
- The `session_started` branch (`setEvents([])` + `setLanguageMode(null)`) is untouched and now carries
  the decision comment (WR-02/CR-01 + 2026-09-30 retention rationale) so the behavior cannot silently swing back.
- Malformed-payload guards (`isServerEvent` / `narrowStatus`) still gate everything (T-QUICK-01 unchanged);
  the clear surface shrank to one entry point.

### `DualPanePage.tsx` — comment only, zero logic change

File-header tail now reads "the panes keep their content after the stop (与手机端一致) — only the next
session's `session_started` clears them". The empty-state conditions (`subtitles.length === 0`,
`timelineItems.length === 0`) were already correct and are untouched — A4 verified by T3 asserting no
empty state while content is retained.

### Tests flipped (RED evidence recorded before the implementation change)

| # | File | Assertion |
|---|------|-----------|
| T1 | `useTauriEvents.test.ts` | `ended` on both channels keeps 3 events (subtitle + strategy + appended status), `status === 'ended'`; a following `session_started` empties them. RED: `expected length 3, received 0`. |
| T2 | `useTauriEvents.test.ts` | Pre-existing "new session clears + resets languageMode" case left verbatim — anchors A3 and catches over-correction. |
| T3 | `DualPanePage.test.tsx` | New `STRATEGY_EVENT` fixture; case 5 now proves both panes keep their content after `ended` (no 等待语音输入 / AI 策略将自动生成, pill and 停止 gone), then `session_started` restores the locked empty states. RED: `Unable to find … QUESTION_EVENT.en`. |
| T4 | `e2e/desktop.spec.ts` | Stop flow emits a strategy event too; `ended` keeps both panes populated and the window open, `session_started` re-empties. The locked confirm copy assertions (停止会话？/ 当前字幕与策略将清空) are unchanged — per plan §9, the copy still holds lifecycle-semantically. |

## Verification

| Gate | Command | Result |
|---|---|---|
| RED (hook + component) | `vitest run src/hooks/useTauriEvents.test.ts src/pages/DualPanePage.test.tsx` | 2 failed / 8 passed — both flips red for the predicted reason |
| GREEN (same target) | same command | 2 files / 10 tests passed |
| Desktop unit suite | `pnpm --filter @nextalk/desktop test` | 7 files / 27 tests passed |
| Build | `pnpm --filter @nextalk/desktop build` | built in 7.22s — 413.24 kB JS (131.13 kB gz) / 26.20 kB CSS (5.53 kB gz) |
| E2E | `pnpm exec playwright test e2e/desktop.spec.ts --project=desktop --grep "dual pane"` | 6 passed (22.2s) — includes the renamed stop-flow spec |
| Regression (non-blocking) | `pnpm exec playwright test e2e/demo.spec.ts --project=desktop` | 5 passed (17.1s) |

Environment: nothing was killed. The debug session's vite instance was reused where live; Playwright
started its own `vite preview` on 8791 for the teleprompter (port was free) and tore it down itself.

## Acceptance Criteria

- A1/A2 — retention on both `ended` channels proven at hook level (T1) and component level (T3).
- A3 — `session_started` clear + `languageMode` reset: T2 unchanged and still green; T1/T3 assert the re-clear.
- A4 — empty states only when nothing rendered: T3 asserts their absence while content is retained.
- A5 — e2e stop-flow retains both panes, window open, no `plugin:window|close`; then `session_started` re-empties.
- A6 — desktop vitest 27/27, `--grep "dual pane"` 6/6, `demo.spec.ts` 5/5.
- A7 — diff scope is exactly the 5 TS/TSX files; no Rust, no teleprompter, no new dependencies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] `DualPanePage.test.tsx` not listed in the task brief**
- **Found during:** planning (pre-execution file audit)
- **Issue:** the brief said 3 files; `DualPanePage.test.tsx` (created by `56f90f4`) asserted the old
  empty-after-stop behavior at case 5 and would have gone red.
- **Fix:** planned and executed the flip; plan §3 documented the correction with a new strategy fixture.
- **Files modified:** `apps/desktop/src/pages/DualPanePage.test.tsx`
- **Commit:** `a442841`

**2. [Rule 2 - Misleading documentation] `DualPanePage.tsx` header comment described the reverted behavior**
- **Found during:** plan review
- **Issue:** the header said "the Rust terminal state drives both panes back to their empty states" —
  exactly the semantics this task reverses. Leaving it would re-seed the same regression (the prior
  round's comment/behavior coupling is what caused this fix).
- **Fix:** 2-line comment rewrite; zero JSX/logic change, zero test impact. The plan explicitly allowed
  this (§3 修正说明).
- **Files modified:** `apps/desktop/src/pages/DualPanePage.tsx`
- **Commit:** `a442841`

No other deviations. The confirm-modal copy was left untouched as instructed.

## Known Stubs

None — no placeholder values, no unwired data sources. The change is a state-retention fix plus comments.

## Threat Flags

None — no new endpoints, auth paths, file access, or trust boundaries. The clear surface actually
shrank to a single entry point (T-QUICK-01 in the plan).

## Self-Check: PASSED

- Files: all 5 modified files present and committed; PLAN.md exists and SUMMARY.md content returned to
  the orchestrator (subagent write policy blocked the file).
- Commits: `627c321`, `a442841`, `9f8cf6d` all present in `git log`.
