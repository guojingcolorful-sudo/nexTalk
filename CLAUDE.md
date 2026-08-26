<!-- GSD:project-start source:PROJECT.md -->

## Project

**极言 NexTalk — AI 跨语言实时面试辅助系统**

极言（NexTalk）是一款面向中文母语用户的"超级沟通外挂"：在跨国技术面试中，用户以中文自然表达，系统实时识别、翻译并克隆用户本人音色，向面试官输出无缝衔接的流利英文；同时手机网页端作为"提词器"，实时滚动双语字幕并给出基于用户简历与题库的 AI 回答策略。v1 聚焦技术面试场景（中↔英），形态为本地单机桌面应用（Tauri + Rust，macOS 优先）+ 手机 H5 跨端协同 + 云端 AI API 调用。

**Core Value:** 让用户以母语思考、以本人音色讲出地道英文——面试官听到的是无缝衔接的英文回答，用户看到的是实时字幕与回答策略，端到端延迟 ≤ 2 秒。如果其他都失败，这条链路必须成立。

### Constraints

- **Tech stack**: 桌面端 Tauri + Rust；手机端 H5（React/Vue + WebSocket/SSE）；AI 能力全部走云端 API（本地不跑模型）
- **Performance**: 端到端语音延迟 ≤ 2s；AI 策略提示 ~1.5s；必须级联流式（禁止整句串行）
- **Compatibility**: macOS 12.7 Monterey 可用；低延迟音频缓冲区
- **Design**: 设计规范 V1.0 已确认——新粗野主义、Space Grotesk、三功能色语义（绿=用户/发音、黄=AI 策略、蓝=翻译/系统提示）；参考 HTML 为视觉基准
- **Privacy**: 纯本地存储（录音、逐字稿、复盘报告均不传自有云端）；AI API 调用时注意最小化数据暴露

<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->

## Technology Stack

## Executive Verdict: 2026 Vendor Re-Evaluation

