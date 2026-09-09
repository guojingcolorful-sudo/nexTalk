---
phase: 01-foundation-simulation-mode
plan: 02
subsystem: skeleton
tags: [tauri2, axum0.8, tokio-tungstenite, rust, react19, playwright, websocket, qrcode, typewriter]
requires: [01-01]
provides:
  - "Tauri two-window frameless transparent shell (console 340×680 visible, dual 860×680 hidden)"
  - "axum LAN server (0.0.0.0:8787) with pairing-as-auth WS upgrade + no-store static H5 serving"
  - "SessionState (Arc<RwLock> + tokio broadcast) with 32-hex token, timeline, replay-after-seq"
  - "Minimal REAL SimSource (deterministic script_state(elapsed_ms) evaluator + 100ms tick scheduler)"
  - "H5 teleprompter stub (token from URL, typewriter bilingual subtitle rendering)"
  - "QrCodeCard — live QR encoding http://{lan_ip}:8787/?token={token} via qrcode 1.5.4"
  - "e2e skeleton spec — real WS mock server: subtitle renders, malformed payload dropped"
affects: [01-03, 01-04, 01-05, phase-2]
tech-stack:
  added:
    - "src-tauri: axum 0.8.9 (ws), tower-http 0.7.0 (fs, set-header), futures-util 0.3.34, rand 0.10.2, local-ip-address 0.6.13; dev: tokio-tungstenite 0.28"
    - "@nextalk/teleprompter: @nextalk/protocol workspace dep; root: ws 8.21.3 devDep (mock WS in e2e)"
    - "@nextalk/desktop: @types/qrcode (qrcode 1.5.4 ships no bundled types)"
    - "vite preview script on both apps (Playwright webServer entrypoints)"
  patterns:
    - "One event model, two transports: SessionState.timeline appended once, emitted via Tauri `session` event AND WS broadcast (research Pattern 2)"
    - "Deterministic simulation: pure script_state(elapsed_ms) -> Vec<ServerEvent> evaluator + wall-clock ticker (100ms) — testable without timers"
    - "Pairing-as-auth: 128-bit token gate at WS upgrade (401 on mismatch), token rides the QR URL"
    - "Strict inbound parsing everywhere: serde container-level deny_unknown_fields + 64KB frame cap in Rust; isServerEvent narrowing before any H5 render"
    - "Keyed remount typewriter: one useTypewriter instance per language line (TypedLine component), exact 40ms cadence, reduced-motion instant reveal"
    - "ws= URL override param tolerated for dev/e2e (Open Question 3) — same-origin :8787 stays the default"
key-files:
  created:
    - "apps/desktop/src-tauri/src/{state.rs, lan/mod.rs, lan/server.rs, sim/mod.rs, sim/source.rs, sim/script.rs}"
    - "apps/desktop/src/components/QrCodeCard.tsx"
    - "apps/teleprompter/{index.html, src/main.tsx, src/App.tsx, src/styles/global.css, src/hooks/useWs.ts, src/hooks/useTypewriter.ts, src/hooks/useTypewriter.test.tsx}"
    - "e2e/skeleton.spec.ts"
  modified:
    - "apps/desktop/src-tauri/{Cargo.toml, tauri.conf.json, capabilities/default.json, src/lib.rs, src/main.rs, src/App.tsx, src/pages/ConsolePage.tsx}"
    - "apps/desktop/{package.json, index.html, src/main.tsx, src/styles/global.css, src/components/HeaderBar.tsx, src/components/NexTalkBrand.tsx, src/pages/DualPanePage.tsx}"
    - "playwright.config.ts, .gitignore, root package.json, pnpm-lock.yaml"
key-decisions:
  - "[Open Question 3] H5 accepts an optional ws= override param (default stays ws://{same-host}:8787) — required for e2e mock-server isolation; token is still mandatory so the override adds no spoofing surface (T-01-01 gate unchanged)"
  - "[e2e infra] Teleprompter Playwright preview moved 8787 -> 8791 — the desktop LAN port 8787 is squatted on this dev machine by an unrelated long-running tool (tools/jd-inbox-server.mjs, 127.0.0.1:8787); product default port unchanged"
  - "axum 0.8 root-level nesting is gone — ServeDir mounts via fallback_service (documented, avoids the panic); CACHE_CONTROL: no-store overrides all static responses (T-01-03)"
  - "rand 0.10 API: OsRng -> SysRng + TryRng::try_fill_bytes for the 128-bit pairing token"
  - "Internally-tagged serde enums put deny_unknown_fields + rename_all at the CONTAINER level (variant-level is a compile error)"
  - "Task 2 & Task 3 each ran as a full TDD cycle (RED commit -> GREEN commit); 17/17 cargo tests + 3/3 vitest + 2/2 e2e green"
