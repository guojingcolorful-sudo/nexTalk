---
phase: 02-real-cloud-pipeline-audio-core
fixed_at: 2026-10-09T03:46:24Z
review_path: .planning/phases/02-real-cloud-pipeline-audio-core/02-REVIEW.md
iteration: 1
findings_in_scope: 10
fixed: 10
skipped: 0
status: all_fixed
---

# Phase 2: Code Review Fix Report

**Fixed at:** 2026-10-09T03:46:24Z
**Source review:** `.planning/phases/02-real-cloud-pipeline-audio-core/02-REVIEW.md` (status: `issues_found`)
**Iteration:** 1
**Scope:** Critical + Warning (`fix_scope: critical_warning`) — CR-01, WR-01 … WR-09. The three Info findings (IN-01, IN-02, IN-03) were deliberately out of scope.
**Isolation:** every commit was made on a dedicated git worktree (`/tmp/sv-02-reviewfix-YDgBVM`, branch `gsd-reviewfix/02-62927`) so the main checkout was never touched mid-run. The branch fast-forwards into `main` on cleanup.

**Summary:**
- Findings in scope: 10
- Fixed: 10
- Skipped: 0

**Verification performed (all green, in the worktree, on the fix branch):**

| Gate | Result |
|------|--------|
| `cargo test` (full: 218 lib + 9 integration binaries + doc-tests) | EXIT=0, 0 failed |
| `pnpm -r test` | 13 + 43 + 66 (desktop) + 57 (teleprompter) passed |
| `pnpm exec playwright test` | 36 passed, 4 skipped |
| `node tools/vendor-experiments/failure-cases/run.mjs --check` | 20 条案例全部合规 |
| `pnpm run build` | built (required before Playwright's preview server) |

Note on one transient Playwright failure: the first full run had `e2e/enrollment.spec.ts` fail once on `正在播放…` (the mock's 150 ms preview window racing the assertion). The spec passed on isolated re-run and on the full re-run; no fix commit touches `e2e/` or the enrollment page, so it is a pre-existing timing flake in the mock, not a regression.

## Fixed Issues

### CR-01: AEC3 receives partial frames / mirror failures were invisible

**Files modified:** `apps/desktop/src-tauri/src/audio/aec.rs`, `apps/desktop/src-tauri/src/audio/mod.rs`, `apps/desktop/src-tauri/src/audio/playout.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/tests/audio_chain.rs`, `apps/desktop/src-tauri/tests/session_integration.rs`, `apps/desktop/src-tauri/tests/stability.rs`
**Commit:** `aa09625`
**Applied fix:** the playout render path now mirrors **whole 480-sample (10 ms @ 48 kHz) frames** into the AEC reference, carrying the remainder across device ticks in a `MirrorFramer` instead of pushing per-tick slices (the old code fed AEC3 partial frames, which it rejects). `PlayoutStats.mirror_failures` counts what the processor refused and is surfaced as `LinkHealth.mirrorFailures`. Pinned by tests at three levels: the audio-chain integration test (frame discipline across ticks), `session_integration.rs` (the counter reaches the IPC payload), and the stability suite.
**Status: fixed: requires human verification** — the framer's carry-across-ticks state machine is exactly the class of logic a syntax/behavior check cannot fully confirm; the ordering of carry vs. flush on stop deserves a reviewer's eye.

### WR-01: `PlaybackFirstSample` positions survived a generation boundary

**Files modified:** `apps/desktop/src-tauri/src/audio/playout.rs`, `apps/desktop/src-tauri/tests/stability.rs`
**Commit:** `1fa90d6`
**Applied fix:** `new_generation` now clears `inner.first_positions`, so a position from an interrupted generation can no longer suppress (or mis-fire) the next generation's first-sample mark. Pinned by `playout_a_new_generation_inherits_no_positions_and_no_suppressed_mark` (stability.rs), which asserts two `PlaybackFirstSample` marks across a generation boundary — proven RED by reverting the clear.
**Status: fixed: requires human verification** — a stale-state bug; the test pins the visible symptom, but the invariant ("no mark state outlives its generation") is state handling a human should confirm.

### WR-02: `PlaybackFirstSample` was marked at enqueue, not at playback

**Files modified:** `apps/desktop/src-tauri/src/audio/mod.rs`, `apps/desktop/src-tauri/src/audio/playout.rs`, `apps/desktop/src-tauri/src/pipeline/budget.rs`, `apps/desktop/src-tauri/tests/stability.rs`
**Commit:** `d5ae357`
**Applied fix:** introduced `SegmentPosition { segment_id, position, marked }` recorded at enqueue; the mark now fires from `mark_first_played` when the first sample is actually **written to the device** (the `(rate, segment_id)` tuple is plumbed out of the render inner). `budget.rs`'s consumption-boundary docs and `DEFAULT_TARGET_MS` now agree with the code, and Test 3 asserts "an enqueued sentence that has not played is not marked".
**Status: fixed: requires human verification** — latency-budget semantics (when the clock starts) changed; needs a human read of the boundary definition.

### WR-03: Terminal stage failures could be dropped, and loss counters had no readers

**Files modified:** `apps/desktop/src-tauri/src/lib.rs`, `.../src/pipeline/stages/{deepgram,volc_tts,xfyun}.rs`, `.../src/state.rs`, `.../tests/session_integration.rs`, `apps/desktop/src/pages/DiagnosticsPage.tsx`, `apps/desktop/src/pages/DiagnosticsPage.test.tsx`
**Commit:** `1c4324a`
**Applied fix:** every terminal `SttEvent::Failed` / `TtsEvent::Failed` is now sent with a lossless `.send(...).await` (the `try_send` sites returned `Full` and dropped exactly the event the caller needed). `TraceHealth { dropped_records, write_failures }` is snapshotted at `close_trace_writer` (reset at open), surfaced as `LinkHealth.traceDroppedRecords / traceWriteFailures`, and rendered in a new Chinese-labelled 链路健康 panel with 无丢弃 / 有丢弃 states — the counters now have a reader on both the Rust and the UI side (5 frontend tests, including the IPC-shape guard).
**Status: fixed: requires human verification** — switching terminal paths to an awaited send is a concurrency-behavior change (a full queue now back-pressures the stage task instead of dropping the event) and should be confirmed deliberately, not by test outcome alone.

### WR-04: A 15 s fragment rotation dropped the carried text from the committed final

**Files modified:** `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs`, `apps/desktop/src-tauri/tests/mock_vendors.rs`
**Commit:** `7e8cd7b`
**Applied fix:** the final frame now assembles `carried_text + builder.text()` **before** clearing `carried_text`; the rotation-site comment documents the 15 s-fragment / 0.9 × cap invariant. The mock gained per-service-session scripting (`on_audio_by_session`), and `xfyun_client_commits_the_whole_fragment_across_a_rotation` asserts the committed final equals 前半段后半段 — proven RED by temporarily reintroducing the bug (test failed 0 passed / 1 failed, then restored green).
**Status: fixed: requires human verification** — ordering of clear-vs-assemble is a logic fix; the RED-proven test pins it, but the reviewer classification is a logic error.

### WR-05: Cost/quota were priced with a vendor the pipeline does not call

**Files modified:** `apps/desktop/src-tauri/src/trace/mod.rs`, `apps/desktop/src-tauri/src/trace/jsonl.rs`
**Commit:** `7d51fa0`
**Applied fix:** the flat Gemini constants were replaced by an id-keyed table — `ACTIVE_TRANSLATOR_ID` (= the shipped `deepseek::DEFAULT_MODEL`, `deepseek-chat`), `DEEPSEEK_CHAT_RATES` (0.30 / 1.20 per MTok, peak, fetched 2026-10-08), `GEMINI_FLASH_LITE_ID` / `GEMINI_FLASH_LITE_RATES` kept as the documented alternative, `translator_rates(id)` as the lookup. `StageUsage` gained `translatorId` (`Option<String>`, serde-defaulted, `Copy` dropped with the four literals updated) so the token counts and the vendor that billed them travel in one tuple. `cost_comes_from_the_named_rate_table` re-derives through `translator_rates(ACTIVE_TRANSLATOR_ID)`, and the new `translator_rates_are_keyed_by_the_shipped_translator_id` builds the real `DeepseekTranslator` and asserts its `model_version()` lands on its own row — proven RED by keying `ACTIVE_TRANSLATOR_ID` to the Gemini row.
**Status: fixed: requires human verification** — pricing semantics; the numbers themselves should be confirmed against the vendor page by a human.

### WR-06: The 火山 TTS resource lock aborted the sentence when poisoned

**Files modified:** `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs`
**Commit:** `751ae99`
**Applied fix:** both `set_last_resource` and `model_version` go through one `last_resource_guard()` using `unwrap_or_else(|poison| poison.into_inner())` — the same recovery the playout queue (`playout.rs:626-633`) and the AEC (`aec.rs:292-298`) already use; the misleading `expect("the resource lock is never poisoned")` is gone from both sites, and the module that documented the panic class no longer contradicts it. New test `a_poisoned_resource_lock_does_not_silence_the_sentence` poisons the lock through a genuinely panicking holder (`catch_unwind`) and asserts both readers survive — proven RED at the old `expect` line (`volc_tts.rs:354`). No sibling sites were touched: the identical `deepseek.rs:334/450` pattern is on the translation path and was outside this finding's cited files (recorded here rather than silently expanded).
**Status: fixed**

### WR-07: Enrollment biometric artifacts were world-readable between create and chmod

**Files modified:** `apps/desktop/src-tauri/src/enroll/mod.rs`, `apps/desktop/src-tauri/src/enroll/capture.rs`, `apps/desktop/src-tauri/src/enroll/voice_store.rs`
**Commit:** `2db0cfa`
**Applied fix:** new `enroll::create_private` (mirroring the trace writer's `open_private`) opens the file with `OpenOptions::… .mode(0o600)` and repairs an existing file's mode immediately after the open — **before the caller writes a byte** — closing the CWE-377 window on both writers. `save_wav` now hands that handle to `hound::WavWriter::new(BufWriter::new(file), spec)` (hound's own file-creating constructor is gone from the module), and `VoiceStore::save` uses the same open instead of `File::create` + trailing `set_permissions`. Because the window is invisible to a final-mode assertion, the mechanism is pinned by a source-scan test (`enrollment_artifacts_are_created_private_not_chmod_ed_afterwards`, proven RED when the racy constructor is restored) plus a behavioral test for the pre-existing-permissive-file path. The existing final-mode assertions in `tests/enrollment_capture.rs` and `tests/enrollment_train.rs` stay green.
**Status: fixed**

### WR-08: The Deepgram interviewer line could not be constructed from dispatch

**Files modified:** `apps/desktop/src-tauri/src/pipeline/stages/mod.rs`
**Commit:** `f3df2f5`
**Applied fix:** added the `VendorStt::Deepgram(DeepgramStt)` variant, its `From<DeepgramStt>` impl, all three `SttSource` arms (`provider` / `model_version` / `set_marks` **and** `start` — the enum had four), and the `pub use deepgram::DeepgramStt` re-export; the "T2.3 adds…" TODO comment is gone, so the module table's claim that the interviewer line is shipped is now true. The dispatch test builds a real `DeepgramStt` from `DeepgramCredentials` and asserts `provider() == "deepgram"`, `model_version() == "nova-3"` and the variant — proven RED first (the test did not compile: `cannot find type DeepgramStt` / `no variant named Deepgram`).
**Status: fixed**

### WR-09: The realtime-callback contract was contradicted by the callback bodies

**Files modified:** `apps/desktop/src-tauri/src/audio/bounded.rs`, `apps/desktop/src-tauri/src/audio/capture.rs`
**Commit:** `03a2d9e`
**Applied fix:** two-part change matching the reviewer's two options:
1. **Move instead of copy.** `CaptureSink::push_owned(Vec<f32>)` moves a buffer the callback had to build anyway (multi-channel downmix, i16 → f32 conversion — both call sites now use it); `push(&[f32])` delegates and is documented as the one permitted copy. `bounded.rs`'s absolute "Nothing else" claim is rewritten into a measured statement: the accepted allocation is named explicitly, the zero-allocation alternative (a buffer-recycle pool with a return channel) is named as the deliberate next step if a measured overrun ever implicates the allocator — instead of a contract the code contradicts.
2. **The error path obeys the same discipline.** The device error callback replaced `Mutex::lock()` + `VecDeque::push_back` (blocking + growth allocation on the audio thread) with a bounded `SyncSender` + `try_send`; a full or closed queue counts a drop in an atomic. `drain_reported_errors` drains the channel and appends one Chinese-labelled drop report per counted loss, clearing the count as it reports it — so the counter has a reader and no stale count survives into a later session. Both halves are unit-tested without a device (`a_full_error_channel_counts_drops_and_never_blocks`, `drain_errors_reports_and_clears_the_dropped_reports`, `push_owned_shares_the_queue_and_the_drop_counter`), proven RED by compiling the tests before the helpers existed.
**Status: fixed: requires human verification** — the contract now matches the code, but the residual allocation on the borrowed-slice path is a deliberate trade-off a human should accept or order the recycle pool for.

## Skipped Issues

None — all 10 in-scope findings were fixed. `IN-01`, `IN-02` and `IN-03` were out of scope by configuration (`fix_scope: critical_warning`) and remain open for a later pass.

## Notes for the reviewer

- **Deliberately not expanded:** `deepseek.rs:334/450` uses the same poisoned-lock `expect` pattern as WR-06 but is on the translation path and was not cited; it is recorded rather than fixed here to keep each commit scoped to its finding.
- **Deferred by the finding itself:** WR-09's full buffer-recycle pool (zero allocation on the borrowed-slice path) is documented in `bounded.rs` as the next step if measurement ever implicates the allocator.
- **Trace records now carry `translatorId`** — a wire-shape addition on the JSONL record only (`#[serde(default)]`, camelCase), no change to the phone protocol (`@nextalk/protocol` untouched); old lines stay readable.

---

_Fixed: 2026-10-09T03:46:24Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
