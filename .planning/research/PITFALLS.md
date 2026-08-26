# Pitfalls Research

**Domain:** Real-time AI voice translation / interview copilot (virtual audio, voice cloning, stealth teleprompter)
**Researched:** 2026-08-26
**Confidence:** HIGH for latency/VAD/consent/driver-signing findings (multiple authoritative sources); MEDIUM for macOS 12.7-specific capture behavior and voice-clone performance specifics (platform behavior shifts by OS version, and clone quality is vendor/model dependent)

---

## Critical Pitfalls

### Pitfall 1: Serial pipeline latency stacking — "each stage waits for the previous one"

**What goes wrong:**
End-to-end latency becomes the *sum* of all stages: STT (~300-800ms) + MT (~200-500ms) + TTS first-chunk (~200-500ms, non-streaming TTS adds 4,200ms) + network hops + queue wait + VAD endpointing silence. Naive cascaded systems land at 4-5s glass-to-glass — conversational tolerance is ~500-1000ms. Interviews are already high-stakes; a 4s dead gap after every answer kills the product. This is the exact failure the requirement doc flags.

**Why it happens:**
Developers measure model latency in isolation ("Deepgram is fast") and forget the hidden budget consumers: VAD endpointing is **irreducible latency**, network RTTs stack per-stage, queue wait times hide inside message buses, and TTS cold starts (800-1200ms first request) are invisible in benchmarks. Each module waits for the previous one's *final* output unless explicitly pipelined.

**How to avoid:**
- Cascade the stages (STT partial → MT partial → TTS chunk) so perceived latency approaches the longest single stage, not the sum. Pipeline-parallel execution with async queues between stages measured up to 3.1x latency reduction.
- Build a **latency budget worksheet as a Phase 2 deliverable**: STT first-partial budget, MT first-token budget, TTS first-chunk budget, network budget, jitter budget — then sum them *with* VAD/endpointing time and verify against the 2s target with real network (not localhost).
- Run VAD on a parallel path so its decision time is off the critical path; use 20ms/16kHz audio chunks for STT delivery.
- Shadow voice buffering: pre-generated bridge audio fills TTS synthesis gaps so playback starts instantly.
- Warm persistent connections (TTS/STT) before the session starts; never pay cold-start cost mid-interview.

**Warning signs:**
- Latency measured as sum of per-stage benchmarks instead of a real end-to-end stopwatch from mic-in to BlackHole-out.
- No per-stage instrumentation (a waterfall trace) in the audio pipeline from day one.
- Team celebrates "200ms TTS" while the demo still feels slow.

**Phase to address:**
Phase 2 (Audio Core — latency measurement rig must exist before the AI pipeline) and Phase 3 (AI Pipeline — cascaded streaming architecture). Add e2e latency trace as a Phase 3 success criterion.

---

### Pitfall 2: Acting on unstable partial ASR results — speaking text that is wrong a moment later

**What goes wrong:**
Streaming ASR partials are *hypotheses*, not facts: "fifteen" becomes "fifteen hundred", "to" flickers to "too" then "2", word boundaries shift ("ice cream" → "I scream"). Roughly half of partial instability is text-normalization (numeral, punctuation), half is streaming revision. If you translate-and-speak every partial, the interviewer hears fluent English containing words the user never said — or worse, a correction that contradicts the spoken answer. The product's whole premise is "3-5 words triggers translation," so this is the single most dangerous engineering shortcut.

**Why it happens:**
"Render partials, act on finals" is the documented rule (Soniox: "acting on them irreversibly means acting on text that may be wrong a moment later"), but demo-friendly latency pressure pushes teams to commit partials downstream. Whisper-style models also *hallucinate* on silence (training-data boilerplate, random-language fragments) — one production system produced a stream of hallucinated fragments from an open mic because silent audio reached the model.

**How to avoid:**
- Two contracts in the pipeline: **preview contract** (partials go to the teleprompter, marked provisional) vs **commit contract** (only stable/final segments get translated AND spoken).
- Implement a stability gate: only promote a segment to the TTS path when the ASR has not revised the prefix for N words/ms, or when the provider's final result arrives.
- Gate silence out before STT (RMS threshold + VAD) to kill hallucination at the source.
- If any pseudo-streaming/chunked STT is used (Whisper-based stitching), merge by **audio-timestamp overlap, not text diff** — text-prefix stitching loops and drops segments on Chinese (non-segmented script) and Arabic/Thai.

**Warning signs:**
- Spoken English contains words not present in the Chinese transcript (retroactive partial revisions).
- Transcript shows hallucinated text during silences or before speech starts.
- No test that feeds 100 real interview audio clips and checks "spoken English ⊆ committed finals".

**Phase to address:**
Phase 3 (AI Pipeline). This gate is a core architecture decision, not a tuning knob.

---

### Pitfall 3: Virtual audio driver installs silently but doesn't exist — signing, notarization, and Gatekeeper failures

