---
phase: 02-real-cloud-pipeline-audio-core
reviewed: 2026-10-06T07:00:21Z
depth: standard
files_reviewed: 85
files_reviewed_list:
  - apps/desktop/src-tauri/src/audio/aec.rs
  - apps/desktop/src-tauri/src/audio/bounded.rs
  - apps/desktop/src-tauri/src/audio/capture.rs
  - apps/desktop/src-tauri/src/audio/device.rs
  - apps/desktop/src-tauri/src/audio/mod.rs
  - apps/desktop/src-tauri/src/audio/playout.rs
  - apps/desktop/src-tauri/src/audio/resample.rs
  - apps/desktop/src-tauri/src/audio/routing.rs
  - apps/desktop/src-tauri/src/enroll/capture.rs
  - apps/desktop/src-tauri/src/enroll/mod.rs
  - apps/desktop/src-tauri/src/enroll/register.rs
  - apps/desktop/src-tauri/src/enroll/voice_store.rs
  - apps/desktop/src-tauri/src/lan/server.rs
  - apps/desktop/src-tauri/src/lib.rs
  - apps/desktop/src-tauri/src/pipeline/breaker.rs
  - apps/desktop/src-tauri/src/pipeline/budget.rs
  - apps/desktop/src-tauri/src/pipeline/budget_test.rs
  - apps/desktop/src-tauri/src/pipeline/cascade.rs
  - apps/desktop/src-tauri/src/pipeline/confidence.rs
  - apps/desktop/src-tauri/src/pipeline/mod.rs
  - apps/desktop/src-tauri/src/pipeline/segment.rs
  - apps/desktop/src-tauri/src/pipeline/stages/config.rs
  - apps/desktop/src-tauri/src/pipeline/stages/deepgram.rs
  - apps/desktop/src-tauri/src/pipeline/stages/deepseek.rs
  - apps/desktop/src-tauri/src/pipeline/stages/error.rs
  - apps/desktop/src-tauri/src/pipeline/stages/mod.rs
  - apps/desktop/src-tauri/src/pipeline/stages/traits.rs
  - apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs
  - apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs
  - apps/desktop/src-tauri/src/pipeline/vad.rs
  - apps/desktop/src-tauri/src/pipeline/validate.rs
  - apps/desktop/src-tauri/src/sim/source.rs
  - apps/desktop/src-tauri/src/sim/source_test.rs
  - apps/desktop/src-tauri/src/state.rs
  - apps/desktop/src-tauri/src/trace/jsonl.rs
  - apps/desktop/src-tauri/src/trace/mod.rs
  - apps/desktop/src/App.tsx
  - apps/desktop/src/components/AiTimeline.tsx
  - apps/desktop/src/components/ChatBubble.test.tsx
  - apps/desktop/src/components/ChatBubble.tsx
  - apps/desktop/src/components/LatencyWaterfall.test.tsx
  - apps/desktop/src/components/LatencyWaterfall.tsx
  - apps/desktop/src/components/UsageMinutesPanel.test.tsx
  - apps/desktop/src/components/UsageMinutesPanel.tsx
  - apps/desktop/src/hooks/useLatencyWaterfall.ts
  - apps/desktop/src/pages/DiagnosticsPage.test.tsx
  - apps/desktop/src/pages/DiagnosticsPage.tsx
  - apps/desktop/src/pages/DualPanePage.test.tsx
  - apps/desktop/src/pages/DualPanePage.tsx
  - apps/desktop/src/pages/SetupWizardPage.tsx
  - apps/desktop/src/pages/VoiceEnrollmentPage.test.tsx
  - apps/desktop/src/pages/VoiceEnrollmentPage.tsx
  - apps/teleprompter/src/components/ChatBubble.test.tsx
  - apps/teleprompter/src/components/ChatBubble.tsx
  - apps/teleprompter/src/pages/TeleprompterPage.test.tsx
  - apps/teleprompter/src/pages/TeleprompterPage.tsx
  - e2e/abstention.spec.ts
  - e2e/degraded.spec.ts
  - e2e/desktop.spec.ts
  - e2e/enrollment.spec.ts
  - packages/protocol/src/index.test.ts
  - packages/protocol/src/index.ts
  - tools/vendor-experiments/blind-clone-results.json
  - tools/vendor-experiments/cross-lingual-clone-probe.mjs
  - tools/vendor-experiments/failure-cases/0001-sse-split-token-loss.json
  - tools/vendor-experiments/failure-cases/0002-homophone-term-drift.json
  - tools/vendor-experiments/failure-cases/0003-numeric-drift-wrong-digits.json
  - tools/vendor-experiments/failure-cases/0004-partial-committed-as-final.json
  - tools/vendor-experiments/failure-cases/0005-interrupt-overlap-old-audio.json
  - tools/vendor-experiments/failure-cases/0006-deepgram-silent-timeout-net0001.json
  - tools/vendor-experiments/failure-cases/0007-xfyun-10163-oversized-frame.json
  - tools/vendor-experiments/failure-cases/0008-xfyun-wpgs-rpl-overreplacement.json
  - tools/vendor-experiments/failure-cases/0009-translate-malformed-json-event.json
  - tools/vendor-experiments/failure-cases/0010-volc-tts-error-frame-ignored.json
  - tools/vendor-experiments/failure-cases/0011-retry-during-open-breaker.json
  - tools/vendor-experiments/failure-cases/0012-reserved-field-as-vendor-confidence.json
  - tools/vendor-experiments/failure-cases/0013-low-confidence-false-abstention.json
  - tools/vendor-experiments/failure-cases/0014-numeric-check-first-only.json
  - tools/vendor-experiments/failure-cases/0015-degraded-form-not-rendered.json
  - tools/vendor-experiments/failure-cases/0016-trace-queue-unbounded.json
  - tools/vendor-experiments/failure-cases/0017-clone-handshake-rejected-placeholder.json
  - tools/vendor-experiments/failure-cases/0018-empty-term-hits-false-low-confidence.json
  - tools/vendor-experiments/failure-cases/0019-retry-budget-exhausted-blocks-segment.json
  - tools/vendor-experiments/failure-cases/0020-vad-transient-false-sentence-close.json
  - tools/vendor-experiments/failure-cases/run.mjs
