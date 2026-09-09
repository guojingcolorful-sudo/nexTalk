---
gsd_state_version: 1.0
milestone: v3.4
milestone_name: milestone
status: executing
stopped_at: Plan 2 of Phase 1 complete
last_updated: "2026-09-09T11:40:00.000Z"
last_activity: 2026-09-09 -- 01-02 walking skeleton complete
progress:
  total_phases: 7
  completed_phases: 0
  total_plans: 5
  completed_plans: 2
  percent: 40
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-26)

**Core value:** 让用户以母语思考、以本人音色讲出地道英文——端到端延迟 ≤ 2 秒
**Current focus:** Phase 1: Foundation + Simulation Mode

## Current Position

Phase: 1 of 7 (Foundation + Simulation Mode)
Plan: 2 of 5 in current phase (01-01, 01-02 complete)
Status: Ready to execute next plan (01-03)
Last activity: 2026-09-09 -- 01-02 walking skeleton complete

Progress: [████░░░░░░] 40%

## Performance Metrics

**Velocity:**

- Total plans completed: 2
- Average duration: 16d (01-01 26h active-session + gap; 01-02 6d wall, ~7h active)
- Total execution time: 26h + ~7h active

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation + Simulation Mode | 2 | 5 | 16d wall (incl. idle gaps) |

**Recent Trend:**

- 01-02 walking skeleton (2026-09-09): 5 commits, 17 cargo + 3 vitest + 2 e2e tests green, 7 auto-fixed deviations

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [01-02 Open Question 3]: H5 accepts an optional `ws=` URL override param (default stays ws://{same-host}:8787) — required for e2e mock-server isolation; token stays mandatory so no added spoofing surface (T-01-01 gate unchanged)
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
- 01-03/01-04 entry CSS must `import '@nextalk/design-tokens'` and `import 'core-js/proposals/promise-with-resolvers'` (Pitfall 5) — teleprompter entry now complies (Task 3); desktop entry still needs a check
- Dev-machine port collision: an unrelated long-running tool (tools/jd-inbox-server.mjs) holds 127.0.0.1:8787 — the desktop LAN server binds 0.0.0.0:8787 and will EADDRINUSE while that tool runs (app degrades gracefully: bind failure logged, app continues); e2e previews already moved to 8791. Stop the tool before real-device pairing tests
- Playwright e2e now proves the mock-WS flow; the true QR → phone path (real LAN server + real token) is the manual end-of-phase check per plan

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-09-09T11:40:00.000Z
Stopped at: Completed 01-02-PLAN.md (walking skeleton: Tauri shell + LAN WS + SimSource + H5 stub)
Resume file: .planning/phases/01-foundation-simulation-mode/01-02-SUMMARY.md
