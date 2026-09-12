---
phase: 01-foundation-simulation-mode
fixed_at: 2026-09-12T05:18:25Z
review_path: .planning/phases/01-foundation-simulation-mode/01-REVIEW.md
iteration: 1
findings_in_scope: 9
fixed: 9
skipped: 0
status: all_fixed
---

# Phase 1: Code Review Fix Report

**Fixed at:** 2026-09-12T05:18:25Z
**Source review:** `.planning/phases/01-foundation-simulation-mode/01-REVIEW.md`
**Iteration:** 1
**Scope:** Critical + Warning (`CR-01`, `WR-01`..`WR-08`). Info findings `IN-01`..`IN-09` were left for the developer by instruction.

**Summary:**
- Findings in scope: 9
- Fixed: 9
- Skipped: 0
- Of which flagged for human confirmation: 2 (`CR-01`, `WR-04` — logic changes that syntax/tests cannot fully prove)

Two findings carry the `fixed: requires human verification` status: their fixes change control flow
(session-identity resynchronisation and a lock-scoped epoch re-check), so a passing test suite
demonstrates the intended path but not that every adjacent path is correct.

**Commits** (branch `gsd-reviewfix/01-40880`, base `49c240f`); each finding is one atomic pair of
RED test commit + GREEN fix commit unless noted:

| Finding | RED | GREEN |
|---|---|---|
| CR-01 | `05ec4c7` | `d574ea7` |
| WR-01 | `1884039` | `8cb0497` |
| WR-02 | `05ec4c7` | `d574ea7` |
| WR-03 | `05ec4c7` | `d574ea7` (transport) + `500c819` (page/tests) |
| WR-04 | `3102b27` | `88027c5` |
| WR-05 | `0e26db4` | `c580ae5` |
| WR-06 | — (behaviour-preserving) | `fd6c09b` |
| WR-07 | — (tooling, verified by run) | `3a918df` |
| WR-08 | `491abf5` | `9e77b1d` |

## Fixed Issues

### CR-01: A second session renders nothing on the phone — the resume cursor is never reset across session restarts

**Status:** fixed: requires human verification
**Files modified:** `packages/protocol/src/index.ts`, `packages/protocol/src/index.test.ts`,
`apps/desktop/src-tauri/src/state.rs`, `apps/desktop/src-tauri/src/lan/server.rs`,
`apps/desktop/src-tauri/src/sim/source_test.rs`,
`apps/desktop/src-tauri/tests/session_integration.rs`,
`apps/teleprompter/src/hooks/useWs.ts`, `apps/teleprompter/src/hooks/useWs.test.tsx`,
`e2e/teleprompter.spec.ts`
**Commits:** `05ec4c7` (RED), `d574ea7` (GREEN)
**Applied fix:** Carried the already-existing `session_epoch` to the wire and made every client
resynchronise on it.

- `packages/protocol`: new `SessionEvent` union member `{ t: 'session_started'; epoch: number }`;
  `isServerEvent` narrows it with `typeof x.epoch === 'number' && Number.isFinite(x.epoch)`.
- `state.rs`: `start_session` publishes the `session_started` marker **before** any content;
  `resume_events(since_seq, since_epoch)` replays the whole timeline when the client's epoch is
  stale; `replay_after_subtitle_seq` treats a cursor above every subtitle seq as stale (defence in
  depth for a client that sends no epoch).
- `lan/server.rs`: the Rust enum mirror, plus `#[serde(default)] since_epoch: Option<u64>` on the
  `resume` client frame — an older client that omits it keeps the previous seq-only behaviour.
- `useWs.ts`: the marker clears `seenSeqRef`, `seenStrategyIdsRef`, sets the new epoch, and
  **replaces** the rendered stream instead of appending to it.