**What goes wrong:**
The #1 support incident for this product class. A driver that is unsigned or unnotarized: (a) Gatekeeper blocks the installer with "unidentified developer", (b) installs to `/Library/Audio/Plug-Ins/HAL/` but **never appears in Audio MIDI Setup or Sound preferences** (documented BlackHole failure when signing/notarization was stripped), (c) shows nothing in the console. Users hit this minutes before a real interview and panic. On top of that, meeting apps cache the device list: Zoom/Teams often require a full restart (sometimes logout) before they enumerate the new virtual mic.

**Why it happens:**
Development machines are already "trusted" so Gatekeeper never fires — developers test the driver only on their own Mac and ship the raw build. BlackHole 2ch is a HAL plugin (not a kext), so no Reduced Security/SIP dance is needed on Apple Silicon — but the *installer app* still needs Developer ID + notarization, and teams routinely skip this because it requires a paid Developer Program membership and adds CI steps. Bundle structure (Info.plist, executable permissions, root:wheel ownership) must also be exactly right or coreaudiod silently rejects the plugin.

**How to avoid:**
- Treat "sign + notarize + staple" as a Phase 2 CI pipeline deliverable, not a release chore. Verify every build with `spctl -a -v`, `codesign -v`, and check the quarantine attribute.
- Ship an **install wizard with diagnostics**: detect driver presence (`system_profiler SPAudioDataType`), run `launchctl kickstart -k system/com.apple.audio.coreaudiod` after install, surface "restart your meeting app" guidance, and offer "open Security & Privacy pane" deep links.
- Test installation on a clean/VM macOS 12.7 machine with Gatekeeper at full strength in CI, not just on dev hardware.
- Bundle a fallback: if driver install fails, the app should still work in "mic passthrough" mode (no virtualization) rather than dead.

**Warning signs:**
- No CI step validates notarization; driver artifacts never tested on a clean machine.
- Install docs say "just copy the .pkg" without a verification checklist.
- Support path for "driver doesn't show up" is undefined in the UI.

**Phase to address:**
Phase 2 (Audio Core) for the driver itself; Phase 8 (Productization) for signing/notarization CI; install wizard UI in Phase 4 (Desktop UX).

---

### Pitfall 4: Echo/feedback — AEC wired wrong, doubled, or starved of a reference signal

**What goes wrong:**
Three distinct failures: (1) the meeting app's audio (interviewer's voice) plays from speakers, the mic captures it, and the STT transcribes the *interviewer's English* as if it were the user's Chinese — garbage in, garbage out; (2) **double AEC**: the meeting app's built-in echo canceller and the app's own canceller both process the virtual mic signal, producing half-duplex "walkie-talkie" behavior where speech gets cut; (3) AEC silently degrades because the canceller gets the wrong/mismatched reference signal — sample-rate mismatch or independent device clocks drifting apart over a 30-60 minute interview.

**Why it happens:**
AEC is a wiring problem, not an algorithm problem: it needs a *reference signal* (the exact audio being played to the speaker) before any mixing, time-aligned and sample-rate-aligned with the mic signal. On a virtual-mic architecture, the reference must be captured from the correct playback device (the one the meeting app outputs to), which is easy to get wrong; Web Audio/ConvolverNode hand-rolls fail because the raw reference is not exposed. Clock drift between mic, playback, and virtual devices accumulates over long sessions; aggregate devices without drift correction glitch ("skipping cycle due to overload") after minutes.

**How to avoid:**
- Enforce **headphones** as the primary setup for the user (kills speaker→mic coupling; the product already routes interviewer audio to earphones) — document speakers as "unsupported for echo reasons".
- Run exactly **one** echo canceller (WebRTC AEC3 with the correct reference, or Krisp-class neural AEC) and disable the meeting app's AEC/ANS on the virtual mic input if configurable; never stack cancellers or double noise suppression.
- Keep mic and playback devices on the same sample rate (48kHz everywhere); enable drift correction on any aggregate/multi-output device; keep buffers small (5-10ms).
- Build echo diagnostics into the pipeline: delay probing, clock-skew monitoring, and an "echo detected" health flag so routing failures are distinguishable from cancellation failures.

**Warning signs:**
- Interviewer reports hearing themselves.
- STT transcript contains the interviewer's language mixed into the user's speech.
- Audio sounds "robotic/watery" — sign of double-processing, not poor quality.

**Phase to address:**
Phase 2 (Audio Core) — AEC with correct reference is part of the audio graph, not a Phase 3 afterthought. Verify with a real Zoom/Teams call in CI-like scripted tests.

---

### Pitfall 5: Silence ≠ question-end — endpointing false triggers that cut people off or fire early

**What goes wrong:**
The product auto-triggers AI strategy generation on "question end" (silence + LLM completeness check), with no manual button. Acoustic-only silence detection is the weakest signal available: mid-sentence thinking pauses ("Let me think... [AI fires]"), trailing filler words, coughs, and background noise all cause false endpoints. One production team found **18% of user turns truncated** by a single 700ms silence timeout — cut off mid-word on "the", "and", "to". In interviews, heavy thinking silences make this worse. And the asymmetry is brutal: raising the timeout reduces cut-offs but makes the AI feel dead, and on noisy lines VAD flicker can reset the silence counter forever so the trigger never fires at all.