findings:
  critical: 1
  warning: 9
  info: 3
  total: 13
status: issues_found
---

# Phase 2: Code Review Report

**Reviewed:** 2026-10-06T07:00:21Z
**Depth:** standard
**Files Reviewed:** 85
**Status:** issues_found

## Summary

Phase 2 delivers the real cloud pipeline (讯飞 iat, Deepgram nova-3, DeepSeek streaming translate, 火山 Seed-ICL-2.0 clone TTS) plus the CoreAudio audio core (capture → AEC3 → resample → playout with jitter buffer, epoch guard, barge-in, device rebuild). The vendor clients are the strongest part of the submission: error classification is centralised and sanitised (`stages/error.rs`), credentials come from env only and never reach a message string, `parse_frame` is bounds-checked, the trace JSONL is created 0600, the enrollment sample path is contained and capped, and the LAN pairing-token check is unchanged from Phase 1. No hardcoded secrets, no `eval`/`innerHTML`, no debug artifacts in application code.

The defects cluster in the audio boundary, which is also where the phase's headline claim lives. The AEC render reference — the "what the speaker plays" mirror — is fed in arbitrary-length slices while the processor accepts only exactly 480-sample frames, so on a device with CoreAudio's usual buffer size the canceller receives no usable reference at all and the failure is counted into a counter nothing reads (CR-01). The playout generation reset clears the queue but not the write cursor or the per-segment position table, so a restarted session can inherit the previous session's positions and lose its first latency mark (WR-01). Latency marks fire at enqueue rather than at device consumption, which understates the end-to-end number against the ≤2 s budget the rig exists to police (WR-02). Failure reporting through `try_send` silently drops failures, and the counters introduced to make loss visible have no consumers (WR-03). Two vendor-specific issues (讯飞 carried-text loss, 火山 poison panic) and one documentation/economic mismatch (Gemini rates for a DeepSeek pipeline) round out the warnings.

One structural caveat, consistent with the phase's documented deferral: none of this runs from a production command yet — `start_session` still runs the simulator (`lib.rs`), the device layer builds no-op streams (`audio/device.rs`), and `SttStream::end_fragment` has no callers. Findings marked *latent* below are unreachable today but will become live the moment Phase 3 wires the assembly; they are worth fixing before that wiring lands because several are exactly the seams the wiring will press on.

## Critical Issues

### CR-01: The AEC render mirror feeds the canceller slices it always refuses (only exactly 480 samples are accepted)

