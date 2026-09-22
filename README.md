# 极言 NexTalk

An AI real-time cross-language interview copilot. Think in your native language, speak fluent English in **your own cloned voice**: during international technical interviews you speak Chinese, and NexTalk transcribes, translates, and speaks out fluent English in a clone of your voice — while your phone acts as a teleprompter, scrolling bilingual subtitles in real time and surfacing AI answer strategies grounded in your resume.

**Core Value:** the interviewer hears seamless English answers; you see live subtitles and answer strategies — end-to-end latency ≤ 2 seconds.

## Features

- **Desktop dual-window app**: a 340×680 mini console (stealth mode, phone pairing, knowledge base, assets) plus an 860×680 extended view (live subtitles left, AI timeline right)
- **Phone H5 teleprompter**: scan a QR code and start — no app install. Bilingual subtitles on top, AI assistant below; wake-lock on plain `http://` LAN origins and automatic reconnect with resume
- **Bidirectional session control**: the phone's 开始提词 (Start) and the desktop's 开始模拟会话 (Start Session) are the same function — either side starts and stops the session, and the other follows
- **AI interview assistant**: interviewer questions are recorded in real time → an "AI thinking" card reveals its reasoning step by step → strategy bullets cascade in → a complete bilingual AI answer types itself out
- **Language switching**: 中 / EN / EN+中 filtering, synced live between desktop and phone
- **Simulated demo session**: a deterministic 4-round mock interview (every line badged 模拟数据) with a natural hear → think → answer cadence
- **Vendor experiment framework** (`tools/vendor-experiments/`): STT A/B protocol, blind clone-listening test set, RTT measurement tool — a zero-keys policy; experiment results decide the real pipeline's provider selection

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri 2.11 + Rust (axum LAN server, tokio broadcast, deterministic simulation engine) |
| Desktop UI / Phone H5 | React 19 + TypeScript + Vite 7 (one shared frontend, `safari15` target, runs on macOS 12.7) |
| Cross-device sync | LAN WebSocket gated by a 128-bit pairing token; one event model, two transports |
| Contracts | pnpm workspace: `packages/protocol` (closed union types + runtime guards), `packages/design-tokens` (neobrutalism design tokens) |
| Testing | Vitest · Playwright (32 e2e tests) · cargo test (34 unit + 4 integration) |

## Quick Start

Prerequisites: macOS 12.7+ (Monterey-compatible), Node 20+, pnpm, Rust toolchain (rustup).

```bash
pnpm install
pnpm --filter @nextalk/desktop dev:tauri   # launches the desktop app (both windows)
```

The console shows a pairing QR code — scan it with a phone on the same Wi-Fi to open the teleprompter.

Tests:

```bash
pnpm -r test                                  # all unit suites
pnpm exec playwright test                     # e2e (incl. the full demo session)
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Repository Layout

```
apps/desktop/           Tauri desktop shell (Rust src-tauri/ + React UI)
apps/teleprompter/      phone H5 teleprompter
packages/protocol/      wire protocol: ServerEvent/ClientMessage unions + isServerEvent guard
packages/design-tokens/ neobrutalism design tokens + Tailwind preset
e2e/                    Playwright end-to-end suites
tools/vendor-experiments/ vendor experiment framework (zero keys)
.planning/              development planning archive (roadmap, phase plans, verification reports)
```

## Current Status

- **Phase 1 complete**: full UI + simulated demo session (5/5 plans, all automated gates green, human UAT passed)
- **Phase 2 in progress**: the real cloud pipeline (STT → translate → clone TTS) with a latency rig as the gate; provider selection will be decided by the A/B experiments in `tools/vendor-experiments/`

## Privacy

- Purely local tool: recordings, transcripts, and review reports never leave the machine; v1 has no backend of its own
- Cloud AI calls minimize data exposure; the experiment framework never commits any key (`.env` is gitignored)