**Why it happens:**
"A single silence timeout is one number trying to answer two different questions" — when the user stopped vs when the user finished. Teams tune one knob and watch failure rates bounce between cut-off and dead-air. Interviewers also emit backchannels ("uh-huh", "mm-hmm") that naive endpointers mistake for turn ends.

**How to avoid:**
- Layer the detection: VAD (is there speech?) → endpoint (has this utterance finished?) → turn/semantic check (is the question complete?). Feed the STT transcript into the endpointing decision: punctuation + dangling-word heuristics (ends with "to", "and", "the", "um" → not done) plus an LLM completeness check, as planned.
- Use **two silence budgets**: short (~550ms) when the transcript looks grammatically finished, long (~1300ms) when it ends dangling — this cut truncations from 18% to ~3% in the referenced deployment.
- Add `minSpeechDuration` (~300-500ms) to kill cough/breath triggers; put noise/background-voice cancellation *before* the VAD.
- **Decouple detection from commitment**: start generating on a probable endpoint, cancel cheaply if the user resumes — the teleprompter strategy card can be discarded or regenerated; it never reaches the audio path, so the cost of a false trigger is low if the UI makes dismissal obvious.
- Hard maximum silence cap guarantees the turn always ends, even on noisy lines.

**Warning signs:**
- Strategy cards appear mid-question or while the user is still speaking.
- Test corpus of real interview Q&A shows >5% wrong-trigger rate.
- No automated replay harness for end-to-end trigger evaluation with real interviewer audio.

**Phase to address:**
Phase 6 (AI Interview Assistant). Requires the Phase 3 transcript pipeline plus a replay/eval harness — schedule the harness as a Phase 6 prerequisite, not an afterthought.

---

### Pitfall 6: Stealth hiding — opacity/sharingType false security; the hidden window still leaks

**What goes wrong:**
(1) **opacity → 0 does NOT prevent capture** — screen-capture engines read the composited framebuffer, so a transparent window is still visible in the share. This is already correctly flagged in the project decisions (real `orderOut` hiding). (2) **`NSWindow.sharingType = .none` is only a partial shield**: it reliably excludes a window from legacy capture APIs (built-in Screenshot, QuickTime, CGWindowList-based recorders) on macOS ≤14, but modern apps using ScreenCaptureKit (Zoom/Teams/OBS on newer versions) capture the composited display and are *unaffected* by sharing state; on macOS 15+ Apple confirmed sharingType is ignored entirely and "there are no public APIs for preventing screen capture". (3) The hidden-window leaks: Tauri's `window.hide()` doesn't unfocus and misbehaves during/after fullscreen; the window can remain in Cmd+Tab, the Dock, Mission Control; helper windows (settings, tooltips, permission dialogs) can pop above the share mid-interview; a screen-recording permission dialog appearing during a share is itself a visible leak.

**Why it happens:**
Developers test with their own Screenshot tool (legacy API) and conclude "hidden". They don't test with the actual meeting apps (Zoom/Teams/腾讯会议) doing a real full-screen share, and they forget that macOS fullscreen + Spaces + Dock behaviors are distinct from the window's visibility flag.

**How to avoid:**
- Primary mechanism is **real hiding of every window** (`orderOut`-equivalent, not `hide` semantics alone): main window, settings, tray popovers — with a single global hotkey (Cmd+Shift+H) toggling all of them and muting the audio path as a bonus panic action.
- Set the app to **accessory activation policy** (no Dock icon, no Cmd+Tab presence) for the desktop client.
- Avoid any macOS permission dialog during share: pre-grant/request Screen Recording, Microphone, and Accessibility permissions at first launch *before* the interview, with a checklist UI.
- Belt-and-suspenders: also set `sharingType = .none` where supported (covers macOS ≤14 legacy capture); treat it as defense-in-depth, never the primary mechanism.
- Design the desktop window so even a leaked frame is low-risk: e.g., decoy mode (show a neutral "timer" chrome when explicitly shown) — but the mobile teleprompter is the real mitigation: the share never includes the phone screen.
- **Test in the real meeting apps** as an explicit Phase 4 acceptance criterion: share full screen in Zoom and Teams on macOS 12.7, record the share, verify zero NexTalk pixels appear.

**Warning signs:**
- Test pass recorded only with QuickTime/legacy capture, not Zoom/Teams actual share.
- Hotkey hides main window but a settings window or tooltip remains.
- Window visible in Cmd+Tab / Mission Control after "hide".

**Phase to address:**
Phase 4 (Desktop UX). Requires the Audio Core (Phase 2) to be non-window-dependent (background service architecture) so hiding never kills the pipeline.

---

### Pitfall 7: Voice clone quality — timbre transfers early, delivery transfers last

