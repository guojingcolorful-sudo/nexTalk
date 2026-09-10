---
gsd_state_version: 1.0
milestone: v3.4
milestone_name: milestone
status: executing
stopped_at: Plan 3 of Phase 1 complete
last_updated: "2026-09-10T04:40:00.000Z"
last_activity: 2026-09-10 -- 01-03 desktop surface complete
progress:
  total_phases: 7
  completed_phases: 0
  total_plans: 5
  completed_plans: 3
  percent: 60
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-26)

**Core value:** 让用户以母语思考、以本人音色讲出地道英文——端到端延迟 ≤ 2 秒
**Current focus:** Phase 1: Foundation + Simulation Mode

## Current Position

Phase: 1 of 7 (Foundation + Simulation Mode)
Plan: 3 of 5 in current phase (01-01, 01-02, 01-03 complete)
Status: Ready to execute next plan (01-04)
Last activity: 2026-09-10 -- 01-03 desktop surface complete

Progress: [██████░░░░] 60%

## Performance Metrics

**Velocity:**

- Total plans completed: 3
- Average duration: ~11d wall (01-01 26h active-session + gap; 01-02 6d wall, ~7h active; 01-03 ~1h active)
- Total execution time: 26h + ~7h + ~1h active

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation + Simulation Mode | 3 | 5 | ~11d wall (incl. idle gaps) |

**Recent Trend:**