requirements-completed: [UI-01, UI-03, DSK-01, DSK-02, SYNC-01, SYNC-05]
duration: 6d
completed: 2026-09-09
---

# Phase 1 Plan 2: Walking Skeleton Summary

**Tauri two-window frameless shell + axum LAN server with pairing-as-auth on 8787 + minimal REAL SimSource + H5 stub with typewriter rendering + live QR pairing card — proven end-to-end by 17 cargo tests, 3 vitest tests, and a Playwright e2e where one simulated subtitle flows Rust → WS broadcast → phone H5 and a malformed payload is dropped by the narrowing gate**

## Performance

- **Duration:** ~6d wall clock (sessions: 09-02 Task 1, 09-08 Tasks 2-3 RED, 09-09 Task 3 GREEN; ~7h active)
- **Started:** 2026-09-02
- **Completed:** 2026-09-09
- **Tasks:** 3 (Task 1 auto; Tasks 2 & 3 TDD = 5 commits total)
- **Files modified:** 38 (122-file Task-1 commit included Vite/Tauri scaffolding)

## What Was Built

| Task | Deliverable | Commits |
|------|-------------|---------|
| 1 | Tauri 2.11.5 two-window shell (console visible / dual hidden, frameless transparent, capabilities scoped per window) + design-system ConsolePage with 开始模拟会话 CTA | `1346363` |
| 2 | axum LAN server (0.0.0.0:8787): pairing-as-auth WS upgrade (401 on bad token), strict inbound parsing (64KB cap, deny_unknown_fields), no-store static H5 serving; SessionState with 128-bit token + timeline + replay-after-seq; minimal SimSource (deterministic script evaluator + 100ms scheduler, r1 script with UI-SPEC-locked strings); start/stop_session + get_pairing_info commands | `66f1c3e` (RED), `d6c583b` (GREEN) |
| 3 | H5 stub page (token from URL, optional ws= override) + useTypewriter (exact 40ms cadence, reduced-motion instant reveal) + useWs (isServerEvent gate, no reconnect yet) + QrCodeCard (get_pairing_info → qrcode 1.5.4 dataURL, exact caption) + Playwright e2e with real mock WS server | `f4dd3ad` (RED), `5915eec` (GREEN) |

## Verification Results

- `cargo test`: 17/17 green (token auth 401, broadcast + replay round-trips, script determinism, strict serde wire shapes matching @nextalk/protocol)
- `pnpm --filter @nextalk/teleprompter test`: 3/3 green (typewriter cadence with fake timers, reduced-motion collapse with 0 timers pending)
- `pnpm --filter @nextalk/teleprompter build` + `pnpm --filter @nextalk/desktop build`: green
- `pnpm exec playwright test e2e/skeleton.spec.ts`: 2/2 green — valid subtitle renders (bilingual, typewriter completes), malformed missing-seq payload never reaches the DOM (empty state intact)
- Root `pnpm run build`: green; no external http(s) refs in the built bundles (zero-CDN gate, UI-03)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Playwright teleprompter preview port 8787 → 8791**
- **Found during:** Task 3 e2e run (first-ever Playwright run in the repo)
- **Issue:** `Timed out waiting 60000ms from config.webServer`. An unrelated long-running user tool (`tools/jd-inbox-server.mjs`, up 22 days) holds `127.0.0.1:8787`; vite preview binds `::1` only, so Playwright's readiness fetch hit the user tool and hung. 8787 is the desktop LAN server's production port — e2e previews must not fight it.
- **Fix:** teleprompter webServer + baseURL moved to 8791 (verified free). Product default port 8787 unchanged.
- **Files modified:** playwright.config.ts
- **Commit:** `5915eec`

**2. [Rule 2 - Missing critical functionality] @types/qrcode devDep**
- **Found during:** Task 3 QrCodeCard implementation
- **Issue:** qrcode 1.5.4 ships no TypeScript definitions; `QRCode.toDataURL` import had no types.
- **Fix:** added the DefinitelyTyped `@types/qrcode` to @nextalk/desktop.
- **Files modified:** apps/desktop/package.json, pnpm-lock.yaml
- **Commit:** `5915eec`