**What goes wrong:**
The interviewer hears "the user's voice" saying words — but with the *base model's* prosody: monotone, flat, wrong emphasis on technical terms, robotic cadence. Listeners can't name it but feel it: "sounds like you but reading". Short-reference clones (instant cloning, <2 min audio) sound like the person in tone while landing sentences like the model does. Worse: reference recording quality *bakes into the clone permanently* — reverb makes the clone sound permanently like it's recorded in a room, clipping becomes learned timbre, codec artifacts from video-call clips degrade everything. And long answers generated as separate segments drift in pitch/energy, producing audible seams.

**Why it happens:**
Reference collection is left to the user with no guidance ("record anything"), so samples come from video calls, noisy rooms, or monotone word lists. Instant cloning has a quality ceiling set by the reference and generalizes poorly to expressive styles ("timbre transfers early, delivery transfers late"). Teams demo with a scripted sentence and never test on real interview content (technical terms, numbers, hesitations).

**How to avoid:**
- Ship a **guided reference-recording wizard** (Phase 4/7): minimum clean 20s-2min, connected expressive speech (not word lists), quiet room, no reverb/noise/codec, at least one slightly animated passage. Sample quality beats sample length.
- Verify clone quality on a **real interview-question test set** (numbers, technical terms, "I led a team..."), not a demo sentence; add a pre-session "listen to your clone" step so the user can regenerate before the interview.
- Pre-synthesize the high-frequency utterances (greetings, "could you repeat that", "let me think") to cut live cost and risk.
- If segment-level seams appear, crossfade and loudness-match between segments; re-inject reference for very long utterances.
- Keep a fallback: a neutral high-quality voice the user can switch to mid-session if the clone misbehaves (with honest disclosure to the interviewer).
- Re-evaluate vendor in 2026 terms (ElevenLabs Turbo v2.5 at ~300ms is the current default; Multilingual v2 ~500ms for quality) — the 2024-era selection in the requirement doc should not be assumed optimal.