**Files:** `apps/desktop/src-tauri/src/audio/playout.rs:432`, `apps/desktop/src-tauri/src/audio/aec.rs:200-206`, `apps/desktop/src-tauri/src/audio/aec.rs:226-233`, `apps/desktop/src-tauri/src/audio/aec.rs:314-321`, `apps/desktop/src-tauri/src/audio/playout.rs:816-820`, `apps/desktop/src-tauri/tests/audio_chain.rs:1226-1250`

**Issue:** `render_inner` mirrors the far-end reference one *take* at a time:

```rust
let take = remaining.min(out.len() - written);   // bounded by the chunk AND the device tick
...
reference.push_reference(&out[written..written + take], rate);   // playout.rs:432
```

`take` is whatever is left of the current queued chunk, clipped by whatever is left of the device's buffer. `SharedProcessor::push_reference` forwards that slice to `process_render_frame`, which calls `check_length` and accepts **only** `frame_samples() == FRAME_SAMPLES == 480` (48 kHz / 10 ms). Anything else returns `AecError::BadFrameLength` and the slice is discarded; the only trace is `mirror_failures.fetch_add(1)` (`aec.rs:314-321`).

Consequences, all reachable on real hardware:

1. CoreAudio's usual device buffer is 512 frames — the codebase says so itself at `resample.rs:95-96` ("often 512 frames, which is not a multiple of the 480-sample AEC frame"). With a 512-frame tick, the first take is 480 (accepted) and the next is 32 (refused), so the far-end reference is permanently gappy and misaligned.
2. A chunk whose remainder is not a multiple of 480 (a resampler flush tail, a vendor frame boundary, the 240-sample interrupt fade the test itself asserts on) produces a final take the canceller refuses.
3. `capture.rs:486-492` masks the follow-on symptom: when the capture frame arrives with no render pending, `process_with_aec` injects `vec![0.0f32; frame.len()]` as the reference and retries. Echo cancellation then runs against silence, so the user's own cloned voice is not cancelled and is transcribed as if the user had spoken it.
4. Nothing surfaces any of this: `mirror_failures()` has **no consumers outside `aec.rs`** (grep across `src/` and `tests/`), and the doc comment at `playout.rs:816-820` claims "the mirror receives exactly the written prefix, so a resample mismatch between what is heard and what the canceller knows about is impossible by construction" — true about the *rate*, silent about the *frame geometry*, which is the part the processor rejects.
5. The test that exists to pin this (`playout_the_echo_canceller_hears_exactly_what_the_speaker_plays`) ticks with a 480-sample buffer and asserts concatenation plus `blocks.len() > 1`; it never asserts a block is 480 long, and its `MirrorLog` (`tests/audio_chain.rs:957-985`) records whatever it is handed. It passes regardless of block size.

**Fix:** mirror in whole AEC frames, carrying the remainder across calls exactly like `StreamingResampler` does for the resampler. Minimal shape:

```rust
// in PlayoutChain: a small carry-over for the mirror
fn mirror_frames(&self, out: &[f32], written: usize, reference: &mut dyn RenderReference) {
    const FRAME: usize = crate::audio::aec::FRAME_SAMPLES;
    let mut tail = self.mirror_carry.lock().unwrap_or_else(|p| p.into_inner());
    tail.extend_from_slice(&out[..written]);
    while tail.len() >= FRAME {
        let frame: Vec<f32> = tail.drain(..FRAME).collect();
        reference.push_reference(&frame, crate::audio::capture::GRAPH_RATE_HZ);
    }
}
```

`render_inner` should then mirror `&out[..written]` once per tick instead of per take (or keep the queue borrow and hand the carry-over slices). Additionally: surface `mirror_failures()` on the diagnostics payload, and pin the contract in the test:

```rust
assert!(
    mirror.blocks.iter().all(|(samples, _)| samples.len() == 480),
    "every mirrored block is one AEC frame"
);
```

## Warnings

### WR-01: A playout generation reset does not clear the write cursor or the segment position table

**Files:** `apps/desktop/src-tauri/src/audio/playout.rs:453-459`, `apps/desktop/src-tauri/src/audio/playout.rs:266-276`, `apps/desktop/src-tauri/src/audio/playout.rs:226-233`, `apps/desktop/src-tauri/src/pipeline/segment.rs:107/132/201`, `apps/desktop/src-tauri/src/state.rs:350` and `:373`