- 01-03 desktop surface (2026-09-10): 5 commits (1 RED + 1 GREEN), 11 vitest + 19 desktop e2e (21 total across projects) green, build 128.70 kB gz JS / 5.37 kB gz CSS, 7 auto-fixed deviations
- 01-02 walking skeleton (2026-09-09): 5 commits, 17 cargo + 3 vitest + 2 e2e tests green, 7 auto-fixed deviations

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [01-03]: Language preference is per-bubble local state (not a global store) — each ChatBubble seeds itself from the speaker default (interviewer `bilingual`, user `all-zh`) and toggles independently, satisfying SYNC-03
- [01-03]: The locked @nextalk/protocol ServerEvent union has NO draft variant, so the UI-SPEC green draft timeline node has no producer in Phase 1 — AiTimeline ships context + strategy nodes only; a protocol decision is needed before the draft node can exist
- [01-03]: Testing Library auto-cleanup is not active (vitest globals are off) — React specs MUST call afterEach(cleanup) explicitly or a prior render leaks into the next query
- [01-03]: Voice enrollment probes the real microphone via navigator.mediaDevices.getUserMedia as the Phase-1 permission path, releasing all tracks on stop/unmount; actual capture/cloning is Phase 2+. e2e injects a deterministic navigator.mediaDevices (deny + grant paths) rather than depending on headless-Chromium permission behavior
- [01-03]: PageStub deleted — all six previously-stubbed routes (setup / voice / glossary / resume / recordings / review) render real pages with locked Chinese copy and 模拟数据 badges
- [01-03]: ReviewPage exposes 生成报告/重新生成 so the 暂无复盘报告 empty state is reachable, not dead; RecordingAssetCard export actions render disabled until Phase 6; the voice-sample 试听 tile is an explicitly labelled Phase-2 placeholder
- [01-03 Open Question 3]: H5 accepts an optional `ws=` URL override param (default stays ws://{same-host}:8787) — required for e2e mock-server isolation; token stays mandatory so no added spoofing surface (T-01-01 gate unchanged)
- [01-02 e2e infra]: Playwright teleprompter preview moved 8787 → 8791 — an unrelated long-running local tool (tools/jd-inbox-server.mjs) squats 127.0.0.1:8787 on this dev machine; product default port 8787 unchanged (see Blockers)
- [01-02]: axum 0.8 dropped root nesting — ServeDir mounts via fallback_service + no-store override header; serde internally-tagged enums put deny_unknown_fields at the CONTAINER level (variant-level is a compile error)
- [01-02]: rand 0.10 API — OsRng → SysRng + TryRng::try_fill_bytes for the 128-bit pairing token (T-01-01)
- [01-01 Task 1 gate]: USER APPROVED all 20 npm audit-table packages (2026-08-28) after re-verifying @fortawesome/fontawesome-free 6.7.2 against the npm registry — repo matches FortAwesome/Font-Awesome, scripts={}, publisher fortawesome-admin, dist.integrity present; observed weekly downloads 2.5M vs audit's ~15M (same magnitude, not a risk)
- [01-01]: Rust stable 1.98.0 (2026-08-18) builds against Xcode 14.2 — Open Question A2 resolved favorably, no 1.85 toolchain pin needed
- [01-01]: pnpm 11.24.0 via corepack; locked `packageManager: "pnpm@11.24.0"`; pnpm 11 `allowBuilds` approved for core-js + esbuild postinstall scripts
- [01-01]: @vitejs/plugin-react pinned 5.1.4 (plan's 6.1.0 requires vite 8; plan locks vite 7.3.6)
- [01-01]: @playwright/test pinned 1.53.2 (plan's 1.62.1 cannot install browsers on macOS 12; 1.53.2 is the newest mac12-compatible line — upgrade blocked until OS upgrade)
- [01-01]: contract tests read tokens.css via node:fs (vitest 4 stubs .css imports incl. ?raw)
- [Roadmap]: Follow research build order — protocol/UI first on SimSource (driver off critical path), real pipeline + latency rig before virtual device, copilot after transcript pipeline, consent gate ships with recording
- [Roadmap]: AUDI-07 glossary term protection mapped to Phase 4 (per research); glossary page UI built in Phase 1, wiring into Phase 2 pipeline stages happens in Phase 4
- [Roadmap]: Phase 7 Productization carries no v1 requirement mappings — distribution hardening per research (signing/notarization, clean-machine test, compliance)

### Pending Todos

[From .planning/todos/pending/ — ideas captured during sessions]

None yet.

### Blockers/Concerns

[Issues that affect future work]

- Phase 1 must run the decisive vendor experiments (STT A/B, clone listening test, network RTT) before Phase 2 stack wiring — results may change provider choices
- Claude Sonnet 5 intro pricing and Fish Audio free tier end 2026-08-31 — cost model must assume post-intro pricing
- BlackHole install/signing is a process risk (Gatekeeper, notarization, meeting-app device caches) — isolated in Phase 3
- Playwright capped at 1.53.2 while this machine runs macOS 12 (1.62.1 refuses mac12) — OS upgrade unblocks newer Playwright; e2e runs on the 1.53.2 chromium build
- ~~01-03/01-04 entry CSS must `import '@nextalk/design-tokens'` and `import 'core-js/proposals/promise-with-resolvers'` (Pitfall 5)~~ RESOLVED 2026-09-10: both entries comply — the desktop `main.tsx` boots with the core-js polyfill import first and keeps `@nextalk/design-tokens` before `./styles/global.css`; the teleprompter entry complied in 01-02
- `tsc` is unusable as a gate: `@types/react` / `@types/react-dom` are not installed in the workspace, so typecheck output is dominated by pre-existing errors (see 01-03 deferred-items.md)
- 01-03 Task 4 `<human-check>` (`pnpm --filter @nextalk/desktop tauri dev` manual walkthrough) is still outstanding — the executor ran the full automated suite (21 e2e across both projects) but cannot drive a GUI session
- Dev-machine port collision: an unrelated long-running tool (tools/jd-inbox-server.mjs) holds 127.0.0.1:8787 — the desktop LAN server binds 0.0.0.0:8787 and will EADDRINUSE while that tool runs (app degrades gracefully: bind failure logged, app continues); e2e previews already moved to 8791. Stop the tool before real-device pairing tests
- Playwright e2e now proves the mock-WS flow; the true QR → phone path (real LAN server + real token) is the manual end-of-phase check per plan

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-09-10T04:40:00.000Z
Stopped at: Completed 01-03-PLAN.md (desktop surface: console hub + dual pane + six pages)
Resume file: .planning/phases/01-foundation-simulation-mode/01-03-SUMMARY.md