**Warning signs:**
- Clone demoed only with a scripted sentence.
- Reference samples collected without guidance (user's own room tone).
- No A/B listen test between clone and real voice on the same interview answers.

**Phase to address:**
Phase 3 (AI Pipeline — integration + latency) with clone-quality verification; reference wizard in Phase 7 (Recording/Review assets); re-evaluate provider in Phase 3 research.

---

### Pitfall 8: Recording the interviewer without consent — a legal landmine

**What goes wrong:**
The product records **分轨录制 (separate tracks for user AND interviewer voice)** plus transcripts and review reports. Interviewers are frequently in the US (tech interviews), where California-style all-party consent states (CA Penal Code §632, ~11 states) make recording a confidential conversation without *every* participant's consent a crime: misdemeanor up to 1 year + $2,500 fine, plus civil exposure of **$5,000 per violation per participant** — each participant is a separate violation, and the law applies if even *one* party is in California, regardless of where the host is. AI-notetaker class actions (Otter.ai 2025) show this is being actively litigated. Recording in a China-context also triggers PIPL obligations (notice + consent for biometric voice data). "The user consented" is not enough — the *interviewer* must consent.

**Why it happens:**
Recording tools treat the user as the only stakeholder; "it's their interview, they consented to record it" is wrong under all-party consent rules. Zoom's built-in recording banner is explicitly *not* sufficient notice by itself.

**How to avoid:**
- Default **recording OFF**; a prominent consent gate before any session can start: explain that the interviewer's voice will be recorded locally, and provide a ready-made disclosure line the user can say/paste into the meeting invite ("I'm using a translation assistant that records locally for my own review; the recording is deleted on request").
- Store everything locally-only (already the constraint), add **delete-on-demand** for any track, and never send interviewer audio to cloud AI without documented retention-zero terms (voice is biometric data).
- Ship a compliance checklist in Phase 8: legal review per target market, disclosure copy, regional toggles (one-party vs all-party states), and a "record interviewer? yes/no" user decision at first run.
- Prototype phase is not blocked, but the recording feature must not be marketed/shipped without the consent flow.

**Warning signs:**
- Recording feature demoed without any consent UI.
- No legal review in the roadmap for the recording/review milestone.
- Interviewer audio shipped to cloud STT/AI with default retention.

**Phase to address:**
Phase 7 (Recording & Review — consent gate ships with the feature) and Phase 8 (Productization — legal review, regional compliance).

---

### Pitfall 9: Streaming TTS barge-in and queue race — overlapping English or delayed answers

**What goes wrong:**
TTS streams do not auto-cancel. If the user pauses (thinks) then resumes speaking, or a new utterance commits while the previous English is still playing, the pipeline either queues the second utterance (interviewer hears "answer 1, then answer 2" with the user's live Chinese voice also coming through — double audio) or plays overlapping speech. Cold starts (800-1200ms on first TTS request) and dropped WebSocket connections mid-interview make the problem bursty and unreproducible in demos.

**Why it happens:**
Barge-in handling is assumed to be built into the TTS provider; it isn't — the app owns the queue lifecycle. Teams also benchmark warm-cache latency and miss cold-start and reconnection costs.

**How to avoid:**
- Own the utterance queue explicitly: one active utterance at a time; new commit → either finish current quickly (short) or cancel current stream + flush queued audio chunks; never start a second stream while one is playing.
- Warm the TTS connection + voice model before session start; persistent connection pool; WebSocket reconnection with backoff (documented: WS drops are routine).
- Use streaming-optimized settings (ElevenLabs `optimizeStreamingLatency: 3` — ~200ms first chunk) but beware the tradeoff of slightly robotic cadence; token-level streaming roughly doubles-to-quadruples cost — avoid unless latency demands it.
- Debounce commits (~300ms) to avoid double-triggering from partial flicker.

**Warning signs:**
- Interviewer hears two voices at once.
- Audio keeps playing after the user has moved on.
- No tests for "user re-speaks during playback" and "TTS connection drops mid-utterance".

**Phase to address:**
Phase 3 (AI Pipeline) — queue semantics and interruption handling are part of the cascade design, with fault-injection tests (drop connections, cold cache) in the Phase 3 verification plan.

---

### Pitfall 10: English leakage into STT — the pipeline transcribing the wrong speaker

**What goes wrong:**
If interviewer audio reaches the mic (speakers instead of headphones, or audio routing mistakes), the STT stage transcribes *English*, the MT stage translates it, and the user's own mic path fills with garbage — the interviewer's questions get echoed back in the user's voice, or the user hears translated versions of their own words through the loop. With a virtual-mic topology, misrouting (meeting app output → BlackHole → meeting app input) creates a hard feedback loop that no software AEC can fully escape.

**Why it happens:**
Routing correctness is tested only in the happy path (headphones, default devices). Users in real interviews change output devices (AirPods vs speaker), and macOS default-device changes mid-session silently rewire the audio graph. The system must not assume a stable graph.

**How to avoid:**
- Detect and warn on the speaker path: if the active output device is a speaker (not headphones), show a "use headphones" prompt before the session.
- Route the reference/loopback capture explicitly from the *actual* output device and re-verify on device-change events (listen to `kAudioHardwarePropertyDefaultOutputDevice` changes).
- Channel separation: treat mic input and loopback input as distinct streams; never let the loopback signal into the STT stage.
- Ship a 10-second "audio check" pre-session that plays a test tone through BlackHole and confirms the meeting app receives it (catches routing bugs before the interviewer is on the line).

**Warning signs:**
- Transcripts contain English words mixed with Chinese user speech.
- Any audio path where the meeting app's output feeds back into its own virtual-mic input (hard loop) — add a topology self-check at startup.

**Phase to address:**
Phase 2 (Audio Core) — routing hygiene and device-change handling; pre-session audio check in Phase 4 (Desktop UX).

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Translate + speak every ASR partial (no stability gate) | Demo feels fast, no flicker handling needed | Interviewer hears spoken text that retroactively contradicts the user's answer; core trust destroyed | Never — this is the product's core promise |
| Single hardcoded silence timeout for question-end | One knob, simple code | 15-18% truncation / dead-air oscillation; the exact failure users will abandon over | Prototype demos only; replace with layered endpointing before Phase 6 |
| Instant voice clone instead of professional clone | Zero reference-collection friction | Flat prosody, poor emotion on interview content — "sounds like you, reading"; core differentiator degraded | Prototype only; plan professional-clone option for the core path |
| Pseudo-streaming STT (chunked Whisper + text-prefix stitching) | Vendor-agnostic, cheap | Repetition loops and dropped segments on Chinese; hallucination on silence | Never for the v1 core path — require a true streaming ASR API |
| Aggregate/multi-output device (BlackHole + hardware) as documented setup | One device to select in meeting app | Drift/glitch stutter after minutes without drift correction; pitch-hack clocking | Dev machines only; never in user-facing docs |
| Opacity/transparency animation as "hiding" | Fast, looks slick | Captured by screen share; leaks the entire product | Never — real window hiding only |
| Single mixed-track recording | One file, simple | Cannot separate voices → no clean transcripts, no review reports; legal deletion harder | Never — requirement says 分轨 |
| Embedding cloud API keys in the Tauri binary | Works instantly | Anyone can extract keys from the bundle → theft, abuse, bill shock | Never — keys belong behind a local proxy/OS keychain |
| Mic passthrough without AEC "to keep latency low" | Shaves a few ms | Echo/feedback on any speaker setup; interviewer hears themselves | Only in an explicit "wired headphones only" fallback mode |

## Integration Gotchas

Common mistakes when connecting to external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| ElevenLabs TTS | No warmup → 800-1200ms cold start mid-interview; HTTP per chunk; no reconnect on WS drop | Persistent pool + pre-warm at session start; WebSocket with backoff reconnect; `optimizeStreamingLatency: 3`; own the barge-in queue |
| ElevenLabs (cost) | Token-level streaming for everything | Chunked streaming default; pre-synthesize greetings/common phrases; cache generated audio; token-level only if latency demands |
| Streaming ASR (Deepgram-class) | Ignoring partial-vs-final contract; feeding silent audio to the model | "Render partials, act on finals"; silence-gate input (RMS threshold + VAD); handle final-result promotions correctly |
| Meeting apps (Zoom/Teams/腾讯会议) | Assuming the virtual mic appears without app restart; default mic not set | Wizard instructs full app restart after driver install; ensure virtual mic is set as macOS default input; re-verify after app updates (device caches reset) |
| LAN H5 pairing | QR/WebSocket blocked by AP isolation on corporate Wi-Fi | Fallback: phone hotspot mode (macOS creates hotspot, H5 connects directly); keep pairing timeout short with retry |
| Tauri window APIs | `hide()` as hiding (doesn't unfocus, broken with fullscreen); relying on `sharingType` | Real `orderOut`-equivalent on all windows; accessory activation policy; exit fullscreen before hiding; `sharingType = .none` as defense-in-depth only |
| Cloud AI providers (DPA) | Sending interviewer audio with default retention | Retention-zero terms; minimize data (redact PII between stages); voice = biometric data — require DPA/business terms, never free tier for production voice |
| Local voice clone storage | Storing the cloned voice unencrypted next to recordings | Cloned voice is biometric: store locally, encrypted, user-controlled; document deletion path |

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Real-time audio starvation under CPU load | Pops, stutters, AEC failure exactly when the user needs it (shared screen + WebView + network + AI all competing) | Keep audio thread at high priority; test on the oldest supported Intel Mac; measure RTF under load; WebView rendering throttled when hidden | Sustained >80% CPU on 2015-era Intel Macs; heavy Wi-Fi + VPN traffic |
| Unbounded queue growth between stages | Audio falls further behind; latency balloons silently instead of failing | Backpressure + latency watermark: if the pipeline can't keep up, drop to sentence-level mode or signal "speaking original voice" rather than compounding delay | Network stalls >2-3s (VPN, weak Wi-Fi, meeting-app bandwidth spikes) |
| Clock drift in long sessions | After 30-60min: clicks, echo return, AEC degradation | Drift correction on aggregates; clock-skew monitoring; periodic re-alignment | Sessions >30min with independently clocked mic/playback/virtual devices — i.e., every real interview |
| TTS jitter buffer too small/large | Dropouts (too small) or sluggish barge-in (too large) | ~100-250ms jitter buffer; tune per network; mobile 100-400ms variance is normal | Mobile-to-cloud networks; interviewers' poor conference lines |
| Teleprompter update flood | Phone battery drain, UI jank, WebSocket saturation over LAN | Throttle subtitle renders to display rate (~10-15 fps max); send deltas not full transcripts; H5 page must be lightweight | Long interviews (60min+) at high partial frequency |
| Pseudo-streaming window accumulation | Transcript repetition/drops on long utterances | True session-level streaming state; timestamp-overlap merging | Chinese utterances >20s with chunked stitching |

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| API keys baked into the Tauri binary | Key theft from app bundle → quota theft, billing abuse, malicious use of the user's clone voice | Keys via OS keychain/env; local proxy component for cloud calls; per-user key rotation |
| Interviewer audio to cloud AI with default retention | Biometric data + confidential interview content processed/retained without consent or DPA | Retention-zero terms with vendors; local-only processing where possible; consent gate before recording; redact PII between STT/MT |
| Recording without all-party consent | Criminal + civil exposure ($5k/violation/participant; CA), class-action risk | Default-off recording; explicit consent flow + copy-paste disclosure line; delete-on-demand; legal review per market |
| macOS permission dialogs during screen share | Screen Recording / mic permission prompts appear on the shared display mid-interview | Pre-grant all permissions at first launch with a checklist; never first-request during a session |
| Plain-HTTP LAN teleprompter page | Interception of live interview subtitles by others on the network; session hijack | Random per-session token in QR URL; short-lived sessions; no PII in page cache; HTTPS optional for LAN but token required |
| Untrusted driver install path | Users downloading unsigned driver from random sources | App-bundled installer, notarized, with checksum + version checks; in-app verification of driver state |

## UX Pitfalls

Common user experience mistakes in this domain.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Auto-trigger misfires with no manual fallback | AI strategy card appears mid-question or user can't invoke it when wanted | User chose auto-only: keep it, but add obvious dismiss + "retry/re-ask" affordances; decouple trigger from audio path so misfires are cheap |
| Subtitles lag behind the spoken English | User reads what they already said; timing confusion under stress | Teleprompter may trail slightly but never lead; test sync against the audio path with the interviewer's real network |
| Clone voice surprise | User discovers the voice sounds off in front of the interviewer | Pre-session "listen to your clone" with 3 real-answer samples; regenerate flow; honest fallback voice |
| Driver install failure with no diagnostics | User stuck 5 minutes before the interview, panic | Wizard with step-by-step verification, "what to do if it doesn't show up", meeting-app restart guidance, support log bundle |
| Hiding the window and forgetting how to get it back | User loses the subtitles mid-answer | Single memorable hotkey (Cmd+Shift+H toggles); on-screen toast on hide; also hide the phone screen? No — phone is the teleprompter, it stays |
| Silence detection tuned "safe" | AI never triggers; user feels the product is dead | Two-budget endpointing + hard max cap; surface trigger state in the UI ("thinking…") so delay reads as activity, not failure |
| Reference recording without guidance | Clone permanently degraded by room tone/noise | Guided wizard with live quality meter (noise floor, clipping, length), multiple-style samples |

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Virtual audio driver:** works on the dev machine but fails Gatekeeper/notarization on a clean Mac — verify install on a clean macOS 12.7 VM/machine with `spctl -a -v` and full security settings
- [ ] **Stealth hiding:** hidden from the built-in Screenshot tool but visible in a real Zoom/Teams full-screen share — record an actual share session and check pixel-by-pixel for NexTalk windows (including Dock/tray presence)
- [ ] **Latency:** measured on localhost Wi-Fi with warm caches — measure on the user's real network (interviewer side, VPN, phone H5) with cold-start TTS and first-partial STT included
- [ ] **Voice clone:** sounds great on a scripted demo sentence, fails on real answer content (numbers, technical terms, hesitation) — evaluate with a fixed 20-question interview test set
- [ ] **Question-end detection:** demoed with scripted Q&A, breaks on real thinking pauses and backchannels — replay 20+ recorded real interviews through the detector and measure false-trigger/cut-off rates
- [ ] **Recording:** 分轨 tracks exist but transcripts drift out of alignment over 30+ minutes — verify alignment and voice separation on full-length sessions, not 2-minute clips
- [ ] **H5 teleprompter:** works on the dev phone + home Wi-Fi, fails on corporate Wi-Fi (AP isolation) or mid-session on the interviewer's network — test the hotspot fallback and reconnection path
- [ ] **Barge-in/queue:** never tested with the user re-speaking during playback or a TTS connection drop — fault-injection tests are part of the Phase 3 definition of done
- [ ] **Consent flow:** "we have a settings toggle" but no interviewer-facing disclosure — the consent gate must be a visible pre-session step with a copy-paste disclosure line

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Driver install failure pre-interview | MEDIUM | Guided wizard diagnostics → reinstall → verify with `system_profiler` → fallback to mic passthrough mode so the interview still happens |
| Echo/feedback during live interview | HIGH (interview in progress) | One-hotkey "kill switch": mute virtual output, route user's original voice straight through, so the interview continues at zero risk |
| Clone voice bad in front of interviewer | MEDIUM | One-tap switch to neutral fallback voice + honest disclosure line; regenerate clone from better reference for next time |
| Latency blowout (network/API slow) | HIGH | Degrade to sentence-level mode (higher latency, cleaner audio) or "original voice" fallback; signal mode in UI so the user can compensate |
| AI trigger misfire | LOW | Dismiss/regenerate card; the trigger never reaches audio, so cost is contained if dismissal is one tap |
| STT hallucination garbage | MEDIUM | Silence gating + stability gate catch it early; transcript shows "not committed" markers so the user can see what was/wasn't spoken |
| Consent dispute after recording | HIGH | Delete-on-demand path must be real and fast; record consent choice per session; keep no cloud copies |

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls. (Provisional phase names — map to the final roadmap structure.)

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Serial latency stacking | Phase 2 (latency rig) + Phase 3 (cascade) | End-to-end stopwatch mic→BlackHole ≤2s on real network; per-stage waterfall in CI |
| Unstable partials spoken out loud | Phase 3 | Replay corpus: "spoken English ⊆ committed finals" invariant test |
| Driver signing/notarization/install | Phase 2 (driver) + Phase 8 (CI signing) | Clean-machine install test in CI; `spctl -a -v` pass on every build |
| AEC/echo wiring | Phase 2 | Scripted Zoom/Teams call test: no echo, no interviewer audio in transcript |
| Silence ≠ question-end | Phase 6 (with Phase 3 transcript + replay harness as prerequisites) | ≥20 real interview replays: <5% misfire, <3% cut-off |
| Stealth hiding leaks | Phase 4 | Actual Zoom/Teams full-screen share recordings show zero NexTalk pixels |
| Clone prosody/quality | Phase 3 (integration) + Phase 7 (reference wizard) | 20-question interview test set A/B listen; pre-session clone review |
| Recording consent | Phase 7 (gate) + Phase 8 (legal) | Consent gate is a blocking pre-session step; delete-on-demand verified |
| TTS barge-in/queue races | Phase 3 | Fault-injection tests: re-speak during playback, WS drop mid-utterance |
| English leakage into STT | Phase 2 | Device-change handling tests; pre-session audio check passes |

## Sources

- [BlackHole official repo & FAQ (drift correction, sample rates, install issues)](https://github.com/ExistentialAudio/BlackHole) — HIGH
- [BlackHole install/signing discussion — driver installed but not visible](https://github.com/ExistentialAudio/BlackHole/discussions/753) — HIGH
- [Roon community — BlackHole multi-output stuttering/clock drift post-mortem](https://community.roonlabs.com/t/roon-remote-on-macbook-pro-m3-shows-cores-audio-devices-instead-of-local-devices-ref-utobjg/322947/12) — MEDIUM
- [Deepgram — Real-Time Speech-to-Speech Translation architecture guide](https://deepgram.com/learn/real-time-speech-to-speech-translation) — HIGH
- [Real-Time Audio Translation 2026: Cascaded vs. End-to-End](https://linnk.ai/research/real-time-audio-translation-2026/) — MEDIUM
- [Soniox Voice AI Wiki — Partial vs final results in live transcription](https://soniox.com/wiki/partial-vs-final-results) — HIGH
- [Soniox Voice AI Wiki — VAD vs endpointing vs turn detection](https://soniox.com/wiki/vad-vs-endpointing-vs-turn-detection) — HIGH
- [End-of-turn detection war story: 18% truncation with 700ms timeout and the two-budget fix](https://dev.to/realmarcuschen/ten-days-before-launch-our-voice-agent-kept-cutting-users-off-an-end-of-turn-detection-war-story-3inf) — MEDIUM (single source, but detailed production numbers)
- [Quiq — Why we stopped treating silence as the end of a turn](https://quiq.com/blog/why-we-stopped-treating-silence-as-the-end-of-a-turn/) — MEDIUM
- [Krisp — Improving turn-taking with background noise cancellation](https://krisp.ai/blog/improving-turn-taking-of-ai-voice-agents-with-background-voice-cancellation/) — MEDIUM
- [macOS window privacy research — CGWindowList/sharingState/ScreenCaptureKit behavior](https://github.com/privateai0/macos-window-privacy-research) — MEDIUM (community research, consistent with Apple forums)
- [Apple Developer Forums — NSWindow sharingType ignored by ScreenCaptureKit on macOS 15.4+](https://developer.apple.com/forums/thread/792152) — HIGH
- [iotools — "You cannot hide a window from Zoom on macOS 15"](https://iotools.cloud/zh/journal/you-cannot-hide-a-window-from-zoom-on-macos-15/) — MEDIUM
- [Tauri issue #14200 — ScreenCaptureKit ignores setContentProtection/sharingType](https://github.com/tauri-apps/tauri/issues/14200) — HIGH
- [Tauri issue #7540 — hiding a window on macOS doesn't unfocus](https://github.com/tauri-apps/tauri/issues/7540) — HIGH
- [Tauri issue #12056 — hide() + fullscreen ordering bug](https://github.com/tauri-apps/tauri/issues/12056) — HIGH
- [ElevenLabs docs — Voice cloning (instant vs professional, reference quality)](https://elevenlabs.io/docs/eleven-api/concepts/voice-cloning) — HIGH
- [Voice cloning: sample quality beats length, prosody beats both](https://dev.to/clarajbennett/voice-cloning-sample-quality-beats-sample-length-and-prosody-beats-both-48eb) — MEDIUM
- [Streaming TTS: architecture, cost, production reality](https://theneuralbase.com/voice-ai/learn/intermediate/streaming-tts/) — MEDIUM
- [ElevenLabs 2026 review — v3, latency tiers, emotion limits](https://www.coval.ai/blog/elevenlabs-review-2026-voice-cloning-and-synthesis-capabilities-explained) — MEDIUM
- [California Penal Code §632 all-party consent + AI notetaker exposure guide](https://www.defendmybiz.com/blog/california-two-party-consent-recording-ai-notetaker-employer-guide) — HIGH
- [Lexology — Beware of AI notetakers: wiretap and compliance implications](https://www.lexology.com/library/detail.aspx?g=59c865ef-ab96-4377-8a3e-04486b9f5d16) — MEDIUM
- [AEC in WebRTC — reference signal, double-processing trap, wiring guidance](https://www.forasoft.com/learn/ai-for-video-engineering/articles-ai/echo-cancellation-aec-ai-hybrid-webrtc) — MEDIUM
- [webrtc/samples issue #1243 — Firefox AEC requires same audio graph / native sample rate](https://github.com/webrtc/samples/issues/1243) — MEDIUM
- [Onyx PR #12713 — silence gating to stop STT hallucination](https://github.com/onyx-dot-app/onyx/pull/12713) — MEDIUM
- [vLLM issue #47264 — Qwen3-ASR pseudo-streaming repetition loops on Chinese](https://github.com/vllm-project/vllm/issues/47264) — MEDIUM
- [Streaming ASR stability metrics (partial vs final, UPWR)](https://ar5iv.labs.arxiv.org/html/2006.01416) — MEDIUM

---
*Pitfalls research for: real-time AI voice translation / interview copilot with virtual audio, voice cloning, stealth teleprompter*
*Researched: 2026-08-26*
