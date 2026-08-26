# Feature Research

**Domain:** Real-time AI voice translation (simultaneous interpretation) + AI interview copilot — "own-voice" virtual-mic output for technical interviews (CN speaker → fluent EN interview)
**Researched:** 2026-08-26
**Confidence:** MEDIUM-HIGH (competitor features verified via web research of vendor sites, reviews, and 2025-2026 comparison articles; product claims themselves are vendor-stated and often unverifiable)

## Feature Landscape

### Table Stakes (Users Expect These)

Features every competitor in both adjacent markets (translation + interview copilot) ships. Missing these = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Low-latency cascaded streaming (STT→MT→TTS, ≤2s first response) | iFlytek 同传 already ships 2s 首响; DeepL demo 2-3s EN→KR; Zoom Voice Translator, Pinch (<1.5s), tiyov (1.5-2.5s) all set the bar | HIGH | Cascade must start MT mid-sentence and TTS mid-translation (PROJECT.md constraint). Nobody wins on latency alone anymore — it is hygiene, not differentiation |
| Real-time bilingual subtitles (desktop) | Every product: iFlytek 悬浮字幕, DeepL Voice captions, 通义听悟 双语字幕, Otter/Fireflies live captions, Timekettle on-screen text | MEDIUM | Mid-result time-stamps + mid-call language switching are now expected (通义听悟 added both in 2025) |
| Transcript export (bilingual, timestamped) | Otter, Fireflies, DeepL Voice (translated transcript download, Jul 2025), 通义听悟 (Word/PDF/SRT) all ship it | MEDIUM | SRT + Markdown + Word export expected. Local-only export is a differentiator twist (see below) |
| Meeting recording (local) | Otter/Fireflies record everything; iFlytek 会议记录; 通义听悟 records | MEDIUM | v1 does dual-track local recording (user vs interviewer) — competitor standard is single mixed track |
| Speaker separation / diarization | iFlytek (讲话人分离), 通义听悟 (区分发言人), Otter/Fireflies (speaker labels) | MEDIUM | v1 only needs 2 speakers (user vs interviewer) — much easier than N-speaker diarization |
| Post-meeting AI summary / action items | Otter 30s summary + action items, Fireflies action items + sentiment, 通义听悟 11 项纪要能力, Krisp notes | MEDIUM | v1's 复盘报告 (action items, sentiment, key concerns) maps here but must be interview-shaped, not meeting-shaped |
| Question detection → auto-trigger AI assist | Final Round AI, Sensei (question detection <1s), 面灵AI, Interviews.chat lack it but all real copilots have it | HIGH | 静默检测 + LLM 问句完整性判断 (PROJECT.md). ⚠️ 断句误判 is the known core risk; 面灵/others struggle with follow-up questions |
| Resume import & grounding of answers | Final Round (resume+JD), Sensei (resume+stories), Huru (JD→questions) | MEDIUM | PDF/Word import; answers must cite real resume experience or they sound generic (reviewers dinged Sensei for this) |
| Stealth / discreteness features | Final Round (stealth overlay), Sensei (second-screen advice, "undetectable" claims), 面灵 (anti-screen-capture desktop client) | HIGH | Anti-screen-capture on desktop is a real category (面灵 proves it). 防抓屏 = window orderOut, not transparency (PROJECT.md decision) |
| Glossary / hot-word customization | iFlytek 热词优化/强制替换, Loquora glossary, 通义听悟 术语 | MEDIUM | Tech-interview terms (K8s, backpressure, idempotency) MUST be pinnable or STT/MT will garble them |
| Voice enrollment (own-voice clone setup) | Loquora (60s), VoiceBridge (30s), 讯飞 声音复刻 (one sentence, 90% 相似度), LOVO (1-3 min) | MEDIUM | Users expect quick enrollment; 1-3 min clean recording is the industry norm |
| Microphone/meeting-app setup guide (virtual device wizard) | Pinch, Loquora, Krisp all have install/setup flows; iFlytek has 会前技术指导 | MEDIUM | Zoom/Teams/腾讯会议 mic selection is the #1 support question in this category — v1 needs a guided wizard (design spec already includes it) |
| Mobile/web companion or second screen | Timekettle X1 presentation mode (QR → phone web subtitles), Sensei second-screen guide, FinalRound overlay | MEDIUM | Timekettle proves the QR→H5 pattern; interview teleprompter on phone is rare (see differentiators) |

