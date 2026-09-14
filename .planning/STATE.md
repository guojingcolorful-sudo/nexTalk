---
gsd_state_version: 1.0
milestone: v3.4
milestone_name: milestone
status: executing
stopped_at: Phase 1 complete — automated gates green + human UAT passed 2026-09-14
last_updated: "2026-09-14T00:00:00.000Z"
last_activity: 2026-09-14 -- Phase 1 human UAT passed; marking complete
progress:
  total_phases: 7
  completed_phases: 1
  total_plans: 28
  completed_plans: 5
  percent: 18
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-26)

**Core value:** 让用户以母语思考、以本人音色讲出地道英文——端到端延迟 ≤ 2 秒
**Current focus:** Phase 2: Real Cloud Pipeline + Audio Core (pre-planning: vendor experiments)

## Current Position

Phase: 2 of 7 (Real Cloud Pipeline + Audio Core)
Plan: 0 of 5 in current phase
Status: Phase 1 complete (verification 22/22 + human UAT passed 2026-09-14); Phase 2 awaits vendor experiments → discuss → plan
Last activity: 2026-09-14 -- Phase 1 human UAT passed; marking complete

Progress: [██░░░░░░░░] 18% (5/28 plans, Phase 1/7 done)

## Performance Metrics

**Velocity:**

- Total plans completed: 5
- Average duration: ~1d wall (01-01 26h active-session + gap; 01-02 6d wall, ~7h active; 01-03 ~1h active; 01-04 ~2.5h active over two sessions; 01-05 ~30 min active)
- Total execution time: 26h + ~7h + ~1h + ~2.5h + ~0.5h active

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation + Simulation Mode | 5 | 5 | ~1d wall avg (incl. idle gaps) |

**Recent Trend:**

- 01-05 simulation session + vendor framework (2026-09-11): 4 commits (1 RED + 1 GREEN), 29 cargo tests (27 lib + 2 integration) + 29 playwright specs + 71 vitest green, build 129.39 kB gz JS / 5.37 kB gz CSS, 6 auto-fixed deviations, 1 decision forced by Tauri 2.11 (no app-command ACL namespace)
- 01-04 phone teleprompter (2026-09-11): 4 commits (1 RED + 1 GREEN), 27 vitest + 5 new playwright specs (10/10 with --repeat-each=2, 26/26 full suite) green, build 93.30 kB gz JS / 4.48 kB gz CSS, 8 auto-fixed deviations
- 01-03 desktop surface (2026-09-10): 5 commits (1 RED + 1 GREEN), 11 vitest + 19 desktop e2e (21 total across projects) green, build 128.70 kB gz JS / 5.37 kB gz CSS, 7 auto-fixed deviations
- 01-02 walking skeleton (2026-09-09): 5 commits, 17 cargo + 3 vitest + 2 e2e tests green, 7 auto-fixed deviations

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [01-05]: Tauri 2.11 has NO ACL namespace for app-defined commands (`gen/schemas/acl-manifests.json` lists only `core*`), so T-01-06 is enforced in the commands themselves — `interrupt`/`repeat` return Err unless the session is `generating`, `start_session` returns Err while one is live; `capabilities/default.json` was deliberately left untouched
- [01-05]: One event model, two transports holds end-to-end — the SimSource only appends to `SessionState.timeline`; the Tauri `session` emit and the WS broadcast are two projections of the same list, so console/dual/phone cannot drift
- [01-05]: `phone_count` is desktop-only telemetry on a Tauri event, never a WS ServerEvent — the locked 01-01 protocol union gained nothing for the client counter
- [01-05]: ChatBubble language resolution is `localPref ?? session mode ?? speaker default` — the phone's mode seeds every bubble the user has not personally toggled, keeping 01-03's per-bubble independence intact
- [01-05]: `repeat` (重听) re-emits a round under `-r{n}` ids with fresh seq (the phone's resume dedupe can never swallow a replay); `interrupt` (打断) cuts immediately and opens the next round at `+1000 ms` (`INTERRUPT_LEAD_MS`)
- [01-05]: SimSource determinism contract — `script_state(elapsed_ms)` is pure/IO-free and the scheduler takes an injectable `TimeSource`, so engine tests never sleep; scheduler cancellation is a u64 epoch ticket, not JoinHandle bookkeeping
- [01-05]: Vendor framework (D-04) ships zero dependencies and zero keys — Node built-ins only, TLS-only RTT tool that takes the credential's environment variable NAME (`--auth-env`, with `--auth-header`/`--auth-scheme` for Bearer/Token/raw vendors), and the report records `hasKey` (presence) only
- [01-04]: The phone owns ONE session-level language mode (中/EN/EN+中) pushed as {t:control,language} — per-bubble toggles stay desktop-only; the inbound `language` ServerEvent renders nothing on the phone (it is the desktop observation channel for 01-05)
- [01-04]: The phone's wake-lock fallback is a bundled 977-byte H.264 loop fetched with Vite `?no-inline` (real cached asset, zero CDN, no runtime media synthesis); it needs the same 开始提词 gesture as `wakeLock.request`
- [01-04]: Teleprompter tab is URL state (`?tab=ai`) written with history.replaceState so `?token=` survives; a phone waking from sleep returns to the tab it was reading
- [01-04]: Playwright clock discipline — `page.clock.install()` alone still lets real time through (a leaked tick made the 20-char typewriter checkpoint read 21); freeze with `install()` + `pauseAt()` before navigation and poll the DOM from Node, because Playwright auto-wait polls with page rAF
- [01-04]: e2e mocks the desktop with its own `ws` server on an EPHEMERAL port reached through the `?ws=` override — no port is reserved, and 8787 stays the untouched product default
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
- 01-04 Task 3 `<human-check>` (real-device pass: QR scan → 开始提词 → screen awake ≥2 min → wifi kill 10s → reconnect/resume) is still outstanding — the executor has no phone or camera. Automated equivalents are green (mock-WS e2e covers pairing mount, wake fallback and reconnect/resume); run the hardware pass before `/gsd:verify-work`
- 01-05 Task 2 `<human-check>` (interactive `pnpm --filter @nextalk/desktop tauri dev` demo pass: QR scan → 开始模拟会话 → r1 flows to console + dual + phone in sync → phone count flips to 已连接 1 台设备 → phone mode switch → 打断/重听 → ended) is outstanding — needs a GUI session + phone + camera. Every leg has a green automated equivalent (29 cargo tests incl. the real-WS integration test; 29 playwright specs incl. demo.spec.ts); run it before `/gsd:verify-work`
- Phase 1 is code-complete (5/5 plans, all automated gates green at HEAD) but its three human passes (01-03 desktop walkthrough, 01-04 real-device phone, 01-05 full demo) are the remaining end-of-phase manual checks
- Playwright e2e now proves the mock-WS flow for BOTH surfaces; the true QR → phone path (real LAN server + real token) is the manual end-of-phase check per plan

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-09-11T06:15:49.000Z
Stopped at: Completed 01-05-PLAN.md (4-round SimSource engine, live demo wiring across all surfaces, vendor experiment framework) — Phase 1 has no plans left to execute
Resume file: .planning/phases/01-foundation-simulation-mode/01-05-SUMMARY.md