- Regression coverage: protocol unit test; `state.rs` unit tests
  (`start_session_publishes_the_session_marker_first`,
  `a_cursor_above_this_session_replays_it_from_the_start`,
  `resume_replays_the_whole_new_session_when_the_epoch_moved`,
  `resume_inside_one_session_still_replays_only_the_tail`); the integration test
  `a_restarted_session_announces_itself_and_recovers_a_stale_phone`; the Playwright test
  `a restarted session clears the phone and streams the new one (CR-01)` (stop → restart → the
  phone receives the new session's events).

**Residual gap (documented, not fixed):** a client that has gone offline and missed the
`session_started` marker *and* sends no `sinceEpoch` still relies on the cursor-above-all-subtitles
heuristic. That heuristic covers the restart case for the shipped phone client (which always sends
the epoch after this fix); a third-party client without the epoch field degrades to at worst one
stale replay.

**Requires human verification:** the slicing change in `replay_after_subtitle_seq` (the
`since_seq > highest_seq` branch) is a control-flow change that unit tests exercise only on the
paths they construct.

---

### WR-01: A lagged broadcast receiver silently kills the phone's event fan-out

**Files modified:** `apps/desktop/src-tauri/src/lan/server.rs`
**Commits:** `1884039` (RED), `8cb0497` (GREEN)
**Applied fix:** Extracted the per-socket fan-out into `forward_events(rx, tx)` and made the
`recv()` loop explicit: `Ok(event)` forwards, `Err(Lagged(skipped))` logs the drop count and
**continues** (the receiver is still usable and resumes at the oldest retained event), `Err(Closed)`
breaks. Previously any `Lagged` fell into the catch-all and terminated the task, leaving the phone
connected but permanently mute.
**Test:** `a_lagged_receiver_keeps_forwarding` — pushes 80 events through the 64-slot ring buffer,
then asserts a later event still reaches the socket.

---

### WR-02: Stopping or restarting a session never clears the desktop webviews' event state

**Files modified:** `apps/desktop/src/hooks/useTauriEvents.ts`,
`apps/desktop/src/hooks/useTauriEvents.test.ts`
**Commits:** `05ec4c7` (RED), `d574ea7` (GREEN)
**Applied fix:** In the `session` listener, the `session_started` marker is handled **before**
`narrowSession` and before the `batch.length === 0` early return, clearing `events` and
`languageMode`. This is what makes the locked confirmation copy 当前字幕与策略将清空 literally true —
without it a restarted session stacked under the previous one and the new session's `s-r1` strategy
card was deduped away.
**Test:** `clears the stream and the applied mode when a new session announces itself`.

---

### WR-03: Phone language mode can silently diverge from the desktop

**Files modified:** `apps/teleprompter/src/hooks/useWs.ts`,
`apps/teleprompter/src/hooks/useWs.test.tsx`, `apps/teleprompter/src/pages/TeleprompterPage.tsx`,
`apps/teleprompter/src/pages/TeleprompterPage.test.tsx`, `e2e/teleprompter.spec.ts`
**Commits:** `d574ea7` (transport half), `500c819` (page half, tests, e2e)
**Applied fix:** Three parts.
1. `sendLanguagePref` records `languageRef.current = pref` **first** and returns early when the
   socket is not open, so a tap made while the phone is reconnecting is remembered rather than lost.
2. `ws.onopen` re-asserts the remembered mode (`{t: 'control', language: languageRef.current}`)
   after the `resume` frame, so a reconnect cannot leave the desktop on a stale mode.
3. `TeleprompterPage` derives `echoedLanguage` from the last `language` ServerEvent and seeds
   `languagePref` from it, making Rust's echo the source of truth instead of local optimism.

**Test:** `a tap while the socket is down is queued and re-sent on the next open`,
`re-asserts the last chosen mode on every reopen`,
`adopts the echoed language event as the source of truth`; the Playwright frame-index assertions
were updated (frame 0 = `resume`, frame 1 = `control`, taps follow).

**Note:** the transport half landed in the `d574ea7` commit rather than a WR-03-only commit — the
`useWs.ts` hunks for CR-01 and WR-03 are interleaved in the same functions and were not separable
without a broken intermediate commit.

---

### WR-04: `interrupt_session` / `repeat_session` skip the epoch re-check under the engine lock

**Status:** fixed: requires human verification
**Files modified:** `apps/desktop/src-tauri/src/state.rs`
**Commits:** `3102b27` (RED), `88027c5` (GREEN)
**Applied fix:** Introduced private `interrupt_session_for(epoch)` / `repeat_session_for(epoch)`
seams mirroring the existing `advance_sim_for`. Each takes the engine lock and **re-checks
`self.session_epoch() != epoch` while holding it**, returning
`"the session ended before the command landed"` instead of mutating the engine that the new session
just swapped in. A shared `guard_generating(command)` replaces the duplicated status guard.
**Test:** `a_stale_interrupt_or_repeat_never_mutates_the_new_session` — bumps the epoch, then asserts
both stale calls are `Err`, the status stays `Generating`, and the timeline is unchanged.

**Requires human verification:** the lock-order invariant (engine → state) is the substance of the
fix; the test proves the guard fires but cannot prove the lock order holds on every call path.

---

### WR-05: Voice enrollment keeps the microphone live after the user navigates back

**Files modified:** `apps/desktop/src/pages/VoiceEnrollmentPage.tsx`,
`apps/desktop/src/pages/VoiceEnrollmentPage.test.tsx` (new file)
**Commits:** `0e26db4` (RED), `c580ea5` (GREEN)
**Applied fix:** New `cancelRecording()` — stops every track on the stored `MediaStream`, clears the
ref, ends the recording state, and resets `remainingRef`/`remaining` to `MAX_SECONDS` — wired into a
new `handlePrev` that also decrements the step. 上一步 during the countdown now releases the device
instead of leaving the mic (and the running timer) alive behind the user.
**Test:** `上一步 during the countdown releases the microphone and stops the timer` (asserts every
track received `stop()` and the countdown stops advancing), plus
`the countdown still finishes the take when the user stays` as the guard against over-cancelling.

---

### WR-06: `Array.prototype.at` is used in shipped desktop UI but is outside the declared macOS 12.0 floor and the polyfill set

**Files modified:** `apps/desktop/src/pages/DualPanePage.tsx`
**Commit:** `fd6c09b`
**Applied fix:** Replaced `subtitles.at(-1)` with an explicit
`subtitles.length > 0 ? subtitles[subtitles.length - 1].id : null` and left a comment recording the
Safari 15.4+ origin so it does not come back. No behaviour change, so no new test — the existing
`dual pane extended view` suite covers the scroll-follow behaviour that reads this value.
**Note:** the fix is at the call site rather than by raising `minimumSystemVersion`, per the phase's
macOS 12.7 floor.

---

### WR-07: The RTT tool prints and persists the full endpoint URL, contradicting its own credential policy

**Files modified:** `tools/vendor-experiments/rtt/measure.mjs`
**Commit:** `3a918df`
**Applied fix:** New `redactUrl(href)` returns `origin + pathname` with every query **value**
replaced by `<redacted>` (and `<unparseable url>` for a URL that will not parse), applied to both
sinks: the `endpoint:` line on stdout and `config.url` in the written JSON report. Several target
vendors accept the key in the query string, and the report is designed to be pasted into a
transcript or committed.
**Verified by run:** with `?key=SUPERSECRET123&model=...`, stdout and the report both show
`?key=<redacted>&model=<redacted>` and the secret appears zero times in either output.

---

### WR-08: Once the extended view window is closed, 扩展视图 can never bring it back

**Files modified:** `apps/desktop/src/pages/ConsolePage.tsx`, `e2e/demo.spec.ts`
**Commits:** `491abf5` (RED), `9e77b1d` (GREEN)
**Applied fix:** `openDualPane` no longer swallows the null case behind `await dual?.show()`. When
`WebviewWindow.getByLabel('dual')` returns null (the dual window's own 关闭 destroys it), the window
is recreated with `new WebviewWindow('dual', …)` using the options mirrored from `tauri.conf.json`,
and the handler awaits `tauri://created` / rejects on `tauri://error`. A failure sets `dualFailed`,
which renders the existing `ErrorBanner` with title 扩展视图打开失败, body 请重试, and a 重试 action
wired back to `openDualPane` — the previously silent no-op is now visible and retryable.
**Test:** `recreates the extended view after its window was closed (WR-08)` and
`surfaces a failed extended-view open instead of swallowing it (WR-08)`.

---

## Verification battery

All four gates were run against the worktree at `9e77b1d` (the final fix commit) and are green.

| Gate | Command | Result |
|---|---|---|
| JS/TS unit | `pnpm -r test` | **pass** — 82 tests in 4 packages (`protocol` 23, `design-tokens` 13, `desktop` 14, `teleprompter` 32) |
| Rust | `cargo test --workspace --manifest-path apps/desktop/src-tauri/Cargo.toml` | **pass** — 33 lib + 3 integration, exit 0, 0 failed |
| E2E | `pnpm exec playwright test` | **pass** — 32 tests across the `desktop` and `teleprompter` projects |
| Build | `pnpm run build` | **pass** — exit 0 (`dist/assets/index-*.js` 409.35 kB, gzip 129.65 kB) |

Test counts include the new coverage added by this batch: 5 new Rust unit tests, 1 new Rust
integration test, 1 new protocol unit test, 5 new desktop/teleprompter unit tests, 1 new file
(`apps/desktop/src/pages/VoiceEnrollmentPage.test.tsx`), and 4 new Playwright tests.

**Not in scope:** the `Info` findings `IN-01`..`IN-09` were deliberately not fixed (per the task
scope) and remain open in `01-REVIEW.md`. No `.planning` file other than this report was touched.

---

_Fixed: 2026-09-12T05:18:25Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