**Issue:** `PlayoutQueue::new_generation` clears `queued` and `buffered_ms` only:

```rust
fn new_generation(&self) -> u64 {
    let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let mut inner = self.lock();
    inner.queued.clear();
    inner.buffered_ms = 0;
    epoch
}
```

`cursor`, `first_positions` and `played` survive — while the chain's own doc says of a session stop "keep nothing for the next session to inherit" (`playout.rs:941-942`). The chain is a long-lived object (`state.rs:350` `begin_session`, `state.rs:373` `end_session`), and segment ids are per-`Segmenter` counters starting at 0 → 1 (`segment.rs:132/201`), `Segmenter::new()` per `Cascade`. The moment a session is wired with a fresh cascade (the natural Phase 3 shape), the new session's segment 1 re-uses an id already in `first_positions`:

- the `Stage::PlaybackFirstSample` mark is **silently suppressed** by the dedup guard `if !inner.first_positions.iter().any(|(id, _)| *id == segment_id)` (`playout.rs:269-274`), so the first segment of every restarted session produces no latency waterfall (and `validate_marks` will then report a missing stage);
- `first_sample_position(segment_id)` returns a position from the previous session, which is what the no-overlap invariant compares against (`playout.rs:211-233`);
- `first_positions` also grows one entry per segment, forever.

The tests only drive one generation per queue, so none of this is pinned.

**Fix:** clear the generation-scoped position table (and decide explicitly whether `cursor` should restart, since `rendered_samples()` is the no-overlap comparison baseline):

```rust
fn new_generation(&self) -> u64 {
    let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let mut inner = self.lock();
    inner.queued.clear();
    inner.buffered_ms = 0;
    inner.first_positions.clear();   // positions belong to the generation
    epoch
}
```

Add a stability test that pushes in generation 1, calls `end_session()`, pushes the same `segment_id` in generation 2, and asserts the mark fires and `first_sample_position` reflects the new timeline.

### WR-02: `PlaybackFirstSample` is marked at enqueue, but the budget documents it as device consumption

**Files:** `apps/desktop/src-tauri/src/pipeline/budget.rs:84-86`, `apps/desktop/src-tauri/src/audio/mod.rs:49-52`, `apps/desktop/src-tauri/src/audio/playout.rs:264-276`, `apps/desktop/src-tauri/src/audio/playout.rs:501`

**Issue:** `Stage::PlaybackFirstSample` is documented as "The output device consumed the first PCM frame (the stopwatch end)" (`budget.rs:85`), and the `PlayoutSink` contract says the sink marks it "when a segment's first sample enters the playout timeline (02-01 contract)" (`audio/mod.rs:50-51`). The implementation marks it inside `PlayoutQueue::push`, i.e. at enqueue:

```rust
let position = inner.cursor;
if !inner.first_positions.iter().any(|(id, _)| *id == segment_id) {
    inner.first_positions.push((segment_id, position));
    inner.marks.mark(Stage::PlaybackFirstSample);   // playout.rs:275 — before a single sample is heard
}
```

With the default jitter policy (`pre_roll: true`, `target_ms = DEFAULT_TARGET_MS = 120`, `playout.rs:501`) plus whatever the device already buffers, the produced end-to-end number excludes up to ~120 ms of the latency the user actually experiences. The rig's entire purpose is to police the ≤2 s budget (AUDI-04), so a systematic understatement of the last stage is a correctness problem in the measurement, not a naming preference. The `MirrorLog` test asserts the mirror is empty before the first tick ("a reference from the future would make the canceller converge on a lie") — the same argument applies to the mark.

**Fix:** mark on the first sample actually written to the device: pass `marks` into `render_inner`/`tick_inner` and fire when `written` first becomes > 0 for a segment whose position has been recorded, or keep both numbers (add `PlaybackEnqueued` for the queue-side number and reserve `PlaybackFirstSample` for consumption so the waterfall can show the buffer cost explicitly). Whichever is chosen, make `budget.rs` and `audio/mod.rs` agree on the definition.

### WR-03: A failure reported on a full event queue disappears, and the counters meant to make loss visible have no consumers

**Files:** `apps/desktop/src-tauri/src/pipeline/stages/deepgram.rs:444-446`, `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs:410-415`, `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs:518-524`, `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs:630-633`, `apps/desktop/src-tauri/src/pipeline/stages/traits.rs:239-252`, `apps/desktop/src-tauri/src/trace/jsonl.rs:411-417`, `apps/desktop/src-tauri/src/audio/aec.rs:297-300`