**3. [Rule 3 - Blocking] axum 0.8 root-level ServeDir nesting panics**
- **Found during:** Task 2 server router wiring
- **Issue:** "Nesting at the root is no longer supported" — `nest_service` at root panics in axum 0.8.
- **Fix:** `fallback_service(ServeDir::new(...))` + `SetResponseHeaderLayer::overriding(CACHE_CONTROL, no-store)` (keeps T-01-03).
- **Files modified:** apps/desktop/src-tauri/src/lan/server.rs
- **Commit:** `d6c583b`

**4. [Rule 1 - Bug] serde `deny_unknown_fields` on internally-tagged enum variants is a compile error**
- **Found during:** Task 2 serde mirrors
- **Fix:** moved the attribute to the container level (`#[serde(tag="t", rename_all="snake_case", deny_unknown_fields)]`).
- **Files modified:** apps/desktop/src-tauri/src/lan/server.rs
- **Commit:** `66f1c3e`

**5. [Rule 3 - Blocking] tokio-tungstenite test client http-version clash**
- **Found during:** Task 2 cargo tests
- **Issue:** constructing `Request` by hand mixed tauri::http (0.2-ish) and tungstenite's http versions.
- **Fix:** `connect_async(&url)` via the `&str` IntoClientRequest impl; `ws_url` helper returns String.
- **Files modified:** apps/desktop/src-tauri/src/lan/server.rs (tests)
- **Commit:** `66f1c3e`

**6. [Rule 1 - Bug] clippy `absurd_extreme_comparisons` (elapsed_ms >= const 0)**
- **Found during:** Task 2 clippy gate
- **Fix:** question subtitle is pushed unconditionally in `script_state`; removed the QUESTION_AT_MS const.
- **Files modified:** apps/desktop/src-tauri/src/sim/source.rs
- **Commit:** `d6c583b`

**7. [Rule 2 - Missing critical functionality] .gitignore patterns didn't match src-tauri artifacts**
- **Found during:** Task 1 commit hygiene
- **Issue:** old patterns (bare `target/`) didn't match `apps/desktop/src-tauri/target/`.
- **Fix:** app-scoped patterns for `target/` + `gen/`; verified the Task-1 commit contains zero build artifacts.
- **Files modified:** .gitignore
- **Commit:** `1346363`

## TDD Gate Compliance

Both TDD plans verified in git log: Task 2 `test(01-02)` = `66f1c3e` (RED, 15 failing + 2 contract-passing) → `feat(01-02)` = `d6c583b` (GREEN, 17/17); Task 3 `test(01-02)` = `f4dd3ad` (RED, 2 failing + 1 boundary-passing) → `feat(01-02)` = `5915eec` (GREEN, 3/3 + 2/2 e2e). No REFACTOR commits needed. Compliant.

## Requirement Completion Decision

UI-01, UI-03, DSK-01, DSK-02, SYNC-01, SYNC-05 marked complete in REQUIREMENTS.md. **UI-02 (reference-HTML full screens) and SYNC-02 (full H5 teleprompter UI) intentionally left Pending** — 01-04 builds the complete phone teleprompter (subtitle history, strategy cards, pairing screen) on top of this plan's stub; 01-03 completes the desktop hub/dual-pane surfaces. Sync reconnect + resume also land in 01-04 (useWs has no reconnect yet, matching the plan).

## Threat Surface Scan

No new surface beyond the plan's `<threat_model>`: the only novel entry point is the H5 `ws=` URL override (documented key decision above; token gate unchanged), and the e2e mock WS server is test-only on 127.0.0.1:8788. All T-01 dispositions implemented: T-01-01 (SysRng token, 401), T-01-02 (deny_unknown_fields + 64KB cap in Rust; isServerEvent gate + e2e malformed-payload proof in H5), T-01-03 (no-store, session-scoped replay), T-01-06 (capabilities/default.json per-window scoping).

## Self-Check: PASSED

All 5 commit hashes present in git log (`1346363`, `66f1c3e`, `d6c583b`, `f4dd3ad`, `5915eec`); all key files exist (state.rs, lan/server.rs, sim/source.rs, QrCodeCard.tsx, App.tsx, useWs.ts, useTypewriter.ts + test, e2e/skeleton.spec.ts); root build + full cargo suite re-run green after the final commit.
