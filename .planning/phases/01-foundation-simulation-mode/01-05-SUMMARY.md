---
phase: 01-foundation-simulation-mode
plan: 05
subsystem: simulation-engine
tags: [rust, tauri, axum, tokio-broadcast, websocket, react, playwright, vitest, tdd, deterministic-sim, vendor-experiments, json-schema, node, neobrutalism]

# Dependency graph
requires:
  - phase: 01-01
    provides: locked protocol (@nextalk/protocol ServerEvent union with seq + the language variant), design tokens, Space Grotesk/FontAwesome local bundling
  - phase: 01-02
    provides: SessionState (timeline + tokio broadcast) and the token-gated LAN server with resume replay, the two-window Tauri shell, the minimal r1 sim state, start_session/stop_session, the session/session_status events
  - phase: 01-03
    provides: ConsolePage / DualPanePage / useTauriEvents / QrCodeCard (the phone_count consumer) / ChatBubble per-bubble language toggles
  - phase: 01-04
    provides: the phone's {t:control,language} send + {t:resume,sinceSeq} handshake, and the ephemeral-port mock-WS e2e pattern
provides:
  - The 4-round deterministic SimSource — script.rs data + the pure script_state(elapsed_ms) evaluator + an injectable TimeSource scheduler with a session-long monotonic seq
  - Session lifecycle commands interrupt (打断) / repeat (重听) with epoch-ticket scheduler cancellation
  - Live wiring engine → SessionState.timeline → broadcast → Tauri session emit + WS fan-out → console, dual pane, phone
  - phone_count Tauri event (live WS client counter) closing the QrCodeCard 已连接 N 台设备 / 等待扫码 status line
  - The SYNC-03 round trip — inbound {t:control,language} applied to SessionState and republished as the language ServerEvent the desktop renders
  - tools/vendor-experiments/ (D-04) — the no-keys policy README, the STT A/B protocol, the blind clone test-set JSON Schema, and the offline-safe node:https RTT tool