**Issue:** All three stage sessions report a classified failure with a best-effort send and drop the result:

```rust
fn fail(events: &mpsc::Sender<SttEvent>, error: StageError) {
    let _ = events.try_send(SttEvent::Failed(error));   // xfyun.rs:632, same shape in deepgram.rs:445, volc_tts.rs:413
}
```

If the bounded event channel happens to be full, the failure is lost. The consumer side then sees what looks like a clean end — `next_partial` returns `None` on a closed channel (`traits.rs:239-247`), the documented "session is over (cleanly or not)" — so D-09 retry/breaker classification, which is supposed to key on `Failed`, silently degrades to "the vendor stopped talking". `volc_tts.rs:518-524` has the same pattern for status events.

Compounding it, every counter this phase introduced to make such loss visible is unread: `TraceWriter::dropped_records` (`jsonl.rs:411`) and `write_failures` (`jsonl.rs:416`) have no callers outside `jsonl.rs` (only their own unit test), and `SharedProcessor::mirror_failures` (`aec.rs:298`) has no callers at all. The "counted, never swallowed" promise in the docs currently holds only inside the modules that count.

**Fix:** give the terminal `Failed` event delivery priority — e.g. `events.send(SttEvent::Failed(error)).await` from the async `fail` path (it is not on the audio callback; the senders here are async tasks, and a failure report is the last thing the task does, so blocking briefly is correct), or reserve one slot in the channel for the terminal event. Then expose the three counters where a human will see them (the diagnostics payload / `usage_summary`), or delete them; a counter with no reader is a claim the code does not keep.

### WR-04: 讯飞 drops pre-rotation text from the committed final — *latent*

**Files:** `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs:770-780`, `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs:667-678`, `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs:599`

**Issue:** Non-final frames concatenate the text committed before a session rotation:

```rust
let text = if is_final {
    carried_text.clear();          // xfyun.rs:776 — the pre-rotation text is discarded
    builder.text()
} else {
    format!("{carried_text}{}", builder.text())
};
```

The final frame (`data.status == 2`) is the vendor's **only** committed transcript and is exactly what the GOV-15 commit gate consumes (admission keys on the committed final; `cascade.rs:80-240`). If a fragment ever outlives one rotation, the committed text is only the post-rotation tail — up to `MAX_SEGMENT_MS = 15 s` of the user's Chinese would be dropped from what is allowed to be spoken, and the Chinese/English numeric-consistency check would run against a truncated source.

Today the path is unreachable: a fragment is capped at 15 s (`segment.rs:31`) while rotation triggers at 0.9 × 60 s = 54 s (`xfyun.rs:67/599`), and `SttStream::end_fragment` has no callers yet (`traits.rs:263` — the fragment lifecycle is not driven). The invariant that saves this bug lives in a different module and is not asserted at the site.

**Fix:** assemble the text first, then clear the carry:

```rust
let text = format!("{carried_text}{}", builder.text());
if is_final {
    carried_text.clear();
}
```

Better: make the invariant explicit — a test in `tests/mock_vendors.rs` (the rotation test at line 983 currently asserts only that rotation happened and that a status-2 frame was sent) that pins the committed final's text across a rotation, and a debug assertion that a fragment cannot outlive `rotate_deadline()`.

### WR-05: Cost and quota figures are priced with a vendor that is not in the pipeline

**Files:** `apps/desktop/src-tauri/src/trace/mod.rs:57-58`, `apps/desktop/src-tauri/src/trace/mod.rs:138`, `apps/desktop/src-tauri/src/pipeline/stages/mod.rs:105-112`

**Issue:** The trace's rate table is Gemini:

```rust
pub const TRANSLATE_USD_PER_MTOK_PROMPT: f64 = 0.30; // Gemini 3.5 Flash-Lite
pub const TRANSLATE_USD_PER_MTOK_COMPLETION: f64 = 2.50; // Gemini 3.5 Flash-Lite
```