| Pipeline Stage | 2024-25 Pick (requirement doc) | 2026 Recommendation | Why (latency / cost / quality) |
|---|---|---|---|
| STT — user's Chinese (feeds clone TTS) | Deepgram API | **Gemini Live API, text modality** (`gemini-3.1-flash-live`) | Chirp-3-class Mandarin accuracy (μ-Bench: Nova-3 trails on Mandarin; "Mandarin accuracy can be 5x worse than English"); fuses STT + translation into one WebSocket hop (fewer round-trips than 3-hop cascade) |
| STT — interviewer's English (subtitles/copilot) | (same Deepgram) | **Deepgram Nova-3** (streaming) | Still the latency leader (sub-300ms, ~$0.0048/min) and English is its strongest language; keeps a second independent vendor path |
| Translation (C→EN incremental) | GPT-4o-mini | **Gemini 3.5 Flash-Lite** (or 3.5 Flash for quality) | GPT-4o-mini is legacy: ~4x slower throughput, worse quality. Flash-Lite $0.30/$2.50 per MTok, ~350 t/s, ~0.1-0.2s TTFT |
| Copilot agent (resume-grounded strategy) | GPT-4o / Claude 3.5 Sonnet | **Gemini 3.5 Flash** primary; **Claude Sonnet 5** quality alt; **Haiku 4.5** for micro-tasks | Gemini 3.5 Flash: 1M ctx (resume+题库 fit with 90% prompt-cache discount), 280+ t/s for the ~1.5s strategy budget, strong Chinese. Claude Sonnet 5: best-in-class tool-use/agentic (Aug 2026 Agent Arena #1-#3 are Anthropic) |
| TTS voice cloning (user's voice, C→EN) | ElevenLabs | **MiniMax Speech 2.6 Turbo** primary; **Cartesia Sonic 3.5** latency alt; **Fish Audio S2.1 Pro** budget alt | ElevenLabs: known Mandarin weaknesses (tone errors, weak emotion control in Chinese) + ~$300/M chars (5-10x alternatives) + v3 not realtime-optimized. MiniMax: best Chinese cross-lingual cloning (10s clone, ~99% similarity, sub-250ms, ~$10-40/M chars) |
| Web search (copilot grounding) | Tavily API | **Brave Search API** (+ LLM Context API) | Tavily acquired by Nebius (Feb 2026) → roadmap uncertainty; Brave fastest (669ms), largest independent index, BrowseComp 38.3% vs Tavily 19.3%, $5/1K queries |
| Unified S2S option | Why not primary |
|---|---|
| **Gemini 3.5 Live Translate** (Jun 2026, `gemini-3.5-live-translate-preview`) | Preserves speaker characteristics but is NOT a voice clone; independent LiveLingo benchmark: ~2.9s median first-audio (over the 2s budget); known voice-instability (voice shifts, gender changes); audio-only, no tools |
| **gpt-realtime-translate** (May 2026, $0.034/min) | **No voice selection at all** — cannot do the user's cloned voice; only 13 output languages (Polish/Tagalog unsupported); no tools/system prompts |
| **Azure Speech Live Interpreter + Personal Voice** | Closest single-API match (0.4-0.7s time-to-heard, personal voice on-the-fly) but requires Microsoft "Personal Voice" limited-access **approval** (risk) + vendor lock-in + no mid-stream text control |

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Tauri | 2.11.0 (Apr 2026) | Desktop shell (Rust core + WKWebView UI) | Requirement doc's own pick; Rust zero-copy audio handling, ~10MB bundle, runs on macOS 12.7 (WKWebView, min macOS 10.13+). Tauri 3 still on roadmap — stick with stable 2.x |
| Rust | current stable (MSRV ≥1.77.2) | Audio pipeline, WS clients, LAN server | Only serious language for low-copy low-latency audio graph on a desktop app |
| React | 19.x + TypeScript + Vite 7 (target `safari15`) | Desktop webview UI + mobile H5 teleprompter (one shared app) | Reference HTML is React; single codebase for both surfaces; esbuild `safari15` target matches macOS 12.7 WebKit |
| Tailwind CSS | **v3.4.x** (NOT v4) | Neobrutalism styling utilities | macOS 12.7 ships Safari 15.6 WKWebView; Tailwind v4 relies on modern CSS (`@property`, `color-mix`, `oklch`) requiring Safari 16.4+. v3.4 is safe; plain CSS custom properties per design tokens still used for palette |
| cpal | latest (0.15+) | CoreAudio capture/output (mic + BlackHole virtual device) | Standard Rust audio I/O; reads/writes BlackHole 2ch at 48kHz |
| webrtc-audio-processing | ~2.0 (2.1.0, May 2026) | AEC3 echo cancellation + noise suppression + AGC + VAD | The exact WebRTC AEC stack the requirement doc calls for, as a maintained Rust crate; pin with `~` (version tracks PulseAudio's WebRTC snapshot, not semver) |
| BlackHole | 0.7.1 (Jul 2026) | Virtual audio driver (mic/speaker takeover) | De-facto standard, GPL-3.0, zero-latency loopback, macOS 10.10+ incl. Monterey; install via bundled pkg/Homebrew — custom driver build deferred (signing/notarization burden) |
| silero-vad-rs | latest | Voice-activity detection for question-end detection | Small ONNX VAD, fast on CPU; pairs with webrtc VAD for silence+turn detection to trigger the copilot |
| tokio-tungstenite | latest | WebSocket: Gemini Live API, Deepgram, mobile H5 | All three streaming endpoints are WSS; one client library for everything |
| axum | latest | LAN server: H5 hosting + QR pairing endpoint | Lightweight tokio-native HTTP server inside the desktop app |
| qrcode (rust) | latest | LAN pairing QR generation | No deps on external services; QR encodes `ws://<lan-ip>:<port>` |
| reqwest | latest | Streaming REST for TTS (MiniMax/Cartesia/Fish) | HTTP chunked/SSE consumption of TTS streams; async-friendly |
| rubato | latest | Sample-rate resampling (44.1k↔48k, STT 16k) | Needed at every provider boundary; fast polyphase resampler |
| hound | latest | Dual-track WAV recording (user track / interviewer track) | Zero-dep WAV writer for post-meeting assets; 32-bit float, 48kHz |

### AI Pipeline (Cloud APIs — the 2026 re-evaluation)

| Stage | Primary (2026) | Model/Endpoint | Cost (approx) | Confidence |
|---|---|---|---|---|
| STT + translate (user path) | **Gemini Live API, text modality** | `gemini-3.1-flash-live` (Mar 2026, 90+ languages) | token-based, cents/session | MEDIUM (pattern confirmed by 2026 dev practice; verify exact model string + interim-transcript behavior in phase 1) |
| STT (interviewer path) | **Deepgram** | `nova-3` streaming, WSS | ~$0.0048/min (~$0.29/hr) | HIGH (official + independent 2026 benchmarks) |
| Translation (standalone, interviewer path & re-translate) | **Gemini** | `gemini-3.5-flash-lite` ($0.30/$2.50 per MTok, ~350 t/s); upgrade `gemini-3.5-flash` ($1.50/$9) for idiomatic quality | pennies/session | HIGH (multiple verified pricing sources) |
| Copilot agent | **Gemini 3.5 Flash** (May 2026, 1M ctx, 280+ t/s, cache reads $0.15/MTok) | `gemini-3.5-flash` | ~$0.05-0.2/question with prompt caching | HIGH pricing / MEDIUM best-model |
| Copilot quality alternative | **Claude Sonnet 5** (Jun 2026; $2/$10 intro until Aug 31 2026 → $3/$15) | `claude-sonnet-5` (or stable `claude-sonnet-4-6`) | — | HIGH |
| Copilot micro-tasks (question-completeness detection, summary) | **Claude Haiku 4.5** ($1/$5, 200K ctx) or `gemini-3.5-flash-lite` | `claude-haiku-4-5` | pennies | HIGH |
| TTS voice clone (user's voice, C→EN) | **MiniMax Speech 2.6 Turbo** (10s clone, 40+ langs, sub-250ms, ~99% similarity) | `speech-2.6-turbo` via **api.minimax.io** (international endpoint — cloning NOT available on mainland `api.minimax.chat`) | ~$10-40/MTok chars (≈$0.5-3 per interview hour) | HIGH market / MEDIUM clone-quality (must A/B listening test) |
| TTS latency alternative | **Cartesia Sonic 3.5** (sub-50ms TTFA, 42 langs incl. Chinese, instant clone 3-10s) | `sonic-3.5` WebSocket | Pro $5/mo, ~$35-50/MTok | MEDIUM (Chinese clone quality less battle-tested) |
| TTS budget/free alternative | **Fish Audio S2.1 Pro** (141ms TTFA measured, 83 langs, 10-30s clone) | `s2.1-pro` (free `s2.1-pro-free` tier, no SLA) | $15/MB = **$45/MTok for CJK chars** (3 bytes/char gotcha) | MEDIUM |
| Web search | **Brave Search API** (+ LLM Context API for LLM-ready output) | Brave v2 endpoints | $5/1K queries | MEDIUM-HIGH |
| Web search fallback | **Tavily** (easiest integration, relevance scoring, ~45min on-ramp) | Tavily API | $5-8/1K | MEDIUM (Nebius acquisition → roadmap uncertainty) |
| Web search semantic alt | **Exa** ("Instant" mode sub-200ms) | Exa API | $7/1K + extraction billed separately | MEDIUM |

### Mobile H5 Teleprompter

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Same React app as desktop webview | React 19 + Vite 7 | H5 served by desktop's axum over LAN | Zero install; one frontend codebase; WS pushes subtitle tokens + strategy cards (typewriter rendering) |
| WebSocket (browser native) | — | Subtitle/AI stream from desktop | Simpler than SSE for bidirectional (also allows phone→desktop manual trigger) |
| QR code (desktop-side rendering) | qrcode crate | Pairing | Scan → `ws://192.168.x.x:8787` → join session |
| Tailwind v3.4 (or plain CSS) | v3.4.x | Styling | Mobile browsers are modern, but shared codebase with the Safari-15.6 webview forces the conservative floor |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| pnpm | Package manager | Fast, deterministic, single store for desktop+H5 workspaces |
| Rust toolchain via rustup | Rust builds | Set `MACOSX_DEPLOYMENT_TARGET=12.0`; targets `aarch64-apple-darwin` + `x86_64-apple-darwin` |
| tauri CLI | Dev/build | `tauri dev` hot-reload; `tauri build` with `minimumSystemVersion: "12.0"` |
| Apple Developer ID + notarization | App signing | Required for distribution; same cert story as the BlackHole installer guidance |
| Playwright | E2E + visual regression | Test at 320/768/1024/1440; both themes (dark-only here); test teleprompter flows |
| Vitest | Unit tests | Audio segment utils, VAD state machine, translation-buffer logic |
| Mock API server (wiremock/mockito) | Pipeline tests | Deterministic fake STT/LLM/TTS streams for latency testing without live APIs |

## Installation

# Desktop + H5 workspace (single repo)

# Frontend deps

# Rust deps (cargo add, inside src-tauri)

# Audio driver (user-guided, bundled or brew)

# Or bundle BlackHole2ch-0.7.1.pkg in the app and guide install (needs admin)

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| Gemini Live (text mode) for user STT+translate | Deepgram Nova-3 + separate LLM translate | If Live API interim-text behavior proves unsuitable or latency/accuracy fails validation; classic 3-hop cascade, still fine |
| Deepgram Nova-3 for interviewer STT | OpenAI `gpt-4o-transcribe` ($0.006/min, 99+ langs) | If single-vendor OpenAI billing/ecosystem matters; English+Chinese both solid there |
| MiniMax Speech 2.6 Turbo (clone TTS) | Cartesia Sonic 3.5 | If the cascade needs more latency headroom (sub-100ms TTFA) or MiniMax Turbo's 22kHz/jargon artifacts fail the listening test |
| MiniMax / Cartesia (clone TTS) | Fish Audio S2.1 Pro | Budget MVP / free-tier prototyping (`s2.1-pro-free`); CJK char billing is 3x — factor into cost |
| MiniMax / Cartesia / Fish | ElevenLabs Flash v2.5 | Only if English-voice quality benchmark must be beaten and Mandarin flaws acceptable; ~5-10x cost |
| Gemini 3.5 Flash (copilot) | Claude Sonnet 5 / Sonnet 4.6 | When strategy quality beats latency (hard interviews, ambiguous questions); Anthropic = best tool-use/agentic Aug 2026 |
| Gemini 3.5 Flash (copilot) | Claude Haiku 4.5 | Question-completeness detection + cheap summarization micro-tasks only |
| Brave Search | Tavily | If team wants the fastest integration + relevance scoring; accept post-acquisition roadmap risk |
| Brave Search | Exa | Knowledge-base augmentation ("similar topics", related experiences) — semantic retrieval |
| Cascade (STT→LLM→TTS) | Azure Live Interpreter + Personal Voice | **Only** if Microsoft grants Personal Voice limited access AND product accepts single-vendor lock-in — single-API dream but approval risk |
| Tauri 2.11 | Electron | Never for v1 — requirement doc's own analysis: Rust memory safety + low-copy audio + ~10MB vs Electron's ~200MB and GC hiccups in the audio path |
| React 19 shared frontend | Vue 3 | Either works; reference HTML is React-marked, team picks React |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| ElevenLabs as primary clone TTS | Mandarin weaknesses documented across 2026 reviews (tone errors, weak Chinese emotion, awkward code-switching); ~$300/MTok = 5-10x alternatives; v3 not realtime-optimized (250ms inference); Flash v2.5 is the only realtime tier and quality drops | MiniMax Speech 2.6 Turbo (primary) / Cartesia Sonic 3.5 / Fish S2.1 Pro |
| GPT-4o-mini for translation | Legacy (Oct 2023 knowledge), ~4x slower throughput than Gemini Flash-class (54-101 t/s vs 204-359 t/s), beaten on quality | Gemini 3.5 Flash-Lite / 3.5 Flash |
| Gemini 3.5 Live Translate as core pipeline | ~2.9s first-audio (independent LiveLingo benchmark) exceeds the 2s budget; not a voice clone; voice-instability issues; audio-only, no tool use | Cascade: Gemini Live text mode (STT+translate) → MiniMax clone TTS |
| gpt-realtime-translate as core pipeline | **No voice selection whatsoever**; only 13 output languages; no tools; single-session-per-language | Same cascade |
| Play.ht / PlayAI | Acquired by Meta, service shut down Dec 31, 2025 (clones deleted) | Any of the TTS picks above |
| Bing Search API | Retired by Microsoft Aug 2025 | Brave / Tavily / Exa |
| SerpApi | Google DMCA lawsuit (Dec 2025) — legal risk for the project | Brave Search (independent index) |
| Tailwind CSS v4 in the Tauri webview | Requires Safari 16.4+ features (`@property`, `color-mix`); macOS 12.7 caps at Safari 15.6 | Tailwind v3.4 + plain CSS custom properties (also matches the design-token style rules) |
| Google Fonts / FontAwesome CDN | PROJECT.md constraint: Tauri offline must bundle locally; CDN also leaks the stealth tool's traffic | Bundle Space Grotesk WOFF2 + FontAwesome 6 files in assets |
| Custom forked BlackHole driver in v1 | Building/signing/notarizing a CoreAudio driver needs the full Apple Developer Program + driver signing; months of work | Stock BlackHole 0.7.1 installer + guided onboarding; fork only after v1 validation |
| Rust `webrtc` crate for OpenAI Realtime | Huge, complex; WebRTC-in-Rust is a project of its own | WebSocket client (`tokio-tungstenite`) for OpenAI/Gemini/Deepgram — all expose WSS |
| Local models (whisper.cpp, local TTS) | Out of scope by decision; old-Mac CPU can't meet 2s budget with local LLM/TTS anyway | All cloud APIs (decision documented in PROJECT.md) |
| MongoDB + S3 + backend for v1 | v1 is a pure local tool by decision | Local WAV + Markdown/JSON exports; backend "按需建设" later |

## Stack Patterns by Variant

- Use a China-reachable primary path: MiniMax international (`api.minimax.io`) is globally distributed; consider DeepSeek LLM or iFlytek ASR for STT/translate; Gemini/OpenAI need a stable international connection (the user is already on Zoom/Teams for international interviews, so this is usually satisfied — but test it).
- Because the entire ≤2s budget collapses if each hop adds 200ms+.
- Prototype replacing the user path (Chinese STT→translate→clone TTS) with Azure Live Interpreter + Personal Voice (0.4-0.7s time-to-heard with pre-opened WSS, personal-voice on-the-fly).
- Keep the cascade for the interviewer path (subtitles need text control) and as the fallback. Do not migrate before an A/B listening test.
- Fall back to Deepgram Nova-3 for Chinese STT + Gemini Flash-Lite translate (classic 3-hop). Verify Chinese WER first (μ-Bench warns Mandarin accuracy varies wildly by provider).
- Cartesia Sonic 3.5 (latency-first) → Fish S2.1 Pro (cost-first) → ElevenLabs Flash v2.5 (English-quality-first, last resort for Chinese clones).
- Move the "fast outline" generation to `gemini-3.5-flash-lite` or `claude-haiku-4-5`, and let a richer model (Sonnet 5 / Gemini 3.5 Flash) produce the detailed draft card asynchronously.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| Tauri 2.11 | macOS 12.7 Monterey | WKWebView = Safari 15.6 / WebKit 613.x; set `minimumSystemVersion: "12.0"`; frontend must compile to `safari15` (esbuild target) — do not use Safari 16.4+ CSS/JS features |
| React 19 | Safari 15.6 | Fine (React supports Safari 12+); avoid React 19 concurrent-feature dependencies on newer Web APIs |
| webrtc-audio-processing ~2.0 | Rust stable | Version tracks upstream PulseAudio WebRTC snapshot (not semver) — **pin with `~2.0`**; fixed 10ms frames (480 samples @48kHz) — enforce in the audio graph |
| cpal | macOS 12 | Verify device enumeration of BlackHole 2ch works on Monterey; test on a real Monterey box (M1 + Intel) — CI must include a Monterey VM |
| BlackHole 0.7.1 | macOS 10.10+ | Monterey OK; needs admin install; app should detect missing driver and run the guided wizard |
| Vite 7 build output | Safari 15.6 | Configure `build.target: ['safari15', 'es2022']`; no top-level-await-only syntax |
| Claude Sonnet 5 intro pricing | Billing | $2/$10 intro ends **Aug 31, 2026** → $3/$15 from Sep 1 — price into the roadmap (today is Aug 26) |
| Fish Audio free tier | SLA | `s2.1-pro-free` no latency/SLA guarantee; free extension through **Aug 31, 2026** — paid path required for production |
| MiniMax | Region split | Cloning + HD models only on `api.minimax.io` (international); keys are NOT interchangeable between international and mainland platforms |
| Gemini Live API | Audio formats | Input PCM 16-bit/16kHz mono, output 16-bit/24kHz mono, 100ms chunks — resample everything through rubato at the API boundary |

## Sources

- Deepgram Nova-3 official launch (Feb 2025) — [deepgram.com/learn/introducing-nova-3-speech-to-text-api](https://deepgram.com/learn/introducing-nova-3-speech-to-text-api) — HIGH
- μ-Bench multilingual benchmark (Mandarin: "5x worse than English" warning; Nova-3 trails Chirp-3 on Mandarin, ~8x faster p50) — [sierra.ai/uk/blog/mu-bench-an-open-multilingual-transcription-benchmark](https://sierra.ai/uk/blog/mu-bench-an-open-multilingual-transcription-benchmark) — MEDIUM
- Gemini 3.1 Flash Live (Mar 2026, 90+ languages, Live API) — [safina.ai/en/blog/gemini-3-1-flash-live-realtime-voice-ai](https://safina.ai/en/blog/gemini-3-1-flash-live-realtime-voice-ai) — MEDIUM
- Live API text-modality + external clone TTS "half-cascade" pattern (dev practice, Jan 2026 forum) — [discuss.ai.google.dev/t/live-text-modality-for-stt/118142/3](https://discuss.ai.google.dev/t/live-text-modality-for-stt/118142/3) — MEDIUM
- 2026 STT landscape — [futureagi.com/blog/speech-to-text-apis-in-2026-benchmarks-pricing-developer-s-decision-guide](https://futureagi.com/blog/speech-to-text-apis-in-2026-benchmarks-pricing-developer-s-decision-guide) — MEDIUM
- OpenAI transcription pricing (Aug 2026) — [costgoat.com/pricing/openai-transcription](https://costgoat.com/pricing/openai-transcription) — MEDIUM
- Gemini 3.5 Flash (May 2026, $1.50/$9, 1M ctx, 280+ t/s, Flash-Lite $0.30/$2.50 Jul 2026) — [artificialanalysis.ai/models/gemini-3-5-flash](https://artificialanalysis.ai/models/gemini-3-5-flash) + [llm-stats.com/models/gemini-3.5-flash](https://llm-stats.com/models/gemini-3.5-flash) — HIGH
- Gemini 2.5 Flash vs GPT-4o-mini (TTFT 0.12s vs 0.14s; 230 t/s vs 101 t/s) — [aymo.ai/compare-ai-models/gemini-2-5-flash-vs-gpt-4o-mini](https://aymo.ai/compare-ai-models/gemini-2-5-flash-vs-gpt-4o-mini) — MEDIUM
- Claude lineup Aug 2026 (Sonnet 5 Jun 30 2026 $2/$10→$3/$15; Sonnet 4.6 Feb 2026 $3/$15; Haiku 4.5 $1/$5; Opus 4.8 $5/$25) — [github.com/Sagargupta16/claude-cost-optimizer](https://github.com/Sagargupta16/claude-cost-optimizer) + [cosmicjs.com/blog/claude-opus-4-8-ai-native-development](https://www.cosmicjs.com/blog/claude-opus-4-8-ai-native-development) — HIGH
- Agent Arena Aug 2026 (Anthropic #1-3: Fable 5, Opus 5) — [lmmarketcap.com/ai-models-for-agents](https://lmmarketcap.com/ai-models-for-agents) — MEDIUM
- 2026 ElevenLabs alternatives (Cartesia latency, MiniMax Chinese strength, Fish East Asian value) — [frankx.ai/blog/best-elevenlabs-alternatives-2026](https://www.frankx.ai/blog/best-elevenlabs-alternatives-2026) — MEDIUM
- Chinese TTS real-world test (Aug 2026, MiniMax strongest Mandarin emotion/naturalness) — [zhuanlan.zhihu.com/p/2075288940618199809](https://zhuanlan.zhihu.com/p/2075288940618199809) — MEDIUM
- ElevenLabs Mandarin weaknesses (IVC vs PVC, Chinese comparison) — [foreverwebs.com/blog/elevenlabs-voice-cloning-2026-ivc-pvc-competitors-scenario-guide](https://foreverwebs.com/blog/elevenlabs-voice-cloning-2026-ivc-pvc-competitors-scenario-guide) — MEDIUM
- Cartesia Sonic 3.5 (42 langs incl. Chinese, instant clone 3-10s) — [together.ai/models/cartesia-sonic-35](https://www.together.ai/models/cartesia-sonic-35) + [texttolab.com/blog/cartesia-ai-review](https://texttolab.com/blog/cartesia-ai-review) (40ms claim) — MEDIUM
- MiniMax Speech 2.6 Turbo (sub-250ms, 40+ langs, 10s clone, ~99% similarity) — [wavespeed.ai/models/minimax/speech-2.6-turbo](https://wavespeed.ai/models/minimax/speech-2.6-turbo) + [nolist.ai/item/minimax-speech](https://nolist.ai/item/minimax-speech) (cloning only via api.minimax.io) — MEDIUM
- Fish Audio S2.1 Pro (141ms TTFA measured; $15/MB; free tier) — [humannessindex.vapi.ai/models/fish-s2-1-pro](https://humannessindex.vapi.ai/models/fish-s2-1-pro) + [fish.audio blog](https://fish.audio/zh-CN/blog/tts-inference-optimization-s2-pro-free/) — MEDIUM
- ElevenLabs v3 vs Flash v2.5 (v3 250ms inference, not realtime; Flash 75ms) — [ttsaudit.com/blog/elevenlabs-v3-vs-turbo-v2-5](https://ttsaudit.com/blog/elevenlabs-v3-vs-turbo-v2-5) — MEDIUM
- Gemini 3.5 Live Translate official (Jun 2026) — [Google blog](https://www.gweb-uniblog-publish-prod.appspot.com/innovation-and-ai/models-and-research/gemini-models/gemini-live-3-5-translate/) — HIGH
- Gemini 3.5 Live Translate independent latency (~2.9s first-audio, LiveLingo benchmark) — [livelingo.io/guides/gemini-3-5-live-translate](https://www.livelingo.io/guides/gemini-3-5-live-translate) — MEDIUM
- gpt-realtime-translate official (no voice selection; 13 output languages; $0.034/min) — [developers.openai.com/api/docs/guides/realtime-translation](https://developers.openai.com/api/docs/guides/realtime-translation) — HIGH
- Azure Live Interpreter + Personal Voice (limited-access approval; zh-Hans; universal v2 endpoint) — [learn.microsoft.com Speech translation](https://learn.microsoft.com/en-nz/azure/ai-services/speech-service/how-to-translate-speech) — HIGH
- 2026 search-API benchmarks (Brave 669ms / 38.3% BrowseComp; Tavily 998ms / 19.3%; Exa Instant) — [fastcrw.com/blog/search-api-for-ai-agents](https://fastcrw.com/blog/search-api-for-ai-agents) + [parallel.ai/articles/best-web-search-api](https://parallel.ai/articles/best-web-search-api) + [artificialanalysis.ai/agents/search-api](https://artificialanalysis.ai/agents/search-api) — MEDIUM
- Tavily acquired by Nebius (Feb 2026); Bing retired (Aug 2025); Brave LLM Context API (Feb 2026) — same sources — MEDIUM
- Tauri 2.11.0 release notes (Apr 2026) — [v2.tauri.app/release/tauri/v2.11.0](https://v2.tauri.app/release/tauri/v2.11.0/) — HIGH
- Tauri WebKit/macOS mapping (Monterey 12.0-12.6 → Safari 15.x) — [tauri.app/reference/webview-versions](https://tauri.app/ko/reference/webview-versions/) — HIGH
- BlackHole 0.7.1 (Jul 2026, macOS 10.10+) — [github.com/ExistentialAudio/BlackHole](https://github.com/ExistentialAudio/Blackhole) — HIGH
- webrtc-audio-processing crate (2.1.0 May 2026; AEC3 full; ~2.x tilde pin) — [lib.rs/crates/webrtc-audio-processing](https://lib.rs/crates/webrtc-audio-processing) + [docs.rs](https://docs.rs/webrtc-audio-processing) — HIGH

<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:

- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