affects: [01-phase-verification, phase-2-planning, phase-2-stack-wiring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - 'Pure evaluator + stateful driver split: script_state(elapsed_ms) is IO-free and byte-deterministic; the scheduler owns the clock, the epoch ticket and the appends'
    - 'Session-epoch cancellation: start/stop bump a u64; every spawned scheduler exits when its ticket goes stale — no JoinHandle bookkeeping'
    - 'One event model, two transports: the engine only appends ServerEvents to SessionState.timeline; Tauri emit and WS broadcast are two projections of the same list'
    - 'Lock order invariant: engine mutex → state lock, never reversed'
    - 'phone_count is desktop-only telemetry (Tauri event), deliberately NOT a ServerEvent — the 01-01 protocol union stays untouched'
    - 'Tauri 2.11 does not ACL-gate app-defined commands, so command gating is substantive (state predicates), not declarative'
    - 'Vendor experiments ship as protocol + schema + offline tooling; a credential can only arrive from the environment at run time'

key-files:
  created:
    - apps/desktop/src-tauri/src/sim/source_test.rs
    - apps/desktop/src-tauri/tests/session_integration.rs
    - e2e/demo.spec.ts
    - tools/vendor-experiments/README.md
    - tools/vendor-experiments/stt-ab-protocol.md
    - tools/vendor-experiments/blind-clone-test-set.schema.json
    - tools/vendor-experiments/rtt/measure.mjs
  modified:
    - apps/desktop/src-tauri/src/sim/script.rs
    - apps/desktop/src-tauri/src/sim/source.rs
    - apps/desktop/src-tauri/src/sim/mod.rs
    - apps/desktop/src-tauri/src/state.rs
    - apps/desktop/src-tauri/src/lan/server.rs
    - apps/desktop/src-tauri/src/lib.rs
    - apps/desktop/src/pages/ConsolePage.tsx
    - apps/desktop/src/pages/DualPanePage.tsx
    - apps/desktop/src/hooks/useTauriEvents.ts
    - apps/desktop/src/components/ChatBubble.tsx
    - e2e/desktop.spec.ts
    - playwright.config.ts

key-decisions:
  - '打断 / 重听 are rejected unless the session is generating (a state predicate) — Tauri 2.11 has no app-command ACL namespace, so T-01-06 is enforced in code rather than by extending capabilities/default.json'
  - 'The desktop observes the phone-applied language mode through the language ServerEvent on the same session stream (no second channel); ChatBubble resolves localPref ?? session mode ?? speaker default, so the phone mode seeds every bubble the user has not touched'
  - 'repeat re-emits the round under "-r{n}" suffixed ids with fresh seq, so the phone dedupe can never swallow a replay'
  - 'The RTT tool refuses a non-TLS endpoint and refuses a secret passed where an environment variable NAME belongs; --help is the only path Phase 1 exercises'
  - 'The vendor framework carries no key handling beyond a run-time environment lookup — the report records hasKey (presence), never the value'

patterns-established:
  - 'Determinism contract: sim time advances through an injected TimeSource (scripted in tests, Instant in prod), so no test ever sleeps'
  - 'Interrupt is a one-second hand-off: the cut lands immediately and the next round opens at cut + INTERRUPT_LEAD_MS (1000 ms), asserted on both the state and the wire'
  - 'Replay/dedupe safety: ids carry an "-r{n}" replay suffix and seq never repeats, so a resume replay and a 重听 replay are distinguishable on the phone'
  - 'Desktop-only telemetry (phone_count) travels on a Tauri event, never on the phone protocol'

requirements-completed: [SYNC-01, SYNC-03, DSK-04, UI-02]

# Metrics
duration: ~30 min active (13:40:11 → 14:10:17 +08:00)
completed: 2026-09-11
tasks: 3
files: 19
commits: 4 (+1 metadata)
---

# Phase 1 Plan 05: Simulation Session + Vendor Experiment Framework Summary

**One-liner:** The walking skeleton became a complete simulated product — one button plays a deterministic 4-round interview whose subtitles and strategy cards stream in sync to the console, the dual pane and the phone over one event model, with 打断/重听, a live connected-phone count, a phone→desktop language round trip, and a zero-key vendor experiment framework ready for the Phase 2 planning inputs.

## Performance

- **Duration:** ~30 min active (first 01-05 commit 13:40:11 +08:00 → last task commit 14:10:17 +08:00 on 2026-09-11)
- **Started:** 2026-09-11 (first 01-05 commit 13:40:11 +08:00)
- **Completed:** 2026-09-11T14:10:17+08:00 (last task commit; metadata commit follows)
- **Tasks:** 3/3 complete
- **Files changed:** 19 (+2961 / −216)
- **Commits:** 4 task commits (1 RED + 1 GREEN TDD pair, then Task 2 and Task 3) + 1 metadata commit

## Accomplishments

- **Task 1 — The full 4-round SimSource engine (RED `a6d11d1` → GREEN `2fffebe`):** `script.rs` holds the four rounds as pure data (r1 keeps the locked 数据库优化 content — the question and answer byte-match `.planning/ref/reference-mockup.html`; r2 线上故障排查 / r3 分布式经验 / r4 职业规划 under a `// D-03 discretion: technical questions` marker). `source.rs` separates the pure evaluator `script_state(elapsed_ms)` (byte-identical output for the same elapsed time — the determinism contract) from the stateful `SimSource` driver, which keeps the round index, the session-long monotonic seq, and the replay counter, and advances through an injectable `TimeSource` (scripted in tests, `RealClock` in prod, `TICK_MS = 100`). `interrupt` cuts the current answer and re-opens the next round at `cut + INTERRUPT_LEAD_MS` (1000 ms); `repeat` re-emits the round under `-r{n}` ids with fresh seq. All five behaviour tests plus the state lifecycle tests (start/stop/restart, generating-only gating, non-negative client counter) run without sleeping.
- **Task 2 — Live demo wiring across every surface (`3395a42`):** `state.rs` publishes through the existing broadcast (`publish` / `publish_status` / `publish_all`), owns the epoch ticket, and tracks `connected_clients`; `lan/server.rs` applies the inbound `{t:'control', language}` (deny_unknown_fields intact), then republishes `ServerEvent::Language { language }` through the same broadcast so the desktop observes the phone's mode without a second channel, and bumps the client counter on connect/disconnect; `lib.rs` registers `interrupt` / `repeat` beside the existing commands. On the desktop, `ConsolePage` became status-driven (CTA `开始模拟会话` → `会话进行中` / `回答生成中` + 停止 behind the locked confirm modal, 打断/重听 pair, 扩展视图 gated on a live session), `DualPanePage` put 打断/重听 on the pill row, `useTauriEvents` surfaces `languageMode`, and `ChatBubble` resolves `localPref ?? mode ?? speaker default`. `tests/session_integration.rs` drives the real `SessionState` + a real axum server + a real `tokio-tungstenite` client through a whole session: r1 playback, phone sync, the language round trip, 打断, 重听 and the client counter (1 → 2 → 1 → 0).
- **Task 3 — Vendor experiment framework, zero keys (`fc22e40`):** `tools/vendor-experiments/` ships the D-04 policy README (no API keys in Phase 1, credentials only from the environment at run time, never on a command line), the STT A/B protocol (fixed 中/EN utterance set, warmup rule, 3 runs, CER/WER + p50/p95 rubric, result-table templates), the blind clone test-set JSON Schema (vendor labels confined to `voice_candidates`; the graded sheet carries blind ids only), and `rtt/measure.mjs` — a dependency-free `node:https` tool that sends N sequential requests and reports ttfb/total with min/p50/p95/max plus an `OUTPUT.json` report. `--help` exits 0 with no network access.
- **Verification (final HEAD):** `cargo test --workspace` 29/29 (27 lib + 2 integration); `pnpm exec playwright test` 29/29 across both projects; `pnpm -r test` 71/71 vitest (protocol 20, design-tokens 13, desktop 11, teleprompter 27); `cargo clippy --all-targets -- -D warnings` clean; `pnpm build` green (desktop 129.39 kB gz JS / 5.37 kB gz CSS against the 300/50 kB budgets); the Task 3 verify command (`--help` + schema parse) exits 0.

## Task Commits

| # | Task | Phase | Commit | Type |
|---|------|-------|--------|------|
| 1a | Failing specs for the 4-round sim engine | 01-05 | `a6d11d1` | test (RED) |
| 1b | The 4-round deterministic SimSource engine (+ the SessionState surface it drives) | 01-05 | `2fffebe` | feat (GREEN) |
| 2 | Live demo wiring: session lifecycle, control round trip, 打断/重听 on all surfaces | 01-05 | `3395a42` | feat |
| 3 | Vendor experiment framework (README, STT protocol, clone schema, RTT tool) | 01-05 | `fc22e40` | feat |

**Plan metadata:** `(this commit)` (docs: complete plan)

## Files Created / Modified

**Created (7):** `src-tauri/src/sim/source_test.rs` (the 5 behaviour specs), `src-tauri/tests/session_integration.rs` (full session ↔ WS integration), `e2e/demo.spec.ts` (3 specs over the mocked Tauri bridge), `tools/vendor-experiments/README.md`, `tools/vendor-experiments/stt-ab-protocol.md`, `tools/vendor-experiments/blind-clone-test-set.schema.json`, `tools/vendor-experiments/rtt/measure.mjs`.

**Modified (12):** `sim/script.rs` (4 rounds, r1 locked), `sim/source.rs` (pure evaluator + engine + scheduler), `sim/mod.rs` (module surface), `state.rs` (publish/epoch/clients/interrupt/repeat), `lan/server.rs` (control apply + language republish + client count), `lib.rs` (interrupt/repeat commands), `pages/ConsolePage.tsx`, `pages/DualPanePage.tsx`, `hooks/useTauriEvents.ts`, `components/ChatBubble.tsx` (session mode), `e2e/desktop.spec.ts` (CTA expectations), `playwright.config.ts` (spec routing).

## Decisions Made

- **打断/重听 gating lives in the state, not in a capability file.** Tauri 2.11's ACL manifest only names `core*` namespaces — an app-defined command has no namespace to grant, so extending `capabilities/default.json` cannot express T-01-06. `interrupt_session` / `repeat_session` return `Err` unless the status is `Generating`, `start_session` returns `Err` while a session is live, and the integration test proves both.
- **One observation channel for the language mode.** The phone's control message is applied to `SessionState` and immediately republished as the `language` ServerEvent on the same broadcast; the desktop needs no second transport and the phone keeps ignoring the variant (its own display is local).
- **`localPref ?? mode ?? speaker default`.** The session mode seeds every bubble the user has not personally toggled, which keeps 01-03's "each bubble is independent" behaviour intact while making the phone's mode visible on the desktop.
- **Replay ids carry a suffix.** `repeat` re-emits with `-r{n}` ids and fresh seq so the phone's resume dedupe cannot mistake a 重听 replay for already-seen history.
- **The RTT tool is deliberately narrow.** TLS-only (the tool refuses `http://`), sequential-only, and it takes the credential's *variable name* rather than the secret; `--auth-header` / `--auth-scheme` cover Bearer, Token and raw-key vendors without the tool knowing any vendor's name.
- **No new dependencies.** The engine, the integration test and the RTT tool all ride on crates and Node built-ins already present (the `tests/` crate sees the app's regular dependencies — no dev-dependency additions were needed).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] T-01-06 gating moved from `capabilities/default.json` into the commands**
- **Found during:** Task 2 (live wiring), verified against `gen/schemas/acl-manifests.json`
- **Issue:** The plan's threat register asks to "extend the 01-02 capabilities/default.json to grant the sim commands". Tauri 2.11's ACL manifest contains only `core, core:app, core:event, core:image, core:menu, core:path, core:resources, core:tray, core:webview, core:window` — app-defined commands have no namespace, so a capability entry cannot gate them. Leaving it at that would have shipped the threat with no mitigation at all.
- **Fix:** Enforced the substance of T-01-06 in code: `interrupt` / `repeat` return `Err` unless the session is generating, `start_session` returns `Err` while one is running, and `tests/session_integration.rs` asserts both rejections. Recorded as a decision above so the verifier can confirm the intent.
- **Files modified:** `apps/desktop/src-tauri/src/state.rs`, `apps/desktop/src-tauri/src/lib.rs`
- **Verification:** the integration test drives a real command surface; the counter/gating assertions pass; `capabilities/default.json` is untouched (confirmed via `git log --name-only` over the plan range).
- **Committed in:** `2fffebe` / `3395a42` (task commits)

**2. [Rule 1 - Bug] Integration test failed to compile (E0716, three sites)**
- **Found during:** Task 2 (writing `session_integration.rs`)
- **Issue:** `sub(&read_until(...).await)` borrowed a temporary that was dropped at the end of the statement.
- **Fix:** Added a `read_subtitle(ws, what)` helper that binds the owned event before borrowing it.
- **Files modified:** `apps/desktop/src-tauri/tests/session_integration.rs`
- **Verification:** `cargo test --workspace` compiles and passes.
- **Committed in:** `3395a42`

**3. [Rule 1 - Bug] The integration test read the wrong frame after the language control**
- **Found during:** Task 2 (the assertion saw `Status { Generating }` where it expected `Language { AllEn }`)
- **Issue:** the subtitle helper skipped intermediate frames, so the round's trailing status frame was still queued when the next assertion ran.
- **Fix:** added `read_until(&mut ws, what, predicate)` and consumed frames until the language event arrived.
- **Files modified:** `apps/desktop/src-tauri/tests/session_integration.rs`
- **Verification:** the full-session test passes deterministically.
- **Committed in:** `3395a42`

**4. [Rule 1 - Bug] Latent frame-ordering bug in the 重听 assertion**
- **Found during:** Task 2 (test review before commit)
- **Issue:** a fixed three-iteration collect would have swallowed the pending r2 / strategy / status frames and asserted on the wrong events.
- **Fix:** collect frames until three replay frames are seen, keeping only ids ending in `-r1`.
- **Files modified:** `apps/desktop/src-tauri/tests/session_integration.rs`
- **Verification:** the replay assertions pass and would fail if `repeat` re-emitted without the suffix.
- **Committed in:** `3395a42`

**5. [Rule 1 - Bug] `e2e/demo.spec.ts` asserted an English line the default bubble hides**
- **Found during:** Task 2 (the dual-pane demo spec timed out)
- **Issue:** the user bubble defaults to `all-zh`, so the cloned English line is intentionally hidden until the phone applies a mode — the spec asserted visibility too early.
- **Fix:** assert the English line is absent before the mode change and present (with the Chinese gone) after it, which is exactly the SYNC-03 behaviour the spec exists to prove.
- **Files modified:** `e2e/demo.spec.ts`
- **Verification:** 3/3 demo specs pass, including the mode-change section.
- **Committed in:** `3395a42`

**6. [Rule 2 - Missing critical functionality] Files the plan did not list were required to finish Task 2**
- **Found during:** Task 2
- **Issue:** the plan's file list named the two pages and the hook but not the bubble that renders the lines, nor the existing specs/config that the new CTA behaviour invalidates.
- **Fix:** added `components/ChatBubble.tsx` (the mode has to reach the renderer for SYNC-03 to be observable) and updated `e2e/desktop.spec.ts` + `playwright.config.ts` (the console CTA is now status-driven, and the new spec had to be routed away from the H5 project).
- **Files modified:** `apps/desktop/src/components/ChatBubble.tsx`, `e2e/desktop.spec.ts`, `playwright.config.ts`
- **Verification:** the full 29-spec Playwright suite is green across both projects.
- **Committed in:** `3395a42`

### Notes

- **Task 2's `state.rs` bullet landed in the Task 1 GREEN commit** (`2fffebe`). The engine's unit tests drive the same state surface (publish/epoch/clients/interrupt/repeat), so the two could not be split without leaving the Task 1 suite red. The remaining Task 2 Rust work (server control handling, command registration) is in `3395a42` as planned.
- **`tools/vendor-experiments/` needed no escaping from the plan's constraints**: no new packages (Node built-ins only), no network call anywhere in the Phase 1 path, and the schema carries no secret-shaped fields (T-01-15).

---

**Total deviations:** 6 auto-fixed (2 missing critical, 4 bugs/blocking)
**Impact on plan:** Every fix was required for correctness or for the planned behaviour to be observable; none added scope. The one judgement call — moving T-01-06's mitigation into the commands — was forced by the framework version and is recorded as a decision for the verifier.

## Issues Encountered

- **`tsc --noEmit` remains unusable as a gate** (pre-existing, logged in `deferred-items.md`): `@types/react` / `@types/react-dom` are absent from the workspace, so typecheck output is dominated by `JSX.IntrinsicElements` errors in untouched files. `vite build` + vitest + Playwright carry the type signal instead.
- **`prettier --check` is not a repo-wide gate either** (pre-existing, 60 files including every planning doc): the four files this plan created are prettier-clean, but the two touched pages and the touched specs were already unformatted at HEAD, and formatting them would have produced unrelated churn. Logged to `deferred-items.md` rather than fixed here.
- **A `git commit -m "$(cat <<EOF)"` invocation failed once** because the message contained an apostrophe; the message was written to a temp file and committed with `git commit -F`. No content was lost.

## Human Demo Pass (outstanding)

The plan's Task 2 `<human-check>` (interactive `tauri dev` pass: QR scan → 开始模拟会话 → sync across console + dual + phone → 打断/重听 → phone mode switch → phone count → ended) was **not performed by the executor** — it needs a GUI session, a phone and a camera. It remains the end-of-phase manual check before `/gsd:verify-work`, with the automated equivalents proving each leg:

| Human step | Automated equivalent (green) |
|---|---|
| Session flows to console + dual in sync | `e2e/demo.spec.ts` (console + dual sections) |
| Phone receives the same timeline over WS | `tests/session_integration.rs` over a real axum server + real WS client |
| 打断 cuts, next round opens in 1 s | integration test (`cut_at + INTERRUPT_LEAD_MS - 1` empty, then r2 at the boundary); demo spec 打断 section |
| 重听 replays under fresh ids | integration test (`-r1` ids, seq > previous) |
| Phone mode reaches the desktop | integration test (control → `language` event) + demo spec (ZH gone, EN shown) |
| QrCodeCard counts the phone | integration test (0 → 1 → 2 → 1 → 0) + demo spec (等待扫码 ↔ 已连接 1 台设备) |
| 停止 confirms then ends | demo spec (取消 does not call `stop_session`; 停止 does) |

Everything the execution environment could verify is verified; the GUI/phone pass needs the user.

## User Setup Required

None — no external service configuration. Phase 1 runs fully offline with no keys (the vendor experiments are scripts and docs that the user will run between Phase 1 and Phase 2 planning, with credentials exported in their own shell).

## Next Phase Readiness

- **Phase 1 is code-complete:** all five plans (01-01…01-05) have shipped, and this plan closes the fifth roadmap success criterion (现场演示) on the automated evidence above.
- **Ready for Phase 2 planning:** the SimSource boundary (`script_state` + `TimeSource`) is where the real pipeline will plug in — the events, the seq contract and the broadcast are already what the UI consumes; the Phase 2 stack decision waits on the D-04 experiments, whose framework is now in place.
- **Carry-forward blockers (see STATE.md):** the Task 2 human demo pass above; the 01-03 / 01-04 human passes (desktop walkthrough, real-device phone pass); the dev-machine 8787 collision with `tools/jd-inbox-server.mjs`; and the vendor experiments must run before Phase 2 wiring.

---

*Phase: 01-foundation-simulation-mode*
*Completed: 2026-09-11*

## Self-Check: PASSED

- Files: all 7 created files found on disk (`sim/source_test.rs`, `tests/session_integration.rs`, `e2e/demo.spec.ts`, the 4 `tools/vendor-experiments/` files) + this SUMMARY.
- Commits: `a6d11d1`, `2fffebe`, `3395a42`, `fc22e40` all present in `git log --oneline --all`.
- Test claims re-verified at the final tree: `cargo test --workspace` 29/29, `pnpm exec playwright test` 29/29, `pnpm -r test` 71/71, `pnpm build` green.