while the shipped translator is DeepSeek `deepseek-chat` (`stages/mod.rs` `VendorTranslator`, `deepseek.rs`; `stages/mod.rs:8` lists `deepseek (T2.4)` as the production line). Every 额度/usage number the user sees — including the monthly budget gate at `MONTHLY_COST_BUDGET_USD` — is therefore computed from a price list belonging to a vendor the product does not call, and the comment makes it look deliberate. If Phase 3 ever makes DeepSeek's real pricing visible, the stored traces cannot be reconciled with the money actually spent.

**Fix:** move the per-vendor rates next to the vendor client (or key a small table by the translator id) and record the vendor id in the trace record alongside the token counts, so `usage` is always computed from the same source of truth as the call. Add a test that asserts the trace's translator id and the rate row agree.

### WR-06: 火山 TTS panics on a poisoned resource lock, unlike every other lock on the audio path

**Files:** `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs:350-355`, `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs:364-367`, versus `apps/desktop/src-tauri/src/audio/playout.rs:461-468` and `apps/desktop/src-tauri/src/audio/aec.rs:303-309`

**Issue:**

```rust
self.resource
    .lock()
    .expect("the resource lock is never poisoned") = resource.to_string();
```

`expect` here aborts the TTS stage (and, in a `run_session` task, kills the sentence) on a poisoned mutex, whereas the two other lock sites on the same audio path deliberately recover ("A poisoned lock must not silence the user's voice: the queue's data is plain audio, so recovering the guard is strictly better than panicking on the audio path", `playout.rs:463-467`). The `expect` message is also a claim the type system cannot back: the lock is `std::sync::Mutex`, and a panic in any holder poisons it.

**Fix:** use the same recovery as the rest of the crate:

```rust
let mut guard = self
    .resource
    .lock()
    .unwrap_or_else(|poison| poison.into_inner());
*guard = resource.to_string();
```

### WR-07: Enrollment biometric artifacts are created world-readable and chmod'ed afterwards

**Files:** `apps/desktop/src-tauri/src/enroll/capture.rs:626-646`, `apps/desktop/src-tauri/src/enroll/voice_store.rs:205-214`, versus the correct pattern at `apps/desktop/src-tauri/src/trace/jsonl.rs:489-498`

**Issue:** Both enrollment writers create the file with the process umask (typically 0644 on macOS) and only tighten it after the data is on disk:

