# Project Research Summary

**Project:** 极言 NexTalk — real-time AI cross-language interview copilot (CN→EN voice translation via user's cloned voice + AI interview strategy, macOS Tauri desktop + phone H5 teleprompter)
**Domain:** Real-time speech-to-speech translation / AI interview assistance (desktop app, virtual audio driver, cloud cascaded streaming pipeline)
**Researched:** 2026-08-26
**Confidence:** MEDIUM-HIGH (architecture + pitfalls verified against official sources; vendor quality claims for Chinese voice cloning and Gemini Live text-modality behavior require phase-1 experiments)

## Executive Summary

NexTalk is a real-time Chinese→English simultaneous-interpretation desktop app that routes the *user's own cloned voice* into Zoom/Teams/腾讯会议 via a virtual audio device, with a phone H5 teleprompter and a resume-grounded AI strategy engine. Experts build this class of product as a **five-layer architecture** (audio I/O → cascaded streaming pipeline → Tauri desktop app → LAN WebSocket sync → AI copilot) with two audio loops (user mic path; BlackHole virtual-mic output path), and the dominant 2026 production paradigm is the **cascaded streaming pipeline (STT→MT→TTS, partial→partial→chunk)** — not end-to-end models. This matters decisively: the new 2026 unified speech-to-speech models (Gemini 3.5 Live Translate ~2.9s first audio, gpt-realtime-translate with *no voice selection at all*) are both **slower than the 2s budget and incapable of the product's core requirement (clone the user's voice)**. A fully-streamed cascade with pipeline parallelism lands at ~600ms–1.2s glass-to-glass. Keep the cascade.

The recommended approach: replace the requirement doc's 2024-25 vendor picks (all superseded in 2026) — **ElevenLabs → MiniMax Speech 2.6 Turbo** (best Chinese cross-lingual clone, ~$10-40/M chars vs ElevenLabs ~$300/M; ElevenLabs has documented Mandarin weaknesses), **GPT-4o-mini → Gemini 3.5 Flash-Lite/Flash**, **Tavily → Brave Search API** (Tavily acquired by Nebius, roadmap risk), **keep Deepgram Nova-3** for the interviewer's English path (still the latency leader there), and use **Gemini Live API text modality** for the user's Chinese STT+translation (one WebSocket hop instead of three). Build order: **protocol + UI skeleton with simulated audio first** (the phone H5 and desktop UI are demoable before any driver exists), then the real cloud pipeline plus a **latency rig**, then virtual-device integration, then the copilot, then recording/review. The decisive validations are Phase 1-2 experiments: a blinded **clone listening test** (MiniMax vs Cartesia vs Fish vs ElevenLabs), a **Chinese WER A/B** (Gemini Live vs Deepgram Nova-3 on real interview audio), and an **end-to-end latency rig** on the user's real network.

Key risks and mitigations: (1) **Licensing trap — BlackHole is GPL-3.0, not MIT**: never fork or bundle it in a closed-source app; ship a guided installer wizard for the official build (industry-standard pattern). (2) **Latency collapse**: serial pipelines land at 4-5s; only pipeline-parallel streaming with a stability gate (never speak uncommitted ASR partials) holds the 2s promise. (3) **Endpointing**: silence ≠ question-end; layered detection with two silence budgets (~550ms/1300ms) cut truncation rates from 18% to ~3% in production systems. (4) **Legal**: recording the interviewer without all-party consent is a criminal offense in California-class jurisdictions ($5k/violation/participant); recording must default OFF with a visible consent gate. (5) **Competition**: DeepL Voice-to-Voice (announced Oct 2025) and Zoom Voice Translator will commoditize the audio chain within ~12-24 months — the moat is the **interview vertical + language-bridge positioning + phone stealth teleprompter**, so vertical focus and speed matter more than audio-chain novelty.

## Key Findings

### Top Decision-Relevant Findings

1. **Keep the cascade; do NOT adopt unified S2S models.** Gemini 3.5 Live Translate (~2.9s first audio, voice-instability, not a clone, audio-only), gpt-realtime-translate (no voice selection, 13 languages, no tools), Azure Live Interpreter + Personal Voice (only true single-API match at 0.4-0.7s but requires Microsoft limited-access **approval** + lock-in). Cascade worst case ≈1.2-1.6s e2e — the unified models are *slower*, not faster. Re-evaluate Azure only if approval is granted mid-milestone.
2. **BlackHole is GPL-3.0 — licensing trap.** Official README: non-GPLv3 apps need commercial license from Existential Audio. Never fork/rename/bundle; ship the official installer via guided wizard. Self-written HAL plugin = months of work; Rogue Amoeba Loopback = $99/device, not distributable. v1 verdict: official install + detect + route.
3. **2024-25 vendor picks are all superseded** (see stack table). ElevenLabs: Mandarin tone errors, weak Chinese emotion, 5-10x cost, v3 not realtime-optimized. GPT-4o-mini: legacy, ~4x slower. Tavily: Nebius acquisition → roadmap uncertainty. Bing retired Aug 2025; SerpApi under Google DMCA lawsuit (avoid).
4. **Latency is an architecture property, not a vendor property.** Serial STT→MT→TTS = 4-5s+ (death). Pipeline-parallel streaming with typed stage contracts = up to 3.1x reduction; every stage must emit increments; VAD/endpointing silence is *irreducible* budget; TTS cold starts (800-1200ms) must be pre-warmed. Build the latency rig (e2e stopwatch + per-stage waterfall) before the real pipeline, in Phase 2.
5. **Never speak uncommitted ASR partials.** Half of partial instability is text normalization, half is streaming revision; hallucination on silence is documented. Two contracts: preview (teleprompter, marked provisional) vs commit (translation + TTS). Stability gate + silence gating are core architecture decisions, not tuning knobs.
6. **Endpointing (silence ≠ question-end) is the core UX risk.** 700ms single timeout truncated 18% of turns; two silence budgets (short ~550ms when grammatically finished, long ~1300ms when dangling) → ~3%. Layer VAD → endpoint → LLM completeness; decouple trigger from commitment (dismissable strategy card); replay harness with ≥20 real interviews as a prerequisite.
7. **Recording the interviewer is a legal landmine.** CA Penal Code §632 all-party consent: crime + $5k civil per violation per participant; AI-notetaker class actions active (Otter 2025); PIPL applies for China context. Default-OFF recording, visible consent gate + copy-paste disclosure line, delete-on-demand, local-only, legal review at productization.
8. **~12-24 month competitive window.** DeepL Voice-to-Voice (announced Oct 2025, preserves voice) is the biggest threat to the own-voice differentiator; Zoom Voice Translator commoditizes in-meeting translation (consecutive-only, 5 langs). The virtual-mic + clone pattern is proven (Pinch, Loquora, Krisp, tiyov) — not patentable, de-risked but not unique. Detection arms race (2% → 10%+, eye-tracking, dual-camera) makes full-answer tools toxic; the language-bridge positioning is the escape hatch.

### Recommended Stack

**Definitive 2026 vendor stack per pipeline stage** (detailed in [STACK.md](./STACK.md)):

| Pipeline Stage | Primary 2026 Pick | Why | Fallbacks |
|---|---|---|---|
| STT — user's Chinese (feeds clone TTS) | **Gemini Live API text modality** (`gemini-3.1-flash-live`) | Chirp-3-class Mandarin (Nova-3 trails on zh; "Mandarin can be 5x worse than English"), fuses STT+translation into one WS hop | Deepgram Nova-3 (zh) + separate translate |
| STT — interviewer's English (subtitles/copilot) | **Deepgram Nova-3** streaming (~$0.29/hr) | Sub-300ms, English is its strongest language; second independent vendor path | `gpt-4o-transcribe` |
| Translation (C→EN incremental) | **Gemini 3.5 Flash-Lite** ($0.30/$2.50 per MTok, ~350 t/s); upgrade to 3.5 Flash ($1.50/$9) for idiomatic quality | GPT-4o-mini is legacy and ~4x slower | Claude Haiku 4.5 |
| Copilot agent | **Gemini 3.5 Flash** (1M ctx, 280+ t/s, $0.15/MTok cache reads) | Resume+题库 fit with prompt-cache discount; ~$0.05-0.2/question | Claude Sonnet 5 (best tool-use, $3/$15 after Aug 31 intro); Haiku 4.5 for micro-tasks |
| TTS voice clone (user's voice, C→EN) | **MiniMax Speech 2.6 Turbo** (10s clone, sub-250ms, ~99% similarity, ~$10-40/M chars; cloning only on international `api.minimax.io`) | Best Chinese cross-lingual cloning; 1/5-1/10 ElevenLabs cost | Cartesia Sonic 3.5 (sub-50ms TTFA); Fish S2.1 Pro (budget, CJK 3-byte billing gotcha); ElevenLabs Flash v2.5 last resort |
| Web search (copilot grounding) | **Brave Search API** (+ LLM Context API) | 669ms, largest independent index, BrowseComp 38.3% vs Tavily 19.3% | Tavily (fastest integration, post-acquisition risk); Exa (semantic) |

**Desktop core technologies** (from [STACK.md](./STACK.md)):
- **Tauri 2.11** + **Rust** (stable): desktop shell; Rust zero-copy audio graph; ~10MB bundle; runs on macOS 12.7
- **React 19 + TypeScript + Vite 7** (esbuild target `safari15`): single shared frontend for desktop webview + phone H5
- **Tailwind CSS v3.4.x — NOT v4**: macOS 12.7 ships Safari 15.6; Tailwind v4 needs Safari 16.4+ (`@property`, `color-mix`, `oklch`)
- **cpal** (mic/loopback/BlackHole I/O) + **coreaudio-sys** (multi-output device creation, default-device switching — cpal cannot do this) + **webrtc-audio-processing ~2.0** (AEC3, pin with `~` — tracks PulseAudio snapshot, not semver) + **rubato** (resampling) + **hound**/audiopus (dual-track recording)
- **BlackHole 0.7.1** official installer (guided wizard), **silero-vad-rs** (endpointing), **tokio-tungstenite** (all three WSS providers + LAN), **axum** (LAN server), **qrcode** (pairing), **reqwest** (TTS streaming)

**Version traps to price in:** Claude Sonnet 5 intro pricing ($2/$10) and Fish free tier end **Aug 31, 2026** (5 days after research); MiniMax keys not interchangeable between `api.minimax.io` and mainland; Gemini Live audio format PCM16/16k mono in, 24k out, 100ms chunks; cpal streams are not `Send + Sync` on macOS — dedicated audio thread with channels.

### Expected Features

**Must have (table stakes)** — every competitor ships these; missing them feels broken:
- ≤2s cascaded streaming (3-5 words triggers translation, mid-sentence TTS)
- Real-time bilingual subtitles (desktop + phone) with 全中/全英/双语 toggle
- Bilingual timestamped transcript export (SRT/Markdown/Word)
- Local dual-track recording (user vs interviewer)
- Question detection → auto-triggered AI assist (auto-only per PROJECT.md; manual fallback + dismiss is the mitigation for misfires)
- Resume import (PDF/Word) + grounding of answers
- Stealth/discreteness (Cmd+Shift+H, anti-screen-capture)
- Glossary/hot-word pinning (K8s, backpressure...)
- Voice enrollment wizard (1-3 min clean recording is industry norm) + virtual-device setup wizard (the #1 support question in this category)
- Mobile/web companion second screen (QR pairing)

**Should have (differentiators)** — scarce in combination, the actual moat:
- **Own-voice output via virtual mic into Zoom/Teams/腾讯会议** — the Core Value chain (no competitor targets interviews with this)
- **Mobile H5 teleprompter** (subtitles top, strategy bottom) — undetectable by webcam-tracked proctoring; no competitor has an interview teleprompter on a second device
- **Resume + question-bank grounded auto-strategy in ~1.5s** (bullets + draft answer in user's own words) — requires pre-interview indexing, embedded RAG-lite, streaming output; optional live web search
- **Anti-screen-capture stealth (Cmd+Shift+H real orderOut, audio keeps running)** — 面灵 is the only competitor with this; window orderOut + separate-process audio pipeline
- **Language-bridge positioning vs answer-generation** — strategically the clean escape from the cheating-detection arms race; shapes marketing, pricing, anti-features
- **Local-only privacy** (no accounts, no cloud storage) — trust advantage; proven demand (Timekettle "no cloud")
- **复盘报告** (interview-shaped: action items, sentiment, key concerns, per-question replay) — unclaimed territory; cheap LLM over local assets

**Defer to v1.x/v2+:** glossary pinning (v1.x, low cost), delivery-quality feedback (fillers/pacing), mock-interview practice mode (a second product; Yoodli/Huru own it), voice-swap try-before-clone, Windows client, extra language pairs, cloud sync/accounts.

**Anti-features to refuse:** full-answer reading mode (detection risk + regulatory gray zone), proctoring-evasion tech, two-way voice cloning of the interviewer (biometric consent risk; 原声透传 already decided and better), v1 multi-language pairs (clone cost scales linearly), v1 Windows (second audio core), cloud backend v1, coding-interview hints (strongest cheating perception), 机考 anti-cheat features.

### Architecture Approach

Five layers + two audio loops (detailed in [ARCHITECTURE.md](./ARCHITECTURE.md), HIGH confidence): audio I/O (Rust realtime threads, ring buffers) → cascaded streaming pipeline (tokio mpsc stage queue: VAD → STT → incremental translation → sentence aggregation → TTS → BlackHole) → Tauri desktop layer (commands/events, session state, LAN WS server, QR pairing) → phone H5 teleprompter. Two loops: outbound user Chinese→English voice (mic → pipeline → BlackHole → meeting app virtual mic); inbound interviewer English → headphones **zero-latency via multi-output device [headphones + BlackHole]** (hardware fan-out, no software copy) + loopback capture for subtitles and question-end detection.

**Major components:** DeviceManager (BlackHole detection, multi-output creation, routing), MicCapture + LoopbackCapture + TtsOutput (cpal), STT/translate/TTS stages (typed contracts: `AudioChunk → TranscriptDelta → Sentence → AudioChunk`), question-end detection (VAD + `speech_final` + LLM completeness), Copilot stage (KB RAG + web search), Recording assets (dual-track WAV/Opus), LAN server (tokio WS + pairing-as-auth token), Tauri core, desktop frontend, mobile H5.

**Iron rules:** audio exists only in the Rust layer — WebView never touches audio (browser getUserMedia = 300-400ms + can't reach virtual devices); every stage streams increments (never wait for full sentences); `is_final: true` ≠ sentence end (must concatenate to `speech_final`); TTS starts at 320ms blocks with 60-80ms jitter buffer (buffering = latency floor); 48kHz everywhere with drift correction on aggregates; barge-in (user re-speaks → cancel TTS within 100ms); SimSource (`AudioSource` trait) lets UI/phone/copilot develop without the driver; session state in Rust as single source of truth, WebView/H5 are subscription render layers; wake lock on H5 needs a fallback (hidden looping muted `<video>`, NoSleep.js technique — `http://192.168.x.x` is not a secure context).

### Critical Pitfalls

1. **Serial pipeline latency stacking** — every stage waits for the previous; naive cascade = 4-5s glass-to-glass. *Avoid:* pipeline-parallel streaming from day one; latency budget worksheet + e2e stopwatch rig as a Phase 2 deliverable; pre-warm all provider connections.
2. **Speaking unstable partials** — interviewer hears English the user never said (retroactive revisions), hallucination on silence. *Avoid:* preview vs commit contracts; stability gate; silence gating (RMS + VAD) before STT; invariant test "spoken English ⊆ committed finals".
3. **Driver installs "successfully" but doesn't exist** — unsigned/unnotarized BlackHole never appears in Audio MIDI Setup; Gatekeeper blocks; meeting apps cache device lists (full restart needed). *Avoid:* sign+notarize+staple as CI deliverable; install wizard with diagnostics (`system_profiler`, coreaudiod restart, Security pane deep links); clean-machine (VM macOS 12.7) install test; mic-passthrough fallback mode.
4. **Echo/feedback — AEC wired wrong, doubled, or starved of reference** — double-AEC causes walkie-talkie half-duplex; clock drift glitches after 30-60 min. *Avoid:* headphones as the only supported primary setup; exactly one canceller (WebRTC AEC3 with correct reference); 48kHz everywhere + drift correction; echo diagnostics + health flag.
5. **Silence ≠ question-end** — thinking pauses, backchannels, coughs cause cut-offs (18% truncated at 700ms timeout) or dead-air. *Avoid:* layered VAD→endpoint→LLM completeness; two silence budgets (~550ms/~1300ms) → ~3%; `minSpeechDuration`; hard max cap; replay harness with ≥20 real interviews (<5% misfire, <3% cut-off).
6. **Stealth hiding false security** — opacity→0 and `sharingType` don't stop ScreenCaptureKit-based capture (Zoom/Teams/OBS); ignored entirely on macOS 15+. *Avoid:* real `orderOut` on every window via Cmd+Shift+H; accessory activation policy (no Dock/Cmd+Tab); pre-grant all permissions at first launch; verify with *actual* Zoom/Teams full-screen share recordings (zero NexTalk pixels); phone teleprompter as the real mitigation.
7. **Clone quality: timbre transfers early, delivery transfers last** — "sounds like you but reading"; bad reference audio bakes in permanently. *Avoid:* guided reference wizard (20s-2min clean connected expressive speech, quality > length); verify on a real 20-question interview test set (numbers, technical terms); pre-session "listen to your clone"; neutral fallback voice; segment crossfade + loudness match.
8. **Recording the interviewer without consent** — criminal/civil exposure (CA §632, $5k/violation/participant), class-action risk (Otter 2025), PIPL. *Avoid:* default-OFF recording; blocking consent gate with copy-paste disclosure line; delete-on-demand; retention-zero vendor terms (voice = biometric); legal review per market before shipping.
9. **TTS barge-in/queue races** — overlapping or queued English while user continues; cold starts (800-1200ms) and WS drops bursty mid-interview. *Avoid:* app owns the queue (one active utterance; cancel + flush on new commit); pre-warm connections; ~300ms commit debounce; fault-injection tests (re-speak during playback, WS drop).
10. **English leakage into STT** — interviewer audio reaching the user-mic path transcribes English garbage into the pipeline; hard feedback loop in misrouted virtual-mic topology. *Avoid:* routing hygiene (distinct mic vs loopback streams); headphones enforcement; device-change handling (`kAudioHardwarePropertyDefaultOutputDevice`); pre-session 10-second audio check through BlackHole.

## Implications for Roadmap

The dependency graph dictates the order: **protocol/UI first (mockable), audio core + latency validation second, virtual device third (the only external dependency, a process risk not architecture risk), copilot and recording last (incremental value layers).** UI and phone H5 can be built and demoed *before* the driver exists via SimSource — take the driver off the critical path. The following phase structure reconciles the build orders in ARCHITECTURE.md (P1-P5) and PITFALLS.md (phases 2-8):

### Phase 1: Foundation + Simulation Mode
**Rationale:** Contracts and UI are the cheapest things to change late; the whole upper stack is demoable on simulated audio (SimSource + mock STT/LLM/TTS). Runs in parallel with the decisive vendor experiments.
**Delivers:** Tauri 2 + React 19 skeleton (esbuild `safari15`), neobrutalism design system (per confirmed design spec), desktop dual-pane UI (subtitles left, strategy right), phone H5 teleprompter (LAN QR pairing, 全中/全英/双语 toggle), WebSocket protocol package (`packages/protocol`), Cmd+Shift+H shortcut, full event flow on simulated audio.
**Addresses (FEATURES.md):** mobile H5 teleprompter, bilingual subtitles UI, stealth shortcut, pairing-as-auth.
**Avoids (PITFALLS.md):** none of the pipeline pitfalls yet — this is the de-risking phase.
**Decisive experiments to run here (not code, validation):**
- **STT A/B**: Gemini Live text modality vs Deepgram Nova-3 on 20+ clips of real interview Chinese — WER, partial cadence, stability (μ-Bench warns Mandarin variance is huge)
- **Clone listening test**: MiniMax 2.6 Turbo vs Cartesia Sonic 3.5 vs Fish S2.1 Pro vs ElevenLabs Flash, blinded, ~20 real interview answers, 3 raters
- **Network RTT per vendor** from the user's region (the whole ≤2s budget collapses if each hop adds 200ms+)
- Probe Gemini Live text-modality exact model string + interim-transcript behavior (docs change fast)

### Phase 2: Audio Core + Real Cloud Pipeline
**Rationale:** Validates the two core product assumptions — ≤2s latency and fluent translation — without needing the driver. All pipeline pitfalls live here; instrumentation must exist from day one.
**Delivers:** cpal mic capture, real streaming providers wired (Gemini Live / Deepgram Nova-3 / Gemini Flash-Lite / MiniMax), pipeline parallelism with typed stage contracts, **stability gate** (preview vs commit), **barge-in queue** with pre-warmed connections, **latency rig** (e2e stopwatch mic→output + per-stage waterfall), routing hygiene + device-change handling, AEC reference wiring, output to default device (ear verification, no Zoom yet).
**Addresses (FEATURES.md):** cascaded ≤2s streaming, own-voice chain (pre-virtual-device), transcript engine.
**Avoids (PITFALLS.md):** #1 latency stacking (rig + waterfall), #2 unstable partials (stability gate + invariant test), #4 AEC wiring, #9 barge-in races (fault injection), #10 English leakage (routing).
**Success criteria:** e2e ≤2s on real network *with cold starts*; "spoken English ⊆ committed finals" on 100-clip corpus; WS drop + re-speak fault-injection tests pass.
**Research flag:** interim-result billing amplification (Deepgram counts messages; Gemini Live token costs) — cost monitoring must be wired here.

### Phase 3: Virtual Device Integration
**Rationale:** Adds the only external dependency. BlackHole install + signing is a process risk (Gatekeeper, notarization, meeting-app caching), not an architecture risk — isolate it.
**Delivers:** BlackHole official install wizard with diagnostics (detect via `system_profiler`, coreaudiod restart, Security pane deep links, meeting-app restart guidance), multi-output device creation (coreaudio-sys, `kAudioAggregateDeviceIsStackedKey`), TTS→BlackHole output, loopback capture of interviewer audio, drift correction, pre-session 10s audio check, mic-passthrough fallback.
**Addresses (FEATURES.md):** own-voice virtual-mic output (the Core Value), setup wizard.
**Avoids (PITFALLS.md):** #3 (sign/notarize in CI, clean-machine test), #4 (drift/echo in the real topology), #10 (topology self-check).
**Research flag:** signing/notarization workflow + clean Monterey VM test harness; decide whether basic signing CI comes now or at productization (PITFALLS.md maps full signing CI to productization — flag for the roadmap planner).

### Phase 4: Stealth + Desktop UX Completion
**Rationale:** Stealth must be verified against real meeting apps, and requires the Phase 2/3 audio pipeline to be non-window-dependent so hiding never kills the session.
**Delivers:** real `orderOut` hiding of all windows via Cmd+Shift+H, accessory activation policy (no Dock/Cmd+Tab), permission pre-grant checklist (Screen Recording/Mic/Accessibility at first launch), decoy chrome, wizard polish, glossary/hot-word pinning (first v1.x feature per FEATURES.md), dual-track recording plumbing (audio side).
**Avoids (PITFALLS.md):** #6 stealth leaks — acceptance criterion: actual Zoom/Teams full-screen share recordings show zero NexTalk pixels (incl. Dock/tray/tooltips).
**Standard patterns:** macOS window management, Playwright visual regression (320/768/1024/1440 per web rules) — no research phase needed beyond the already-documented Tauri caveats.

### Phase 5: AI Interview Assistant (Copilot Engine)
**Rationale:** Requires the transcript pipeline (question-end detection consumes interviewer STT from loopback, Phase 3) and a replay/eval harness as a *prerequisite*. Resume import must complete *before* the interview starts (pre-indexing), so the setup flow ships with this phase.
**Delivers:** layered question-end detection (VAD → endpoint → LLM completeness, two silence budgets ~550/~1300ms, hard max cap), resume (PDF/Word) + question-bank import with pre-indexing + embedded RAG-lite, Gemini 3.5 Flash strategy generation (bullets + draft in user's own words, ~1.5s, streaming), optional Brave web search, strategy cards to desktop right pane + phone bottom pane, replay/eval harness.
**Addresses (FEATURES.md):** resume-grounded auto-strategy (differentiator core), question detection, live web search (P2 priority).
**Avoids (PITFALLS.md):** #5 endpointing — acceptance: ≥20 real interview replays, <5% misfire, <3% cut-off; auto-trigger with obvious dismiss/retry so misfires are cheap.
**Research flag:** question-end detection eval methodology (replay corpus construction); PDF/Word parsing pipeline; endpointing parameter tuning per interviewer speaking style.

### Phase 6: Recording + Review Assets
**Rationale:** Consumes the audio graph (Phase 2/3) and transcript (Phase 5). The consent gate ships *with* the feature — never after.
**Delivers:** dual-track recording (WAV/Opus, timestamp-aligned), bilingual transcript export (SRT/Markdown/Word), 复盘报告 (action items, sentiment, key concerns, per-question replay), **consent gate** (default OFF, copy-paste disclosure line, per-session consent record, delete-on-demand).
**Addresses (FEATURES.md):** local dual-track recording, transcript export, 复盘报告.
**Avoids (PITFALLS.md):** #8 consent landmine — gate is a blocking pre-session step; delete-on-demand verified.
**Standard patterns:** WAV/Opus I/O, Markdown/SRT generation — no research phase needed. **Research flag:** consent/legal research per target market (CA-class all-party states, PIPL) — legal review must be scheduled before this ships.

### Phase 7: Productization
**Rationale:** Distribution hardening only after the product works on the dev machine — the classic "works on my Mac" trap.
**Delivers:** signing/notarization CI (every build verified with `spctl -a -v`, clean macOS 12.7 VM test incl. Intel + M1), compliance checklist (legal review per market, retention-zero vendor terms, regional recording toggles), pricing (must undercut $89-148/mo interview copilots; positioned as a language tool), bundled assets (Space Grotesk/FontAwesome local — no CDN per offline constraint), onboarding polish.
**Avoids (PITFALLS.md):** #3 driver distribution, #8 compliance; performance traps (CPU starvation on old Intel Macs, API cost caps).
**Research flag:** pricing survey; notarization CI tooling.

### Phase Ordering Rationale
- **Protocol + UI first** (Phase 1): contracts are the cheapest late change; SimSource removes the driver from the critical path; H5 teleprompter is demoable before any AI exists.
- **Latency validated before the driver** (Phase 2 before 3): the ≤2s budget is the core product assumption; it can be verified to a default output device and only re-verified through BlackHole.
- **Driver isolation** (Phase 3): BlackHole install is a process risk (Gatekeeper/notarization/meeting-app caches), not an architecture risk — it must not block AI-pipeline development.
- **Copilot after the transcript pipeline** (Phase 5): question-end detection consumes interviewer STT from loopback (Phase 3); the replay harness is a prerequisite, not an afterthought. Partial copilot work (resume import, KB, prompts) can start in Phase 2 simulation.
- **Recording after consent research** (Phase 6-7): prototype not blocked; shipping without the consent flow is a legal exposure.

### Research Flags
- **Phase 1:** STT A/B + clone listening test + network RTT + Gemini Live API behavior — the decisive vendor experiments; design the blinded methodology carefully (real interview audio, 3 raters, fixed test set).
- **Phase 2:** latency rig methodology; interim-result billing amplification (cost monitoring).
- **Phase 3:** BlackHole signing/notarization workflow + wizard UX; multi-output creation via coreaudio-sys; clean Monterey VM test harness.
- **Phase 5:** endpointing eval corpus; PDF/Word resume parsing; endpointing parameter tuning.
- **Phase 6:** consent law per market (CA §632 all-party states, PIPL); disclosure copy.
- **Phase 7:** notarization CI; pricing survey.
- **Standard patterns (skip research-phase):** Phase 1 scaffolding (Tauri 2 + React + Vite + Tailwind v3.4, QR pairing, WebSocket) and Phase 4 (window management) are well-documented — proceed with implementation directly.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM-HIGH | 2026 vendor landscape verified against multiple sources (official docs + independent benchmarks); per-vendor **Chinese→English clone quality** and **Gemini Live text-modality behavior** are unverified and MUST be validated by Phase 1 experiments |
| Features | MEDIUM-HIGH | Competitor features verified via vendor sites + 2025-26 comparison articles; product claims are vendor-stated; competitive-window estimates (~12-24 months) are informed inference |
| Architecture | HIGH | Key facts verified against official docs/repos (BlackHole GPL-3.0 + technical architecture, Tauri/WebKit mapping, cpal, Deepgram endpointing semantics) |
| Pitfalls | HIGH | Latency/VAD/consent/driver-signing findings from multiple authoritative sources; MEDIUM for macOS 12.7-specific capture behavior and clone-performance specifics (OS-version and vendor-model dependent) |

**Overall confidence:** MEDIUM-HIGH — architecture and pitfalls are solid; the stack needs three phase-1 experiments (STT A/B, clone listening test, latency rig) to convert the MEDIUM items to HIGH.

### Gaps to Address
- **Chinese WER of Gemini Live text modality vs Deepgram Nova-3 on real interview audio** — resolves the single most consequential vendor choice; run in Phase 1 before wiring the pipeline
- **CN→EN clone quality A/B** (MiniMax vs Cartesia vs Fish vs ElevenLabs) — the Core Value depends on it; blinded listening test with a fixed 20-question interview set (numbers, technical terms, hesitation)
- **Network RTT from the user's region to each provider** — the ≤2s budget is region-sensitive (mainland China: +100-200ms per hop); test before committing
- **Gemini Live API exact model string + interim-transcript cadence** — docs change fast; probe in Phase 1, re-verify at Phase 2 wiring
- **E2E latency on a real Monterey 12.7 box (Intel + M1)** — CI must include a Monterey VM; modern-Mac benchmarks over-promise
- **Azure Personal Voice limited-access approval** — re-evaluate mid-milestone only if approval obtainable; do not design around it
- **Pricing deadlines:** Claude Sonnet 5 intro pricing and Fish Audio free tier both end **Aug 31, 2026** (5 days after this research) — cost model must assume post-intro pricing ($3/$15) and paid TTS
- **Tauri window hide() quirks** (no unfocus, fullscreen bugs — issues #7540/#12056) — resolved by using orderOut-equivalent APIs, but must be tested on 12.7 in Phase 4

## Sources

### Primary (HIGH confidence)
- [BlackHole official repo — GPL-3.0 license, install/FAQ](https://github.com/ExistentialAudio/BlackHole) — licensing trap, driver architecture, drift correction
- [Tauri 2 docs + WebView/macOS version mapping + issues #14200/#7540/#12056](https://v2.tauri.app/) — Safari 15.6 floor, screen-capture behavior, hide() bugs
- [Deepgram docs — endpointing/interim semantics + real-time S2S architecture](https://developers.deepgram.com/docs/understand-endpointing-interim-results) — `is_final` vs `speech_final`, cascade patterns
- [Gemini 3.5 Live Translate official announcement (Google)](https://www.gweb-uniblog-publish-prod.appspot.com/innovation-and-ai/models-and-research/gemini-models/gemini-live-3-5-translate/) — unified-model reality check
- [OpenAI realtime translation docs](https://developers.openai.com/api/docs/guides/realtime-translation) — no voice selection, 13 languages
- [Azure Speech translation / Live Interpreter](https://learn.microsoft.com/en-nz/azure/ai-services/speech-service/how-to-translate-speech) — Personal Voice limited-access
- [California Penal Code §632 + AI-notetaker exposure guide](https://www.defendmybiz.com/blog/california-two-party-consent-recording-ai-notetaker-employer-guide) — all-party consent, $5k/violation
- [Apple Developer Forums — sharingType ignored by ScreenCaptureKit on macOS 15.4+](https://developer.apple.com/forums/thread/792152) — stealth ceiling
- [cpal docs + ElevenLabs voice cloning docs](https://docs.rs/cpal) — audio I/O constraints, clone reference quality

### Secondary (MEDIUM confidence)
- [μ-Bench multilingual benchmark (Sierra)](https://sierra.ai/uk/blog/mu-bench-an-open-multilingual-transcription-benchmark) — Mandarin "5x worse" warning, Nova-3 vs Chirp-3
- [Artificial Analysis — Gemini 3.5 Flash / Flash-Lite pricing & throughput](https://artificialanalysis.ai/models/gemini-3-5-flash) — vendor table pricing
- [LiveLingo — Gemini 3.5 Live Translate latency (~2.9s first audio)](https://www.livelingo.io/guides/gemini-3-5-live-translate) — unified-model rejection
- [2026 ElevenLabs alternatives + Chinese TTS real-world tests](https://www.frankx.ai/blog/best-elevenlabs-alternatives-2026) — MiniMax/Cartesia/Fish positioning
- [2026 search-API benchmarks (Brave/Tavily/Exa)](https://fastcrw.com/blog/search-api-for-ai-agents) — web search choice; Tavily/Nebius acquisition
- [End-of-turn detection war story (18% truncation → two-budget fix)](https://dev.to/realmarcuschen/ten-days-before-launch-our-voice-agent-kept-cutting-users-off-an-end-of-turn-detection-war-story-3inf) — endpointing numbers
- [Soniox Voice AI Wiki — partial vs final; VAD vs endpointing](https://soniox.com/wiki/partial-vs-final-results) — stability gate rationale
- [DeepWiki — BlackHole technical architecture/multi-output](https://deepwiki.com/ExistentialAudio/BlackHole/4-technical-architecture) — HAL plugin internals
- [Competitor landscape: 面灵/讯飞同传/通义听悟/DeepL Voice/Zoom](https://mianlingai.com) — feature matrix, detection arms race, DeepL Voice-to-Voice threat

### Tertiary (LOW confidence — needs validation)
- [Cascaded pipeline 3.1x latency reduction (IEEE via secondary citation)](https://www.forasoft.com/learn/real-time-speech-translation-live-video) — single-source pipeline-parallelism number
- [macOS window privacy research repo](https://github.com/privateai0/macos-window-privacy-research) — community research on CGWindowList/sharingState behavior
- [Roon community — BlackHole multi-output stutter/clock drift post-mortem](https://community.roonlabs.com/t/roon-remote-on-macbook-pro-m3-shows-cores-audio-devices-instead-of-local-devices-ref-utobjg/322947/12) — drift behavior in long sessions
- Various vendor marketing claims (clone similarity %, "undetectable" claims) — treat as marketing until the Phase 1 listening test

---
*Research completed: 2026-08-26*
*Ready for roadmap: yes*
