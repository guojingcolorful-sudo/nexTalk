# Roadmap: 极言 NexTalk

## Overview

NexTalk is a real-time CN→EN interview copilot: the user speaks Chinese, hears fluent English in their own cloned voice through the meeting app, and watches bilingual subtitles plus resume-grounded AI strategy on a phone teleprompter — end-to-end latency ≤ 2s. The research dictates the build order: **contracts and UI are the cheapest things to change late and the whole upper stack is demoable on simulated audio**, so Phase 1 builds the design system, desktop UI, and phone H5 on a simulated event flow (removing the audio driver from the critical path). Phase 2 wires the real cascaded streaming pipeline (STT→translate→TTS) with a latency rig gating everything downstream. Phase 3 adds the only external dependency — the BlackHole virtual device (a process risk, not an architecture risk, deliberately isolated). Phase 4 completes stealth (real window hiding verified against actual meeting apps) and desktop helpers. Phases 5-6 layer the AI interview assistant and local recording/review assets on top of the transcript pipeline. Phase 7 hardens distribution (signing, notarization, compliance) on a clean machine. **MVP milestone: all 7 phases, 27 v1 requirements, 28 plans.**

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Foundation + Simulation Mode** - Design system, desktop dual-pane UI, phone H5 teleprompter, LAN QR sync — full event flow demoable on simulated audio
- [ ] **Phase 2: Real Cloud Pipeline + Audio Core** - Cascaded streaming STT→translate→TTS with cloned voice, ≤2s e2e, latency rig as gate
- [ ] **Phase 3: Virtual Audio Device Integration** - BlackHole install wizard, aggregate multi-output device, meeting-app audio loop
- [ ] **Phase 4: Stealth + Desktop Completion** - Cmd+Shift+H real orderOut hiding verified against real meeting apps, glossary term protection
- [ ] **Phase 5: AI Interview Assistant** - Auto question-end detection, resume-grounded streaming strategy cards in ~1.5s
- [ ] **Phase 6: Recording + Review Assets** - Consent gate, dual-track recording, transcript export, review report
- [ ] **Phase 7: Productization** - Signed/notarized distribution, clean-machine install, compliance, onboarding polish

## Phase Details

### Phase 1: Foundation + Simulation Mode
**Goal**: The complete product UI — desktop app and phone teleprompter — runs end-to-end on simulated audio, establishing the design system, LAN sync protocol, and a demoable event flow before any real pipeline exists
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: UI-01, UI-02, UI-03, SYNC-01, SYNC-02, SYNC-03, SYNC-04, SYNC-05, DSK-01, DSK-02, DSK-04
**Success Criteria** (what must be TRUE):
  1. Desktop app opens in the neobrutalism design system (4px black borders, hard-offset shadows, three functional colors, Space Grotesk): mini console 340×680 and dual-pane view 860×680 with bilingual subtitles left and AI strategy pane right
  2. Phone scans the QR code and connects via LAN WebSocket without installing anything; upper half streams bilingual subtitles with typewriter rendering, lower half shows AI strategy cards
  3. User toggles 全中/全英/双语 on phone or desktop; subtitles re-render immediately, with interviewer and user bubbles switching independently
  4. Phone screen stays awake during a session on plain `http://192.168.x.x` (wake-lock fallback engages automatically when the API is unavailable)
  5. On simulated audio, the full event flow (subtitles → strategy → cross-device sync) plays end-to-end and is demoable to a third party; all reference-HTML screens plus missing pages (setup wizard, voice enrollment, glossary, resume import, recording assets, review report) are implemented
**Plans**: 5 plans

Plans:
- [ ] 01-01: Workspace skeleton (Tauri 2 + React 19 + Vite + Tailwind v3.4 local, esbuild safari15) + neobrutalism design tokens + reference HTML 4 screens
- [ ] 01-02: Desktop UI: mini console, dual-pane view, subtitle components + missing pages
- [ ] 01-03: LAN WebSocket server + shared protocol package + QR pairing (pairing-as-auth token)
- [ ] 01-04: Phone H5 teleprompter: streaming subtitles, strategy pane, language toggle, typewriter, wake-lock fallback
- [ ] 01-05: SimSource audio source + mock STT/translate/TTS stages + full end-to-end event flow demo