- `capture.rs:626` `hound::WavWriter::create(&path, spec)` streams the whole 3 s+ recording to disk, and `capture.rs:645` applies `set_permissions(0o600)` afterwards;
- `voice_store.rs:205` `File::create(&path)` writes the profile (which names the user's voice id), and `voice_store.rs:213` chmods afterwards.

Between creation and chmod, the user's raw voice recording / voice template is readable by any local user or process (CWE-377/CWE-732). The project's privacy constraint treats this data as the most sensitive thing it stores, and `open_private` in the trace writer shows the codebase already knows the correct pattern:

```rust
std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path)
```

**Fix:** create at 0600 — open the file yourself and hand the handle to hound (`hound::WavWriter::new(std::io::BufWriter::new(file), spec)`), or write to a 0600 temp file in the same directory and `rename` into place; for the profile, `OpenOptions::new().write(true).create(true).truncate(true).mode(0o600)`.

### WR-08: The Deepgram interviewer line cannot be constructed from the dispatch layer

**Files:** `apps/desktop/src-tauri/src/pipeline/stages/mod.rs:52-59`, `apps/desktop/src-tauri/src/pipeline/stages/mod.rs:40-45`, `apps/desktop/src-tauri/src/pipeline/cascade.rs:975-976`, `apps/desktop/src-tauri/src/pipeline/stages/deepgram.rs:1-20`

**Issue:** `deepgram.rs` (the interviewer's English line, Deepgram nova-3) is fully implemented, declared (`pub mod deepgram;`) and tested, and the module doc table at `stages/mod.rs:11` presents it as a shipped stage. But:

```rust
pub enum VendorStt {
    Scripted(ScriptedStt),
    Xfyun(XfyunStt),
    // T2.3 adds `Deepgram(DeepgramStt)` for the interviewer track.   // mod.rs:58
}
```

The variant does not exist, `DeepgramStt` is not re-exported (the `pub use` block at 40-45 exports `XfyunStt`, `DeepseekTranslator`, `VolcTts` but not it), and `InterviewerCascade::new(stt: VendorStt)` (`cascade.rs:975-976`) can therefore only be instantiated with the Chinese STT or the scripted double. The comment still reads as a forward-looking TODO although T2.3 has landed — a reader concluding "the interviewer line is wired" from the module table would be wrong. The plan for this phase requires the interviewer line to be assembled independently (`02-03-PLAN.md:161`).

**Fix:** add the variant, its `From<DeepgramStt>` impl, its `SttSource` arm and the `pub use`, or — if assembling it is Phase 3 work — correct the module doc and the enum comment to say so explicitly.

### WR-09: The realtime-callback contract is violated by allocations and a mutex in the callback bodies

**Files:** `apps/desktop/src-tauri/src/audio/bounded.rs:51-61`, `apps/desktop/src-tauri/src/audio/capture.rs:267-275`, `apps/desktop/src-tauri/src/audio/capture.rs:281-303`

**Issue:** `bounded.rs` states the rule ("**The callback copies into a bounded queue or counts the block as dropped. Nothing else.**", lines 6-9; T-02-23), but the implementation allocates on the audio thread:

```rust
pub fn push(&self, samples: &[f32]) {
    match self.sender.try_send(samples.to_vec()) {   // bounded.rs:55 — heap allocation per block
```

and `capture.rs` adds more: the multi-channel path builds a fresh `Vec` via `downmix_f32` inside the callback (`capture.rs:281-283`), and cpal's error callback takes a `std::sync::Mutex` and allocates a `String` (`capture.rs:285-291`) — the comment there says "The callback must not log (no allocation on the audio thread)" while doing exactly an allocation, one line above. An allocator lock or a contended mutex inside a 480-frame deadline is a dropped-block risk, which is the failure the whole queue exists to avoid.

**Fix:** recycle buffers — send the empty `Vec` back on a second channel (or hold a small free-list) so the callback performs no allocation, and for the error path pre-allocate the worst-case string/use an atomic counter plus a `OnceLock`-initialised slot. If the allocation is a deliberate, measured trade-off, say so in `bounded.rs` and drop the absolute wording; a documented contract that the code contradicts is worse than either alternative.

## Info

### IN-01: Numeric unit comparison pairs facts by `(value, raw)`, so equal values with different units can mispair

**File:** `apps/desktop/src-tauri/src/pipeline/validate.rs:197-232`

**Issue:** Both sides are sorted with `number_key = (value, raw)` and then zipped. Within one `value` group the order is decided by the raw string's Unicode order, which differs by script — the Chinese form and the English form of the same fact can therefore land in different relative orders, and the pairwise unit check (`UnitMismatch` / `MissingUnit`) can fire on a translation that was actually consistent. The direction is fail-closed (the segment abstains rather than showing a wrong number, D-08), so this is a false-abstention risk rather than data loss.

**Fix:** compare unit multisets per value group (`group by value → sort units → compare`), or key the sort by `(value, unit)` and fall back to `raw` only to break ties within the same `(value, unit)`.

### IN-02: `SttStream::end_fragment` and the fragment lifecycle have no callers *yet*

**File:** `apps/desktop/src-tauri/src/pipeline/stages/traits.rs:262-270`

**Issue:** `end_fragment` (the only way a 讯飞/Deepgram session is told "finalise and flush") has no call site in `src/`, and neither the rotation-facing committed-text path (WR-04) nor the `CloseStream`/status-2 flush is exercised end to end. This is consistent with the documented deferral of the session assembly, but it means the vendor flush contracts are currently pinned only by mock-server tests.

**Fix:** when Phase 3 wires the cascade, assert the call explicitly (a test that a segment close produces exactly one `End` upstream per fragment); until then a `// not wired until 03-xx` note at the definition would keep the next reader from assuming it is live.

### IN-03: The latency-panel preview fixture duplicates rig numbers as literals with no test tying them together

**File:** `apps/desktop/src/hooks/useLatencyWaterfall.ts:188-194`

**Issue:** `PREVIEW_SCRIPT` hardcodes five segments' offsets (cold e2e 1180 ms, warm ~1210-1300 ms) and the file says the script follows the Rust rig's five boundaries, but nothing asserts that the fixture's numbers match anything on the Rust side. If the rig's constants change, the preview quietly keeps showing the old story in the panel that is supposed to be the honest display of the rig's output.

**Fix:** generate the fixture from the shared constant/script (or add a test that compares the fixture against a JSON emitted by the Rust side) — or label it in the UI as a static illustration rather than "预览数据".

---

_Reviewed: 2026-10-06T07:00:21Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