### Differentiators (Competitive Advantage)

Features that set the product apart. The three listed in the milestone context (own-voice output, stealth teleprompter, resume-grounded strategy) are genuinely scarce in combination — but see the threat analysis under Sources/Implications.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Own-voice output via virtual mic into Zoom/Teams/腾讯会议** | User speaks Chinese; interviewer hears *the user's own cloned voice* speaking fluent English — no "robot interpreter" tell, seamless identity. Competitors: Pinch (macOS, Jan 2026, 50+ langs), Loquora, Krisp voice translation do the chain but **none target interviews**; DeepL Voice-to-Voice announced Oct 2025 (dubbing-grade, preserves voice) but not yet available; iFlytek 声音复刻 outputs through *their* hardware, not your meeting mic | HIGH | This is the core value chain (PROJECT.md Core Value). ⚠️ Windows of exclusivity: ~12-24 months before DeepL/Zoom commodity it. Must pair with interview vertical |
| **Mobile H5 teleprompter via LAN QR pairing** (top half bilingual subtitles, bottom half AI strategy cards; 全中/全英/双语 toggle) | No competitor has interview teleprompter on a *second device*. Timekettle has QR→web subtitles (meetings only); Sensei/FinalRound use screen overlays (detectable); 面灵 hides windows but user still looks at the same screen the interviewer can see. A phone under the desk/screen is undetectable by eye-tracking proctoring (they track the *webcam* screen, not your phone) | MEDIUM | Best stealth story in the category. Requires desktop↔phone sync (WebSocket), ~1.5s strategy push latency |
| **Resume + question-bank grounded strategy auto-triggered on question end** (bullets + draft answer in ~1.5s, optional live web search) | Final Round/Sensei ground on resume but generate full answers (cheating perception, latency too slow to read aloud naturally). NexTalk: strategy *bullets* + draft that the user answers in *their own words* — compatible with the language-bridge positioning | HIGH | 1.5s budget demands pre-indexed resume (embedded, RAG-lite) + pre-cached question bank + streaming LLM output. Live web search (Tavily-class) is optional per question |
| **Anti-screen-capture stealth mode (Cmd+Shift+H real hide, audio keeps running)** | For full-screen-share interviews (screen share → interviewer sees desktop). 面灵 (Windows/macOS desktop client) is the only competitor with anti-screen-capture for interview windows; FinalRound/Sensei overlays get captured. True window orderOut (not transparency — capture engines read window buffers) | MEDIUM | macOS: orderOut + MenuBarExtra + audio pipeline lives in a separate process so it survives UI hide |
| **Language-bridge positioning vs answer-generation** (user's own words translated; AI assists structure, not content) | The 2026 market reality: real-time answer assistance detection rates rose 2% → 10%+ (Karat data), eye-tracking every 10s (牛客), dual-camera proctoring (北森). Full-answer tools (FinalRound/Sensei/面灵) are an escalating-cheating arms race. A translation tool is defensible as language access — like using an interpreter, not a ghostwriter | LOW | Strategic differentiator, not a feature. Shapes marketing, pricing, and which anti-features to refuse (see below) |
| **Local-only privacy (no accounts, no cloud storage, recordings/transcripts stay on device)** | Otter/Fireflies/通义听悟/iFlytek are all cloud SaaS (audio uploads). Timekettle X1 advertises "no cloud, secure wired export" — proves local-privacy sells. For interviews, resume + voice clone are deeply personal data; local-only is a trust advantage | MEDIUM | v1 is pure local (PROJECT.md). AI APIs still see audio/transcript segments — minimize exposure in vendor selection |
| Post-interview 复盘报告 (interview-shaped: action items, sentiment, key concerns, per-question replay) | Otter/Fireflies summarize *meetings*; FinalRound has performance analytics (mock only). An interview-specific review report (which questions were weak, emotional stress points, resume gaps) is unclaimed territory | MEDIUM | Built on local recording + transcript; sentiment + concern extraction is standard LLM work |
| Delivery-quality feedback (Yoodli-style: filler words, pacing) integrated into the review report | Yoodli ($15-17/mo) built a whole product on this; it's especially valued by non-native speakers — NexTalk's exact audience | MEDIUM | v2 candidate; trivially computable from the local transcript |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Full-answer reading mode (AI writes the answer, user reads it aloud in Chinese, system voices it) | "Zero English ability" users want total substitution | (1) Detection risk spikes (answer quality anomalies, latency patterns — 10%+ detection in 2026); (2) follow-up questions expose it immediately; (3) turns the product into a cheating tool, regulatory gray zone (2026 银行 AI 考官 vs AI 考生 coverage; lawyers call it 求职失信); (4) reads robotic, pacing analysis catches it | Strategy bullets + draft answer in *user's own words*; keep the user in the loop. The translation chain already makes Chinese natural |
| Undetectability from proctoring software (evading 牛客/北森 eye-tracking, dual-camera) | "I don't want to get caught" | Escalating arms race you cannot win; legal/ethical exposure; product becomes unmarketable if "anti-detection" is the headline | Position as language access (translation), not answer assistance; stealth teleprompter on a phone is naturally undetectable because the interviewer's screen-share/proctor sees only the webcam feed — no special "evasion" tech needed |
| Two-way automatic voice output (also voice-clone the *interviewer's* translated voice into Chinese headphones) | Symmetric immersion | Interviewer's voice is another person's biometric data — consent/legal risk (PROJECT.md flags recording consent already); doubles TTS cost; 原声透传 (zero-latency pass-through of interviewer's English) is already decided and better | Keep 英→中 as subtitles only (mobile H5); pass through original audio at zero latency |
| v1 multi-language pairs (JP/KR/DE...) | "Global product" | Each language pair needs STT + MT + *voice clone* quality — cost grows linearly (PROJECT.md: TTS 音色克隆成本随语言对线性增长) | CN↔EN only for v1; engine is pair-agnostic, add pairs post-validation |
| Windows support in v1 | Double the market | WASAPI/VB-Audio driver work is a second audio core (PROJECT.md decision); macOS-only keeps the audio core reviewable | macOS v1; Windows after audio core validates |
| Cloud backend / accounts / sync | "Access my transcripts anywhere" | v1 is deliberately pure-local (PROJECT.md); backend is infrastructure, not a feature — steals months | Local files with clear export; re-evaluate after product validation |
| Video/avatar lip-sync or AI interviewer avatar features | Marketing shine | Different product (avatar industry); Zoom AI Companion 3.0 already does avatars; no interview value | None — skip entirely |
| Coding-interview platform support (HackerRank/LeetCode/牛客 overlay hints) | Big interview-assistant segment | (1) Strong cheating perception — coding support is the most flagged feature by employers; (2) FinalRound/Sensei already own it; (3) conflicts with the language-bridge positioning | Out of scope v1; interviewers of CN speakers in EN coding interviews still speak English — the *translation* value applies, the *hints* value does not |
| Mock-interview practice mode with AI interviewer | Standard in every copilot (FinalRound, Yoodli, Huru, Sensei) | Users expect it — but it is a *second product* (practice flows, video feedback, scoring) and Yoodli/Huru own it at $15-25/mo | v2: reuse the same engine; record practice sessions through the same pipeline (cheap once v1 exists) |
| 机考/笔试 anti-cheat features, screen-sharing of strategy cards | Direct sales push from interview-tool buyers | Pure cheating vertical; regulatory exposure; brand damage | Don't build; the phone teleprompter + translation story is enough |

## Feature Dependencies

```
Own-voice virtual-mic output (core chain)
    ├──requires──> Virtual audio device (BlackHole/CoreAudio) + driver install wizard
    ├──requires──> Voice clone enrollment (user's voice sample → cloned TTS voice)
    ├──requires──> Cascaded streaming pipeline: STT → MT → TTS (mid-sentence triggers)
    └──requires──> AEC/echo handling (virtual mic path must not re-capture its own output)

Mobile H5 teleprompter
    ├──requires──> Desktop engine (subtitles + strategy data source)
    ├──requires──> LAN pairing (QR code → WebSocket/SSE sync)
    └──enhances──> Stealth: phone screen is the only UI the user needs mid-interview

AI strategy engine
    ├──requires──> Question detection (interviewer STT + silence + LLM completeness check)
    ├──requires──> Resume/question-bank import & pre-indexing (must be ready BEFORE interview starts)
    ├──enhances──> Optional live web search (per-question)
    └──feeds──> Strategy cards (bullets + draft answer, ~1.5s)

Anti-screen-capture stealth (Cmd+Shift+H)
    ├──requires──> UI (Tauri) and audio engine in separable processes
    └──conflicts──> On-screen teleprompter overlays (Sensei/FinalRound pattern) — cannot have both; phone teleprompter wins

Local dual-track recording + transcript export
    ├──requires──> Audio capture of BOTH channels (mic-in user, virtual mic out / interviewer pass-through)
    └──feeds──> 复盘报告 (review report: action items, sentiment, key concerns)

复盘报告 ──requires──> Recording + transcript + strategy event log (which question triggered what)
```

### Dependency Notes

- **Own-voice output requires the whole audio core first:** virtual device → enrollment → cascade. Everything downstream (teleprompter, recording, report) consumes the same audio graph — the audio core is phase 1, no parallel paths.
- **Strategy engine requires pre-interview indexing:** resume/question-bank import must complete before the interview starts; 1.5s answer budget is impossible with on-the-fly full-document grounding. This is a setup-flow feature, not a mid-interview one.
- **Anti-capture conflicts with on-screen overlays:** the anti-screen-capture story only holds if all sensitive UI lives on the *phone*. If strategy cards also render on the desktop (dual-pane view), the desktop must be hideable via Cmd+Shift+H before any screen share. Design decision: desktop shows subtitles + strategy; user can share the desktop and rely on the phone, or hide desktop and rely on phone entirely.
- **Question detection depends on interviewer STT quality:** in a virtual-mic setup the interviewer's voice arrives through the meeting app — the pipeline must capture it as an input source separate from the user's mic. This is also the source for the recording's interviewer track.

## MVP Definition

### Launch With (v1)

- [ ] Own-voice virtual-mic output chain (STT→MT→TTS cascade, ≤2s, CN→EN, cloned user voice) — the Core Value; nothing validates without it
- [ ] Virtual audio device install + guided mic-setup wizard (Zoom/Teams/腾讯会议) — table-stakes onboarding
- [ ] Bilingual desktop subtitles with 全中/全英/双语 toggle — table stakes
- [ ] Mobile H5 teleprompter (LAN QR pairing; subtitles top, strategy bottom) — the stealth + UX differentiator, cheap once the desktop engine exists
- [ ] Resume import + question-bank import with pre-indexing — differentiator core
- [ ] Question detection → auto-triggered strategy cards (~1.5s bullets + draft) — differentiator core
- [ ] Cmd+Shift+H anti-capture hide — differentiator (the project's "防抓屏" requirement)
- [ ] Local dual-track recording + bilingual transcript export — table stakes (privacy angle)
- [ ] 复盘报告 (action items, sentiment, key concerns) — differentiator (cheap, LLM over local transcript)

### Add After Validation (v1.x)

- [ ] Glossary/hot-words pinning (tech terms: K8s, backpressure...) — table-stakes hygiene, obvious first add; measure mis-translation complaints first
- [ ] Mock-interview practice mode — Yoodli/Huru prove demand; engine already exists, needs practice UI + AI interviewer flow
- [ ] Delivery-quality report (fillers, pacing) in 复盘 — Yoodli-style; computed from local transcript
- [ ] Voice-swap / try-before-clone (use a stock voice until user enrolls) — reduces enrollment friction in the wizard

### Future Consideration (v2+)

- [ ] Windows client (WASAPI/VB-Audio) — after audio core validated (PROJECT.md)
- [ ] More language pairs (JP/KR/DE) — engine is pair-agnostic; cost is voice clones
- [ ] Optional cloud sync/accounts — only if users demand it post-validation; v1 is pure-local by decision
- [ ] 商务谈判 / multilingual-meeting scenarios — shared engine, different prompt layer (PROJECT.md out-of-scope note)

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Own-voice virtual-mic output (core chain) | HIGH | HIGH | P1 |
| Cascaded streaming ≤2s (STT→MT→TTS) | HIGH | HIGH | P1 |
| Virtual device install + setup wizard | HIGH | MEDIUM | P1 |
| Bilingual desktop subtitles + toggle | HIGH | MEDIUM | P1 |
| Mobile H5 teleprompter (QR pairing) | HIGH | MEDIUM | P1 |
| Resume/question-bank import + pre-index | HIGH | MEDIUM | P1 |
| Question detection → auto strategy cards | HIGH | HIGH | P1 |
| Cmd+Shift+H anti-capture hide | HIGH | MEDIUM | P1 |
| Local dual-track recording + transcript export | MEDIUM | MEDIUM | P1 |
| 复盘报告 (action items/sentiment/concerns) | MEDIUM | LOW | P1 |
| Glossary/hot-words | MEDIUM | LOW | P2 |
| Mock-interview practice mode | MEDIUM | MEDIUM | P2 |
| Delivery-quality feedback (fillers/pacing) | MEDIUM | LOW | P2 |
| Live web search in strategy engine | LOW | MEDIUM | P2 |
| Windows client | MEDIUM | HIGH | P3 |
| Extra language pairs | MEDIUM | HIGH | P3 |
| Cloud sync/accounts | LOW | HIGH | P3 (revisit post-validation) |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | iFlytek 同传 | DeepL Voice | Zoom/Teams native | Otter/Fireflies/Krisp | FinalRound/Sensei/面灵 | Pinch/Loquora/tiyov | NexTalk |
|---------|--------------|-------------|-------------------|----------------------|----------------------|---------------------|---------|
| Real-time voice translation (own voice out) | 声音复刻 via own hardware, not your meeting mic | Voice-to-Voice announced Oct 2025, not shipped (dubbing-grade) | Zoom Voice Translator: 5 langs, consecutive only, Business+ | Krisp: voice translation + accent conversion (generic voice, not cloned — MEDIUM confidence) | — | Yes: virtual mic + cloned voice, 50+ langs | Yes — core chain, via virtual mic |
| 2s-class latency | Yes (2s 首响) | Yes (2-3s demo) | Consecutive (slower) | — | — | Yes (1.5-2.5s) | Yes (hard requirement ≤2s) |
| Bilingual subtitles | Yes (悬浮字幕, share links) | Yes (17 spoken / 35 display langs) | Yes (35+ caption langs) | Yes (captions, EN-focused) | — | Yes (incoming captions) | Yes (desktop + phone) |
| Interview-focused (resume + Q&A strategy) | No (conferences) | No (meetings) | No | No | Yes (FinalRound resume+JD, Sensei resume+stories, 面灵) | No | Yes — with translation |
| Question detection auto-trigger | No | No | No | No | Yes (FinalRound, Sensei, 面灵) | No | Yes (silence + LLM) |
| Mobile teleprompter (QR/H5) | No | No | No | No | Sensei: second-screen advice only | No | Yes — the stealth teleprompter |
| Anti-screen-capture stealth | No | No | No | No | 面灵: desktop anti-screen-capture; FinalRound: stealth overlay; Sensei: Chrome ext (visible to proctoring) | No | Yes (Cmd+Shift+H orderOut + phone UI) |
| Recording + transcript + summary | Yes (会议纪要) | Yes (translated transcript) | Captions only (2026: no caption download) | Yes (Otter/Fireflies, action items, sentiment) | Analytics (mock only) | Transcripts (Pinch) | Yes, local-only + interview 复盘 |
| Local/privacy-first | No (cloud) | No (cloud, enterprise) | No | No | No (cloud) | Mixed (tiyov OSS local) | Yes — pure local v1 |
| Interview-specific review report | No | No | No | Meeting summaries only | Partial (FinalRound analytics) | No | Yes (action items, sentiment, concerns) |
| Voice-clone enrollment | One-sentence 复刻 (90% 相似) | N/A (in dev) | N/A | No | No | 30-60s enrollment | 1-3 min enrollment wizard |
| Price point | Hardware + SaaS (enterprise) | Enterprise tiers | $7-12/user/mo add-on | $8-18/mo | $89-148/mo (copilots), 面灵 $4-30/session | Pinch ~$20-30/mo (LOW confidence); tiyov OSS free | TBD — must undercut copilots (they own the "answer" value), positioned as language tool |

## Sources

- Interview copilots: Final Round AI vs Yoodli (finalroundai.com), Best AI Interview Coach 2026 (finalroundai.com), Interview Coach vs Copilot 2026 (revarta.com), Sensei Copilot guide (skywork.ai, senseicopilot.com), Interviews.chat via hrfuture.net, SaasHub FinalRound vs Huru — MEDIUM confidence (vendor claims dominate)
- 讯飞同传: Baidu Baike entry, geekpark.net 同传大模型升级 (2025-10), cnstock.com 中英首响 2 秒 — MEDIUM-HIGH (first-party + press)
- 通义听悟: aliyun.com release notes (2025 timeline: 04.30/05.30/10.30/11.03 updates), 36氪 review — HIGH (official release notes)
- DeepL Voice: deepl.com press releases + blog (Jul 2025 expansion, Oct 2025 Voice-to-Voice announcement, Zoom integration Sep 2025), speechtechmag.com — HIGH (first-party)
- Zoom AI Companion 3.0 / Voice Translator: it-boltwise.de, jotme.io, intermind.com — MEDIUM (press coverage of Zoomtopia 2025)
- Otter/Fireflies/Krisp: cybernews.com, openhelm.ai 500-meeting test, superagi.com, techpoint.africa — MEDIUM (independent 2025 tests)
- Phone call translation: Samsung official support pages (Live Translate, 20 languages) — MEDIUM-HIGH
- Timekettle X1: xataka.com review, Amazon listing, price.com.hk — MEDIUM
- LOVO: latestly.ai, maestra.ai comparisons, lovo.ai marketing — MEDIUM
- Virtual-mic voice-clone tools: startpinch.com research page, producthunt.com/loquora, github.com (tiyov, saymo), lablab.ai (Conduit), ElevenLabs hack VoiceBridge — MEDIUM (mostly launch materials + OSS repos)
- 面灵AI + 2026 detection landscape: mianlingai.com (detection risk 2026, anti-cheat guide, tool comparisons), 21jingji.com 2026-04 bank AI-interviewer/AI-candidate report, cnblogs.com 5-tool comparison — MEDIUM (vendor blog + press; detection-rate numbers vendor/employer-stated)

**Threat assessment (why speed matters):**
- DeepL Voice-to-Voice (announced 2025-10, "dubbing-grade," preserves speaker's voice) is the single biggest future threat to the own-voice differentiator; expect availability within ~12 months. It is meeting-oriented, not interview-oriented — NexTalk's moat is the interview vertical, not the audio chain.
- Zoom Voice Translator (beta Mar-Apr 2026, 5 languages, consecutive, paid tiers) commoditizes basic cross-language speech *inside* Zoom — but consecutive-only and 5-language means the seamless-conversation + interview use case stays open.
- The virtual-mic + clone pattern is proven (Pinch Jan 2026, Loquora, Krisp, OSS tiyov) — de-risks the audio core but means it is NOT patentable/unique; differentiate on workflow.
- The interview-copilot market is saturated and fighting detection (10%+ detection, eye-tracking, dual-camera); the "language bridge" positioning is the clean escape hatch — NexTalk must not drift into answer-ghostwriting territory.

---
*Feature research for: 极言 NexTalk — real-time AI voice-translation interview copilot*
*Researched: 2026-08-26*