**Research notes**: Run the decisive vendor experiments here (STT A/B Gemini Live vs Deepgram Nova-3 on real interview Chinese, blinded clone listening test MiniMax vs Cartesia vs Fish vs ElevenLabs, network RTT per vendor from user's region, Gemini Live text-modality probe). Results decide the Phase 2 stack wiring.
**UI hint**: yes

### Phase 2: Real Cloud Pipeline + Audio Core
**Goal**: The real mic→headphones cascaded streaming translation chain works end-to-end within the 2s budget, with cloned-voice output and honest latency instrumentation gating all downstream phases
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: AUDI-03, AUDI-04, AUDI-05, AUDI-06
**Success Criteria** (what must be TRUE):
  1. User speaks Chinese into the default microphone; within ≤2s (cold start included) they hear English in their own cloned voice through the headphones, and desktop and phone show the bilingual subtitles
  2. The latency rig reports a per-stage waterfall (mic→STT→translate→TTS→playback) with the e2e number visible; any budget breach is attributable to a stage
  3. User records 1-3 minutes to register a voice clone; before registration the pipeline works with a stock voice
  4. When the user re-speaks mid-playback, English output stops within ~100ms with no overlapping audio; device plug/unplug does not kill the session
  5. The "spoken English ⊆ committed finals" invariant holds on the test corpus — unstable STT partials are never spoken
**Plans**: 5 plans

Plans:
- [ ] 02-01: Latency rig: e2e stopwatch + per-stage waterfall timing (gate for all downstream work)
- [ ] 02-02: Real providers wired (Gemini Live text modality / Deepgram Nova-3 / Gemini Flash-Lite / MiniMax) as streaming stages with typed contracts
- [ ] 02-03: Stability gate (preview vs commit), sentence aggregation, barge-in queue, jitter buffer, provider pre-warming
- [ ] 02-04: Voice clone enrollment (1-3 min recording → clone) + stock voice fallback
- [ ] 02-05: AEC wiring + device hot-change handling + routing hygiene (distinct mic vs loopback streams)

**Research notes**: Wire cost monitoring here (interim-result billing amplification: Deepgram per-message, Gemini Live token costs). AUDI-07 glossary protects the same stage contracts but ships with the desktop glossary management in Phase 4.

### Phase 3: Virtual Audio Device Integration
**Goal**: The user's translated voice reaches the meeting app via BlackHole while the interviewer's voice reaches the headphones with zero latency and is captured for analysis — the Core Value chain completes
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: AUDI-01, AUDI-02
**Success Criteria** (what must be TRUE):
  1. The guided wizard installs the official BlackHole (no bundled GPL code), detects it via system_profiler, and shows routing status, with diagnostics covering coreaudiod restart and Security-pane deep links
  2. The aggregate multi-output device [headphones + BlackHole] is created and active: interviewer audio passes to the headphones with zero latency (hardware fan-out) while being loopback-captured for STT
  3. User sets BlackHole as the meeting app's input; the interviewer hears fluent cloned-voice English, and the user's raw Chinese is not heard
  4. The pre-session 10-second audio self-check passes (user speaks → hears own cloned English through headphones), and device hot-swaps produce clear recovery guidance
**Plans**: 3 plans

Plans:
- [ ] 03-01: BlackHole guided install wizard + detection/diagnostics (system_profiler, coreaudiod restart, Security-pane deep links, meeting-app restart guidance)
- [ ] 03-02: Aggregate multi-output device creation (coreaudio-sys, kAudioAggregateDeviceIsStackedKey) + routing guidance
- [ ] 03-03: TTS→BlackHole output + loopback capture + drift correction + pre-session 10s audio check + mic-passthrough fallback

**Research notes**: Signing/notarization workflow for the driver + clean Monterey VM test harness — decide whether basic signing CI moves here or stays at Phase 7 productization.

### Phase 4: Stealth + Desktop Completion
**Goal**: The app becomes invisible during meetings without interrupting the audio chain, and the glossary term protection completes the desktop experience
**Mode:** mvp
**Depends on**: Phase 2, Phase 3
**Requirements**: DSK-03, AUDI-07
**Success Criteria** (what must be TRUE):
  1. Cmd+Shift+H hides every window completely (real orderOut, windows leave the layer); the audio engine keeps running; the shortcut restores the windows
  2. Actual Zoom/Teams full-screen share recordings show zero NexTalk pixels — including Dock, menu bar, tray, and tooltips
  3. The app does not appear in the Dock or Cmd+Tab (accessory activation policy)
  4. User adds hot words (K8s, backpressure, 幂等性) in the glossary; STT and translations then preserve these terms verbatim
**Plans**: 3 plans

Plans:
- [ ] 04-01: Real orderOut hiding via Cmd+Shift+H + accessory activation policy + permission pre-grant checklist (Screen Recording/Mic/Accessibility at first launch)
- [ ] 04-02: Stealth verification: actual Zoom/Teams full-screen share recordings show zero NexTalk pixels (incl. Dock/tray/tooltips)
- [ ] 04-03: Glossary management wiring: term table UI (Phase 1 page) → STT/MT term protection in the Phase 2 pipeline stages

### Phase 5: AI Interview Assistant
**Goal**: After the interviewer finishes a question, the copilot automatically produces resume-grounded streaming strategy cards in ~1.5s — no manual trigger, and misfires are cheap to dismiss
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: COPT-01, COPT-02, COPT-03, COPT-04
**Success Criteria** (what must be TRUE):
  1. Before the interview, the user imports a resume (PDF/Word) and a question bank; both are pre-indexed and ready before the interview starts
  2. Question-end is auto-detected (dual silence budgets 550ms/1300ms + LLM completeness): on ≥20 real-interview replays, misfire <5% and truncation <3%
  3. Within ~1.5s of question end, streaming strategy cards appear in the desktop right pane and phone bottom pane — bullets grounded in resume experience, question bank, and live web search, rendered progressively (not full-text ghostwriting)
  4. User can dismiss a misfired strategy card instantly; the main translation flow is unaffected
**Plans**: 4 plans

Plans:
- [ ] 05-01: Question-end detection (VAD → endpoint → LLM completeness, two silence budgets, hard max cap) + replay/eval harness (≥20 real interviews)
- [ ] 05-02: Resume (PDF/Word) + question bank import, pre-indexing, embedded RAG-lite KB
- [ ] 05-03: Strategy generation: streaming bullet outline + reference phrasing in user's own words, ~1.5s, optional Brave web search
- [ ] 05-04: Strategy cards to desktop right pane + phone bottom pane + dismiss/retry on misfire

**Research notes**: Endpointing eval methodology (replay corpus construction), PDF/Word parsing pipeline, parameter tuning per interviewer speaking style.

### Phase 6: Recording + Review Assets
**Goal**: After the meeting, the user owns complete local assets — dual-track recording, bilingual transcript, and a review report — produced under an explicit, default-OFF consent gate
**Mode:** mvp
**Depends on**: Phase 5
**Requirements**: REC-01, REC-02, REC-03, REC-04
**Success Criteria** (what must be TRUE):
  1. Recording defaults OFF; first enablement shows a blocking consent gate with copy-pasteable disclosure; no recording without consent
  2. The session is recorded locally on two tracks (user / interviewer, Opus); delete removes all data on demand
  3. User exports a bilingual timestamped transcript in SRT, Markdown, or Word
  4. User generates a review report with Action Items, sentiment, key concerns, and per-question replay
**Plans**: 4 plans

Plans:
- [ ] 06-01: Consent gate (default OFF, disclosure copy, per-session consent record, delete-on-demand)
- [ ] 06-02: Dual-track local recording (user/interviewer tracks, Opus, timestamp-aligned) + delete-on-demand
- [ ] 06-03: Bilingual transcript export (SRT/Markdown/Word with timestamps)
- [ ] 06-04: Review report generation (Action Items, sentiment, key concerns, per-question replay)

**Research notes**: Consent/legal research per target market (CA §632-class all-party states, PIPL) must be scheduled before this phase ships.

### Phase 7: Productization
**Goal**: The app installs and runs on a clean machine as a signed, notarized release with compliance and onboarding complete — escaping the "works on my Mac" trap
**Mode:** mvp
**Depends on**: Phase 6
**Requirements**: (none — distribution hardening phase per research; all 27 v1 requirements already mapped to Phases 1-6)
**Success Criteria** (what must be TRUE):
  1. Signed DMG installs and launches on a clean macOS 12.7 VM (Intel + M1); Gatekeeper passes (`spctl -a -v`)
  2. Fully offline bundle: zero network requests at runtime — fonts, icons, and styles are all local (no CDN)
  3. Compliance checklist complete: legal review per market (CA §632-class, PIPL), retention-zero vendor terms, regional recording toggles
  4. First-run onboarding (permissions, driver wizard, voice enrollment) completes without external support
**Plans**: 4 plans

Plans:
- [ ] 07-01: Signing + notarization CI (every build verified with spctl, staple)
- [ ] 07-02: Clean macOS 12.7 VM install tests (Intel + M1), offline bundle verification (zero CDN requests)
- [ ] 07-03: Compliance checklist: legal review per market, retention-zero vendor terms, regional recording toggles
- [ ] 07-04: Onboarding polish: permission pre-grant flow, wizard flow, bundled assets verification

**Research notes**: Pricing survey (must undercut $89-148/mo interview copilots; positioned as a language tool); performance traps on old Intel Macs (CPU starvation) and API cost caps.

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation + Simulation Mode | 0/5 | Not started | - |
| 2. Real Cloud Pipeline + Audio Core | 0/5 | Not started | - |
| 3. Virtual Audio Device Integration | 0/3 | Not started | - |
| 4. Stealth + Desktop Completion | 0/3 | Not started | - |
| 5. AI Interview Assistant | 0/4 | Not started | - |
| 6. Recording + Review Assets | 0/4 | Not started | - |
| 7. Productization | 0/4 | Not started | - |
