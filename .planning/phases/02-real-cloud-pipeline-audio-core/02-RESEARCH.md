# Phase 2: Real Cloud Pipeline + Audio Core - Research

**Researched:** 2026-09-29
**Domain:** Realtime cascaded streaming voice translation (中文 STT → 增量翻译 → 克隆音色 TTS) on Rust/Tauri + macOS 12.7, with cloud vendor APIs
**Confidence:** MEDIUM-HIGH — wire protocols confirmed against the repo's own working experiment code (highest available authority); several pipeline crates are weeks-old with rewritten APIs; one product-critical assumption (cross-lingual clone) is unverified

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**输出可控：置信与弃权（实时流语义）**
- **D-01:** 置信度 = **三因子加权**：STT 置信度 × 翻译模型输出置信 × 术语命中率。阈值与权重由研究+规划代理用实验数据标定（不拍初值）。注意：可控原则同时约束 **Phase 5 的回答框架（GOV-23）**——简历库+术语库+网络检索证据 + LLM 理解组织，不可胡编；Phase 2 的 termHits 数据结构是其地基。
- **D-02:** 实时字幕**仅低置信标记**（气泡角红色「低置信」微章）；中置信黄色提醒只在复盘报告呈现，直播流零打扰。
- **D-03:** 弃权**仅限无声场景**：音频无有效文本（空转/无法识别）才弃权并显示「待翻译」标记；**低置信不弃权、照常输出但红色标记**——实时面试必须连续输出，用户选择优先于文档里更严格的 abstention 原则（该原则在复盘环节生效）。
- **D-04:** 确定性输出前校验：数字/金额/日期格式一致性规则校验（如 STT「800毫秒」↔ 译文「800ms」），全部译文过此层。

**可溯源：数据结构与线协议扩展**
- **D-05:** 溯源落地 = **JSONL 事件流文件**（每句一行：原文/译文/置信/时间戳/术语命中/provider/modelVersion），与现有 timeline 内存模型同构、追加写、零依赖。SQLite 索引与复盘查询 UI 留到 Phase 6。
- **D-06:** 时间戳粒度 = **语句级 segment 偏移**（每句相对会话起始的 ms）。
- **D-07:** 线协议（`packages/protocol` ServerEvent 闭合联合）向后兼容扩展：
  - `subtitle` 变体新增 `confidence: 'high'|'medium'|'low'`
  - 新增 `trace` 对象：`segmentStartMs`、`termHits`、`provider`、`modelVersion`
  - 新增 `abstained` 事件（无声弃权）
- **D-08:** 模型版本**逐句记录**（实验期频繁换模型，会话级快照会归因失真）。

**可兜底：容错链路 v1 参数**
- **D-09:** **片段级重试**：瞬时错误（超时/429/5xx）重试 2 次、指数退避 100→200ms、总预算 500ms 内放弃该片段并流下一片段。禁止整句串行重试（会破 2s 预算）。
- **D-10:** 熔断：**连续 2 次失败熔断该供应商 120s**，半开探测恢复。
- **D-11（已定，2026-09-29 盲听实验）:** TTS 主供应商 = **火山声音复刻 2.0（ICL 2.0）**——盲听 MOS 5.0/5.0（用户本人判定，相似度与自然度满分），国内直连、1.3s 合成；MiniMax 国际站不再必需，备用供应商在 v1 可免（失败走降级显示原文）。实验数据：`tools/vendor-experiments/blind-clone-results.json`。
- **D-12:** 降级展示（中文锁定）：红色微章 + 「翻译服务暂时不可用」+ 显示原文 + 副标注「正在重试」。

**成本可算：本地化计量（开发者面板）**
- **D-13:** 分阶段计量：STT 按音频分钟、翻译按 token、TTS 按字符；每句记录各阶段用量。**双面板分层**（2026-09-29 优化）：桌面设置页展示**用户视角的剩余分钟数**；完整分阶段明细归开发者观测（Phase 2 本地开发面板 + Phase 8 云端管理后台）。分钟额度逻辑与定价联动（Phase 8）。
- **D-14:** 成本路由：**实验期固定路由**（每阶段单一供应商），自动升降级留到数据积累后。
- **D-15:** 用户侧预算闸门：**提示式，待细化**——达到月度用量阈值给予提示，不硬停（与未来收费模式联动，见 Deferred）。

**架构拓扑：本地网关与公测基建（2026-09-29 二次讨论）**
- **D-16:** 三层容错架构映射：**前端层** = 手机 H5 / 桌面 Webview（UX 兜底：状态展示、低置信标记、重试按钮、本地缓存）；**网关层** = **桌面 Rust 内核**（重试/熔断/成本闸门，D-09..D-15 在此落地）——Phase 2-7 无远程网关、保持纯本地；**管理后台** = 公测前新建（成本看板/错误日志/卡点分析/模型切换/配额管理/人工干预）。
- **D-17:** 公测前最小基建优先级：**账号（内测码+邮箱注册）→ 埋点上报 → 观测看板**。观测是控制的前提；远程网关（服务器持 key）随商业化（分钟包月制）引入，本地直连为 v1 兜底。
- **D-18:** 埋点日志**复用本地 JSONL 溯源流**（D-05），云端聚合上报供管理后台查询——不另造第二套埋点体系。
- **D-19:** ROADMAP 新增 **Phase 8: Beta Infrastructure + Commercialization**（Phase 7 之后、公测之前）承接：账号体系、埋点上报、观测看板、远程网关最小版、分钟包月制定价落地。Phase 2 只需保证 JSONL 结构与埋点字段（用户/任务/模型/耗时/状态/错误码）可聚合。
- **D-20:** **失败案例库 + 回归评测机制**（反馈闭环的 Phase 2 侧）：
  - 每个失败案例 = 输入（音频片段/原文）+ 错误输出 + 期望输出 + **根因分类**（术语缺失 / 模型幻觉 / 音频质量 / 数字漂移 / 其他）+ 来源（低置信事件 / 用户反馈 / 人工抽检）。JSONL 溯源流自动抽取低置信/弃权/数字不一致事件作为候选入库源。
  - **回归评测**：全量案例库在每次修复与发布前回归（正确案例必须继续通过，失败案例在修复时**转正为回归测试**——「每一只逃出来的 bug 都要变成笼子上的新栅栏」）。评测集起步 20 例（AI-SPEC 5 节），随错误发现自动增长。
  - 案例库落位与目录结构由规划代理设计（建议 `tools/vendor-experiments/failure-cases/` 或 `.planning/ref/`），CI 集成方式与既有三套测试并列。

### Claude's Discretion
- 置信三因子的具体权重公式与低置信红线（实验标定）
- JSONL 事件流的字段 schema 细节与文件滚动策略
- 延迟测量装置的具体实现（分阶段时间戳注入点）
- 重试/熔断参数的实验期微调

### Deferred Ideas (OUT OF SCOPE)
- **商业化定价 + 公测基建**：已晋升为 ROADMAP Phase 8（D-19），分钟包月制 + 内测码 + 邮箱注册 + 漏斗观察 + 远程网关最小版；Phase 2 仅埋可聚合数据。
- **用户侧预算闸门细化**：与收费模式联动后细化（提示式 → 硬闸门 → 恢复流程）。
- 多级配额（用户级/部门级）、RBAC（查看者/编辑者/管理员）、审计日志——SaaS 概念，v1 不适用，商业化阶段再评估。
- 低置信「人工确认」环节——实时流不可行，复盘报告（Phase 6）中实现。
- 反馈闭环 UI（用户修正→术语库/评测集）——术语库 Phase 4、复盘 Phase 6；Phase 2 仅埋错误记录数据。
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUDI-03 | 实时语音翻译链（mic → STT → 翻译 → 克隆 TTS → 播放） | Topic 1 (讯飞 WSS client), Topic 2 (Deepgram WSS), Topic 4/5 (AEC + cpal audio graph), Topic 6 (rubato 5 resampling), Topic 7 (cascade commit gate) — all four vendor wire protocols verified against the repo's own working experiment scripts |
| AUDI-04 | 延迟测量装置（分阶段瀑布 + e2e ≤2s 冷启动） | Topic 8 latency-arithmetic finding: existing per-vendor numbers are *whole-request* totals, not streaming TTFB. 02-01 must timestamp at stage boundaries (mic-callback → STT first-partial → MT first-token → TTS first-audio → first PCM drained by cpal) |
| AUDI-05 | 克隆音色注册（1–3 分钟录音 → 复刻音色）+ 预置音色兜底 | Topic 1 of the deep dive (火山 cross-lingual risk, `explicit_language=en` + `tone_fidelity=false`), voice_clone REST path already in `volc-voice-clone.mjs`, 10MB audio cap + WER gate 45001109 |
| AUDI-06 | 抢话处理（~100ms 内停止英文输出，无重叠音频）+ 设备热插拔存活 | Topic 3 (epoch-based barge-in), Topic 5 (cpal ErrorKind-driven rebuild — no hotplug notification API exists) |
| GOV-01/02/03 | 三因子置信 / 低置信红标 / 无声弃权 | Topic 7 + Topic 1 finding: 讯飞 `sc` is a **reserved, undocumented** field — the Chinese path has no vendor confidence score. Proxy definition required (Claude's Discretion) |
| GOV-06/07/08 | JSONL 逐句溯源 / 语句级 ms / 协议置信+trace+abstained 扩展 | `state.rs` `append_event`/`publish` is the single write point; `packages/protocol/src/index.ts` ↔ `lan/server.rs` mirror pattern (Phase 1 `wire_shapes_match_protocol_package` test) |
| GOV-12/13/14/15 | 片段级重试 / 熔断 / 降级展示 / partial-final 不变量 | Topic 1 error-code table (retryable vs terminal classification), Topic 2 is_final/speech_final buffering rule, Topic 7 invariant test design |
| GOV-19/20 | 失败案例库 / 回归评测 | `tools/vendor-experiments/failure-cases/0001-0002` already exist; Wave 0 gaps list the missing runner |
</phase_requirements>

## Summary

Vendor selection is settled and was **not** re-evaluated. This research covers the *engineering* of wiring the four decided vendors (讯飞 iat, Deepgram Nova-3, DeepSeek-chat, 火山 Seed-ICL 2.0) into the existing `SessionState` / `ServerEvent` architecture, plus the macOS audio core (cpal + rubato + AEC3).

Three findings change the shape of the plan:

1. **The clone's cross-lingual capability is unverified — and it is the whole product.** The blind MOS test (5.0/5.0) used **Chinese** test sentences only. The synthesis parameter that actually governs our use case, `tone_fidelity` (还原模式), **explicitly does not support cross-lingual synthesis**; English output therefore requires `explicit_language=en` + `tone_fidelity=false`, and BytePlus's English docs directly contradict the Chinese docs on whether cross-lingual works at all. This must be settled empirically before 02-04 (clone enrollment) and 02-02 (provider wiring) are locked — if it fails, D-11's primary fails and a second TTS vendor returns as mandatory.

2. **The `≤2s` budget cannot be validated from the existing numbers.** They are *whole-request* latencies, not streaming first-byte latencies: 讯飞 0.7s = full clip → final result; 火山 1.3s = full sentence → `SESSION_FINISHED`. Naive summation 0.7 + 0.223 + 1.3 = 2.22s exceeds the budget, but that sums three non-concurrent completions. 02-01 must measure per-stage **TTFB**, and the cascade must overlap stages (first audio starts while STT is still finalizing later fragments).

3. **The Chinese path has no vendor confidence score.** 讯飞's `sc` field is classed as 保留字段 ("reserved, do not care about it") in the official spec and is always `0` in samples. GOV-01's "STT 置信度" factor therefore has no direct source on the user path — the plan must define an explicit local proxy (and mark it as such in the trace so it is never mistaken for a vendor number).

**Primary recommendation:** Build 02-01 (latency rig + typed stage contracts + mock providers) first and treat it as the hard gate; wire the four vendors behind `SttSource`/`Translator`/`TtsSink` traits so the vendor A/B stays a one-file swap; and run the cross-lingual clone probe as the **first task of 02-04**, before any enrollment UI is designed.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Mic capture / resample / AEC3 | Desktop Rust core (`audio/`) | — | CoreAudio device access exists only in the native process; cpal callbacks must never block |
| Chinese STT (user path) | Cloud vendor — 讯飞 iat via Rust WSS client | Desktop Rust core | The Rust core owns the socket, the retry policy and the breaker; vendors are never called from the UI tier |
| English STT (interviewer path) | Cloud vendor — Deepgram Nova-3 via Rust WSS client | Desktop Rust core | Same; second vendor path also acts as fault isolation |
| Incremental translation | Cloud vendor — DeepSeek-chat SSE | Desktop Rust core | Same; SSE parsing and numeric pre-validation live in the core |
| Cloned TTS synthesis | Cloud vendor — 火山 Seed-ICL 2.0 | Desktop Rust core | Same; per-character cost metering happens at the call site |
| partial/final commit gate (D-16) | Desktop Rust core (`pipeline/cascade.rs`) | — | Must be the single authority — no other tier may forward text toward TTS |
| Output pre-validation (D-04) | Desktop Rust core (`pipeline/`) | — | Deterministic, and must run before any audio leaves the process |
| Three-factor confidence (D-01) | Desktop Rust core | — | Local aggregation; no vendor provides the full triple |
| Playback queue + jitter buffer + barge-in flush | Desktop Rust core (`audio/`) | — | Realtime audio thread ownership; epoch is a core-local concept |
| JSONL trace stream (D-05..D-08) | Desktop Rust core (`trace/`) | — | Append-only; shares the `SessionState::append_event` path |
| Wire-protocol extension (`confidence` / `trace` / `abstained`) | Desktop Rust core (`lan/server.rs`) + `packages/protocol` | Desktop webview + phone H5 (consumers) | One event model, two transports — the Rust mirror and the TS union must move together |
| Cost metering (D-13) | Desktop Rust core | Desktop settings page (user view) + developer panel | Metering is local; display is split by audience |
| Low-confidence badge / degraded copy (D-02, D-12) | Phone H5 + desktop webview | — | Presentation only — the Rust core ships the `confidence` enum, never the score |
| TTS→BlackHole, aggregate device | **Phase 3 — out of scope** | — | Phase 2 plays to the default output device |

## Standard Stack

### Core (new in Phase 2)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-tungstenite | 0.30.0 | All three WSS clients (讯飞 iat, Deepgram listen, 火山 unidirectional TTS) | Already in the repo as a dev-dependency (0.28) for `session_integration.rs`; one client library for every streaming endpoint; supports custom handshake headers (required by 火山 `X-Api-*`) |
| reqwest | 0.13.5 | DeepSeek SSE streaming + 火山 `voice_clone` REST | De-facto async HTTP client; `bytes_stream()` gives chunk-level SSE consumption |
| cpal | 0.18.2 | CoreAudio capture + playback | The standard Rust audio I/O crate (22.1M downloads, RustAudio org); 0.18 unified `Error`/`ErrorKind` is what makes hot-plug recovery expressible |
| rubato | 5.0.0 | 48k↔16k (STT) and 24k→48k (TTS) resampling | Exact-ratio `FixedSync::Both` mode has the lowest buffering; 48k→16k is exactly 3:1, 24k→48k exactly 2:1 |
| webrtc-audio-processing | `~2.0` (2.1.0) | AEC3 echo cancellation + noise suppression + AGC + high-pass | The reference WebRTC AEC3 implementation. **Pin with `~`** — the crate does not follow semver (2.3 may break 2.2) |
| hound | 3.5.1 | WAV read/write for enrollment recording + test fixtures | Zero-dep RIFF writer/reader; the crate's own examples use it |
| thiserror | 2.0.21 | Typed provider errors (retryable vs terminal) | Drives the D-09 retry classifier and the D-10 breaker without string matching |
| anyhow | 1.0.104 | Ergonomic error propagation at the app boundary | Already transitively present |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde / serde_json | 1.0.229 / 1.0.151 | Provider frame parsing + the wire protocol | All provider JSON uses `deny_unknown_fields` structs — provider payloads are untrusted input |
| tokio | 1.53.1 | Runtime, bounded mpsc channels (backpressure), timers | Every stage boundary is a bounded channel; unbounded = latency growth |
| futures-util | 0.3.34 | Stream adapters over provider messages | Already present; used for the SSE and WSS stream combinators |
| tokio-tungstenite (dev) | 0.28 → align to 0.30 | Existing integration tests | Upgrade so dependency and dev-dependency do not diverge |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| cpal | `coreaudio-rs` directly | Needed only if cpal's device abstraction fights Phase 3's aggregate-device work; not for Phase 2 |
| rubato | `samplerate` (libsamplerate bindings) | Adds a C dependency; no exact-ratio fixed mode; worse for the 3:1 and 2:1 cases |
| webrtc-audio-processing | `sonora` (pure Rust WebRTC AP) | 85 stars, unproven against the 2026 upstream — not a v1 risk worth taking |
| tokio-tungstenite | `fastwebsockets` | Faster, far less ergonomic; the repo already has a working tungstenite pattern |
| silero-vad-rs | Server-side endpointing (Deepgram `endpointing`/`UtteranceEnd`) + a local energy VAD | **Recommended for Phase 2** — silero-vad-rs pins `ort =2.0.0-rc.9` exactly, has 3,004 total downloads and its last release was 2025-04-04 (see Environment Availability) |
| Bounded mpsc | `broadcast` for stage-to-stage | `broadcast` is already used for UI events (`channel(64)`); it drops on lag, which is wrong for audio — audio paths need bounded mpsc with backpressure |

**Installation:**
```bash
cd apps/desktop/src-tauri
cargo add tokio-tungstenite@0.30 reqwest@0.13 --features reqwest/stream
cargo add cpal@0.18 rubato@5 hound@3.5
cargo add webrtc-audio-processing@~2.0 --features bundled
cargo add thiserror@2 anyhow@1
```

**Version verification:** Every version above was verified in this session against the crates.io registry API (the Rust-ecosystem equivalent of `npm view`):

```bash
curl -sL "https://crates.io/api/v1/crates/cpal" | python3 -c "import sys,json;d=json.load(sys.stdin)['crate'];print(d['max_stable_version'], d['downloads'])"
```

| Crate | max_stable_version | Downloads (all-time) | Repository | Published |
|-------|-------------------|---------------------|------------|-----------|
| cpal | 0.18.2 | 22,088,150 | github.com/RustAudio/cpal | 2026-08-16 (MSRV 1.85) |
| rubato | 5.0.0 | 11,956,830 | github.com/HEnquist/rubato | 2026-08-10 (MSRV 1.87) |
| webrtc-audio-processing | 2.1.0 | 134,792 | github.com/tonarino/webrtc-audio-processing | 2026-05-13 |
| silero-vad-rs | 0.1.2 | 3,004 | github.com/binarycrayon/silero-vad-rs | 2025-04-04 |
| tokio-tungstenite | 0.30.0 | 287,766,513 | github.com/snapview/tokio-tungstenite | — |
| hound | 3.5.1 | 19,416,943 | github.com/ruuda/hound | — |
| reqwest | 0.13.5 | 751,894,695 | github.com/seanmonstar/reqwest | — |
| thiserror | 2.0.21 | 1,530,433,302 | github.com/dtolnay/thiserror | — |
| anyhow | 1.0.104 | 996,720,514 | github.com/dtolnay/anyhow | — |

Local toolchain on the target machine: `rustc 1.98.0 (88d9e12ae 2026-08-18)`, targets installed = `x86_64-apple-darwin` only, `node v24.15.0`, `pnpm 11.24.0`, `macOS 12.7.6 (21H1320)`. All MSRVs are satisfied.

## Package Legitimacy Audit

slopcheck 0.6.1 was installed (`pip3 install --user slopcheck`) and run against the **crates.io** ecosystem (correct registry for this phase). Results:

| Package | Registry | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----------|-------------|-----------|-------------|
| tokio-tungstenite | crates.io | 287.8M | github.com/snapview/tokio-tungstenite | [OK] | Approved |
| reqwest | crates.io | 751.9M | github.com/seanmonstar/reqwest | [OK] | Approved |
| rubato | crates.io | 12.0M | github.com/HEnquist/rubato | [OK] | Approved |
| webrtc-audio-processing | crates.io | 134.8K | github.com/tonarino/webrtc-audio-processing | [OK] | Approved |
| hound | crates.io | 19.4M | github.com/ruuda/hound | [OK] | Approved |
| thiserror | crates.io | 1.53B | github.com/dtolnay/thiserror | [OK] | Approved |
| anyhow | crates.io | 996.7M | github.com/dtolnay/anyhow | [OK] | Approved |
| silero-vad-rs | crates.io | 3,004 (total, all versions) | github.com/binarycrayon/silero-vad-rs | [OK] | Approved, **but not recommended** — see maintenance risk below |
| **cpal** | crates.io | 22.1M | github.com/RustAudio/cpal | **[SUS]** | Flagged — **assessed as a false positive** |

**Packages removed due to slopcheck [SLOP] verdict:** none.

**Packages flagged as suspicious [SUS]:**

- `cpal` — slopcheck raised `TYPOSQUAT_RISK: "Suspiciously close to 'clap'. Could be a typosquat."` This is a name-similarity false positive: `cpal` (Cross-Platform Audio Library) is a 12+ year old crate maintained by the RustAudio organisation with **22.1M all-time downloads** and is the dependency of essentially every Rust audio project. slopcheck's heuristic compares against `clap` (a CLI argument parser) purely on edit distance. **Disposition: keep.** The planner should record the check and its rationale in the plan rather than gate the install behind a human checkpoint, but should not silently drop it — the audit trail matters.

**Maintenance-risk flag (not a slopcheck finding):** `silero-vad-rs` is `[OK]` for legitimacy but has a hard pin on `ort =2.0.0-rc.9` (a release-candidate ONNX Runtime binding), only 3,004 lifetime downloads, and no release since 2025-04-04. Verify it actually builds and runs on macOS 12.7 x86_64 before it becomes load-bearing; the recommended default for Phase 2 is to defer it (see Topic 4 and Topic 6).

## Architecture Patterns

### System Architecture Diagram

```
          ┌─────────────────────── Tauri desktop process (Rust core) ───────────────────────┐
          │                                                                                  │
 mic ──▶  │  cpal input callback ──▶ bounded mpsc ──▶ resample(→48k) ──▶ AEC3 Processor ──┐  │
 (48k)    │      (never blocks)                          ▲                             │  │
          │                                              │ render reference            │  │
          │                                              │                             ▼  │
          │                                     cpal output callback ◀── playout ring  ┌───┴──┐
          │                                              ▲          (jitter buffer)   │ VAD  │
          │                                              │                            └──┬───┘
          │                                              │                               │ segment
          │                                       TTS audio chunks                       │
          │                                              ▲                               ▼
          │                                              │                    ┌────────────────────┐
          │                                        ┌─────┴──────┐             │  COMMIT GATE       │
          │                                        │ 火山 TTS   │             │ only committed     │
          │                                        │ WSS client │             │ finals may pass    │
          │                                        └─────▲──────┘             └─────────┬──────────┘
          │                                              │                              │ final text
          │                                              │                              ▼
          │                              ┌───────────────┴───────┐            ┌────────────────────┐
          │                              │ NUMERIC PRE-VALIDATOR │◀───────────│ DeepSeek translator│
          │                              │ (D-04; fail→source)   │            │ SSE client         │
          │                              └───────────────▲───────┘            └─────────▲──────────┘
          │                                              │                              │ zh final
          │                                              │                    ┌─────────┴──────────┐
          │                                              │                    │  讯飞 iat WSS      │
          │                                              │                    │  (user, zh)        │
          │                                              │                    └─────────▲──────────┘
          │                                              │                              │
          │  ┌───────────────────────────────────────────┴───────────────────────────┐  │
          │  │ SessionState  ── append_event() ──▶ timeline (resume)                 │  │
          │  │                    │                    │                    │        │  │
          │  │                    ├──▶ Tauri emit (desktop webview)                │  │
          │  │                    ├──▶ WS broadcast (phone H5)                     │  │
          │  │                    └──▶ JSONL writer task (D-05; single writer)      │  │
          │  └──────────────────────────────────────────────────────────────────────┘  │
          │                                                                             │
          │  ┌──────────────────────────────────────────────────────────────────────┐   │
          │  │ CircuitBreaker per vendor (D-10) + segment retry (D-09) + cost meter │   │
          │  └──────────────────────────────────────────────────────────────────────┘   │
          └─────────────────────────────────────────────────────────────────────────────┘
                    ▲                                                        ▲
                    │ WSS (interviewer path, independent)                    │
        ┌───────────┴────────────┐                                ┌──────────┴─────────┐
        │ Deepgram Nova-3 (en)   │  ──▶ subtitles only, never TTS  │ phone H5 / webview │
        └────────────────────────┘                                └────────────────────┘
```

Reader trace of the primary use case: **User speaks Chinese** → cpal callback queues PCM → AEC3 cleans it → 讯飞 returns word pieces (previews, then a `status=2` final) → the commit gate releases only the final → DeepSeek streams English fragments → numeric pre-validator passes them → 火山 streams cloned-voice audio → the playout ring feeds cpal → the user hears English. In parallel, every event lands in `SessionState`, so the desktop and phone render subtitles and the JSONL writer persists the trace. A barge-in signal (VAD during playback) bumps the epoch, and the playout ring drops everything whose epoch is stale.

### Recommended Project Structure

```
apps/desktop/src-tauri/src/
├── audio/                  # NEW: cpal input+output, rubato resamplers, AEC3 wrapper, VAD, playout ring
│   ├── capture.rs          #   input stream + error-callback → rebuild channel
│   ├── playout.rs          #   jitter buffer + epoch-guarded queue (barge-in flush)
│   ├── resample.rs         #   rubato 5 FixedSync::Both wrappers (48k↔16k, 24k→48k)
│   ├── aec.rs              #   webrtc-audio-processing Processor, 10ms/480-sample loop
│   └── device.rs           #   enumeration + ErrorKind-driven hot-swap (NO hotplug API exists)
├── pipeline/
│   ├── stages/             #   NEW: trait + vendor impls
│   │   ├── stt_source.rs   #     trait SttSource + XfyunStt / DeepgramStt
│   │   ├── translator.rs   #     trait Translator + DeepSeekTranslator
│   │   └── tts_sink.rs     #     trait TtsSink + VolcTts (+ stock-voice fallback)
│   ├── cascade.rs          #   NEW: commit gate + overlap loop + bounded channels
│   ├── breaker.rs          #   NEW: closed/open/half-open, injectable TimeSource
│   ├── validate.rs         #   NEW: D-04 numeric/unit/date consistency
│   ├── confidence.rs       #   NEW: D-01 three-factor scoring
│   └── budget.rs           #   NEW: latency waterfall marks + cost metering
├── trace/
│   └── jsonl.rs            #   NEW: single-writer append task
├── lan/
│   └── server.rs           #   EXTEND: confidence / trace / abstained on ServerEvent
├── state.rs                #   EXTEND: route transport paths through the JSONL writer
└── sim/                    #   KEEP: SimSource stays as the deterministic test double
```

### Pattern 1: Typed stage contracts (mirrors the Phase 1 `SimSource` abstraction)

**What:** Each pipeline stage is a Rust trait with a vendor implementation behind it, so the vendor A/B stays a one-file swap and the tests use deterministic doubles.
**When to use:** Every provider boundary in this phase.
**Example:**
```rust
// Source: 02-AI-SPEC.md §3 "Key Abstractions" — trait shape given there;
// this is the concrete version consistent with sim/source.rs
#[async_trait::async_trait]
pub trait SttSource: Send + 'static {
    /// Yields ordered partials; only `is_final` text may leave the commit gate.
    async fn stream(&mut self, pcm16: &[i16]) -> Result<SttStream, StageError>;
    fn provider(&self) -> &'static str;      // -> trace.provider
    fn model_version(&self) -> String;       // -> trace.modelVersion (D-08)
}
```

### Pattern 2: Epoch-guarded playout (barge-in)

**What:** A monotonically increasing `epoch` (already present as `SessionState::session_epoch`) is the logical owner of every queued audio chunk. Bumping it invalidates all queued work; each async boundary re-checks before acting.
**When to use:** Every point in the playout path, and every callback that can fire after a cancellation.
**Example:**
```rust
// Source: Amadeus barge_in_aec_interrupt_notes.md (epoch discipline);
// matched to this repo's existing session_epoch (state.rs)
pub fn interrupt(&self) {
    self.playback_epoch.fetch_add(1, Ordering::SeqCst); // bump FIRST
    self.queue.clear();                                   // then flush
}
// ...and at every await boundary in the playout task:
if chunk.epoch != self.playback_epoch.load(Ordering::SeqCst) {
    return; // stale — discard, never enqueue
}
```

### Pattern 3: Buffer-then-flush finality (Deepgram)

**What:** Accumulate `is_final: true` transcripts; only emit an utterance when `speech_final: true` arrives.
**When to use:** The interviewer path and any Deepgram client.
**Example:**
```rust
// Source: developers.deepgram.com/docs/understand-endpointing-interim-results
// "Do not use speech_final: true alone to capture full transcripts."
if results.is_final { buffer.push_str(&transcript); }
if results.speech_final { emit_utterance(&buffer); buffer.clear(); }
```

### Anti-Patterns to Avoid

- **Forwarding 讯飞 `wpgs` previews into the translation stage.** A `pgs: "rpl"` frame retroactively rewrites earlier text — anything already sent downstream cannot be recalled. Preview text may only reach the *renderer*.
- **Whole-sentence serial cascading** (`await stt_final(); await translate(); await tts()`). Guaranteed budget breach; the AI-SPEC forbids it explicitly.
- **Unbounded channels between stages.** Latency silently grows and attribution becomes impossible.
- **Doing network I/O inside a cpal callback.** Callbacks run on a realtime-ish thread; only enqueue.
- **A `bool` in place of the breaker state.** Half-open needs a timer + probe; model it explicitly.
- **Writing JSONL from multiple tasks.** Interleaved partial lines corrupt the trace; one writer task owns the file.
- **Treating `trace` numbers as vendor-reported.** The Chinese-path confidence is a local proxy; the schema must say so.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HMAC-SHA256 request signing (讯飞) | Manual hash comparison / string concat | `hmac` + `sha2` (or `ring`) | Constant-time verification, no accidental timing leaks; the exact origin-string format is documented and in the repo script |
| SSE event parsing | `chunk.split('\n')` per TCP chunk | A `\n\n`-boundary buffered parser (or `eventsource-stream`) | This is **failure case 0001**: per-chunk parsing silently dropped the number "800". TCP chunks are not event boundaries |
| Binary WSS framing (火山) | Re-deriving the frame layout | Port `tools/vendor-experiments/volc-tts-stream.mjs`'s `parse()` | Already reverse-engineered against the live service and measured; re-deriving costs a day and reintroduces risk |
| WAV read/write | Manual RIFF header math | hound 3.5.1 | Spec conformance, tested by the same crate family the AEC crate uses |
| Resampling | Linear interpolation | rubato 5.0.0 | Anti-aliasing + exact-ratio fixed mode; hand-rolled resampling produces aliasing artefacts that AEC3 and STT both amplify |
| Echo cancellation | Any custom AEC | webrtc-audio-processing AEC3 | The reference implementation; a bespoke AEC is a multi-month project |
| Circuit breaker | Pull a framework | A ~80-line explicit enum state machine | It must be deterministic and testable with an injected clock; a framework adds a dependency for no benefit |
| JSONL appends | `OpenOptions::append` from callers | One dedicated writer task with an mpsc inbox | Serialized writes, correct rotation, no interleaving |
| Byte-level WSS keepalive timers | Ad-hoc `sleep` loops | tokio interval pinned to a named constant | Deepgram's 10s window is a documented failure mode (NET-0001) |

**Key insight:** Every one of these has already produced a measured failure in this repository's own experiment log. The pipeline's risk is not "can we write it" — it is "do we reproduce a solved problem correctly under latency pressure".

## Common Pitfalls

### Pitfall 1: Speaking a 讯飞 `wpgs` preview
**What goes wrong:** A half-sentence is synthesized and played to the interviewer; it cannot be recalled.
**Why it happens:** `dwa: "wpgs"` deliberately emits provisional text that later frames replace via `rg`. A naive client concatenates every `ws` array it sees.
**How to avoid:** Treat only the `data.status == 2` frame's reconstructed text as committed. Preview frames update the *renderer only*. Enforce this in the commit gate, not at the call sites.
**Warning signs:** Subtitles that flicker/rewrite while audio does not — or audio that contains a phrase the subtitle never showed.

### Pitfall 2: Deepgram `speech_final` misuse
**What goes wrong:** Utterances are truncated or split.
**Why it happens:** The docs warn explicitly: *"Do not use `speech_final: true` alone to capture full transcripts."* Long utterances produce multiple `is_final: true` events before a single `speech_final: true`.
**How to avoid:** Buffer on `is_final`, flush on `speech_final`.
**Warning signs:** English subtitles with gaps at clause boundaries.

### Pitfall 3: Deepgram NET-0001 (the 10-second close)
**What goes wrong:** The socket closes with NET-0001 (usually close code 1011) during silence.
**Why it happens:** No audio and no KeepAlive for 10 seconds.
**How to avoid:** Send `{"type": "KeepAlive"}` as a **text** frame every 3–5 seconds (not 10), and send at least one audio frame within the first 10 seconds — KeepAlive alone does not prevent the initial closure.
**Warning signs:** Mid-session reconnects during interviewer pauses.

### Pitfall 4: Deepgram `language=multi` for Chinese
**What goes wrong:** Mandarin audio is force-mapped onto one of ten non-Chinese languages and returns **garbage with no error**.
**Why it happens:** `multi` covers only English, Spanish, French, German, Hindi, Russian, Portuguese, Japanese, Italian, Dutch. Chinese is not in the set; there is no `detect_language` on the streaming API.
**How to avoid:** The interviewer path is always `language=en`. The user path never touches Deepgram. This validates the dual-vendor split in CLAUDE.md.
**Warning signs:** Nonsense English subtitles when a Chinese question is spoken into the wrong stream.

### Pitfall 5: Mixing the two 火山 auth schemes
**What goes wrong:** 401s against a service that was working an hour ago.
**Why it happens:** Two endpoints, two schemes: `POST /api/v1/tts` uses `X-Api-App-Id` + `X-Api-Access-Key`; `wss://.../api/v3/tts/unidirectional/stream` uses `X-Api-Key`. The header comment at the top of `volc-tts-stream.mjs` still describes the v1 HTTPS variant and is **stale** — the code below it is correct.
**How to avoid:** One client module per endpoint; fix the stale comment when porting; never copy headers between them.
**Warning signs:** Auth failures only on one of the two paths.

### Pitfall 6: The cross-lingual clone trap (highest-impact pitfall)
**What goes wrong:** The clone speaks Chinese perfectly but produces poor or failed English — discovered after the enrollment UI is built.
**Why it happens:** `tone_fidelity` (还原模式) explicitly supports *only* same-language text as the training audio and **does not support cross-lingual synthesis**. English output requires `explicit_language=en` with `tone_fidelity=false`. BytePlus's English Voice Training doc contradicts the Chinese docs outright ("Cross-lingual voice cloning and synthesis are not supported").
**How to avoid:** **Probe first.** Synthesize English through the existing `S_9k337yqg2` clone with `explicit_language=en` + `tone_fidelity=false` and listen, before designing enrollment.
**Warning signs:** Any plan that schedules the enrollment UI before the probe.

### Pitfall 7: 讯飞 payload-size ceiling
**What goes wrong:** Error 10163 mid-stream, killing the session.
**Why it happens:** The base64 `audio` field must stay under 13000 bytes. 1280 B of PCM per 40 ms base64-encodes to ~1708 B, so the ceiling is generous — but batching several 40 ms frames in one `data` payload is not.
**How to avoid:** One 1280-byte chunk per frame at 40 ms, as the repo script does.
**Warning signs:** Sudden 10163 after a buffering "optimization".

### Pitfall 8: `webrtc-audio-processing` `bundled` build prerequisites
**What goes wrong:** `cargo build` fails with *"Failed to execute meson. Do you have it installed?"*
**Why it happens:** macOS has no system `libwebrtc-audio-processing-2`, so dynamic linking is impossible; `bundled` needs `clang`/`gcc` + `pkg-config` + `meson` + `ninja`, and meson fetches abseil-cpp over the network at build time (only `subprojects/abseil-cpp.wrap` ships in the crate).
**How to avoid:** Install toolchain prerequisites as an explicit Wave 0 task; assert availability before the AEC plan is executed.
**Warning signs:** A green build on a developer machine with Homebrew and a red one without.

### Pitfall 9: rubato 5 is a rewrite, not an upgrade
**What goes wrong:** A plan written against `FftFixedIn` / `FftFixedInOut` / `SincFixedIn` compiles nowhere.
**Why it happens:** Those types no longer exist in 5.0. Buffers are now `audioadapter` `Adapter`/`AdapterMut` objects, not `Vec<Vec<f32>>`.
**How to avoid:** Plan against `Fft` / `Async` / `Slip` + `FixedSync::Both`; use `SequentialSliceOfVecs` if planar vectors are convenient.
**Warning signs:** Any task text mentioning `FftFixedIn`.

### Pitfall 10: cpal has no hotplug notification
**What goes wrong:** The session dies on unplug because nothing is listening.
**Why it happens:** There is no `device_changed` callback in cpal 0.18. Recovery is error-driven.
**How to avoid:** Route the stream error callback into a channel; on `DeviceNotAvailable` / `StreamInvalidated`, re-enumerate → re-query configs → rebuild → `play()`. Note streams are created paused and `build_output_stream(..., None)` waits indefinitely.
**Warning signs:** Silent death when headphones are swapped.

### Pitfall 11: AEC frame-size panic
**What goes wrong:** The audio thread panics.
**Why it happens:** `process_capture_frame` / `process_render_frame` **panic** if the sample count is not exactly 10 ms at the constructor's sample rate (480/channel @48k). A resampler chunk-size bug becomes a crash inside a callback.
**How to avoid:** Assert `len == processor.num_samples_per_frame()` at the boundary; never `unwrap()` the panic path into the callback.
**Warning signs:** Crashes correlated with unusual device buffer sizes.

### Pitfall 12: Missing epoch check after an `await`
**What goes wrong:** Stale audio plays after a barge-in — the exact criterion-4 failure.
**Why it happens:** The epoch is checked when a chunk is produced but not after the queue pop, the player-ready wait, the PCM write, or the AEC reference push.
**How to avoid:** Check the epoch after *every* async boundary, not once at the top.
**Warning signs:** Overlapping audio only under load.

### Pitfall 13: `cargo test --workspace` does not exist here
**What goes wrong:** The AI-SPEC's CI line fails.
**Why it happens:** The repo has **no Cargo workspace** — only `apps/desktop/src-tauri/Cargo.toml`.
**How to avoid:** Use `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` in every plan and CI task.

### Pitfall 14: JSONL trace on the broadcast path
**What goes wrong:** The phone lags or the trace loses lines.
**Why it happens:** `SessionState`'s broadcast channel has a ring buffer of 64 (WR-01 `Lagged` handling exists for a reason). Trace events are per-sentence × 3 stages — an order of magnitude more than UI events.
**How to avoid:** The trace writer subscribes to its own channel (or is called directly in the pipeline), not to the UI broadcast.
**Warning signs:** `Lagged` warnings during long sessions.

## Code Examples

### 讯飞 iat: signature + first frame
```javascript
// Source: tools/vendor-experiments/stt-ab.mjs (repo, verified against the live service)
//         + xfyun.cn/doc/asr/voicedictation/API.html
const host = 'iat-api.xfyun.cn';
const date = new Date().toUTCString();                       // RFC1123, UTC only (300s skew window)
const origin = `host: ${host}\ndate: ${date}\nGET /v2/iat HTTP/1.1`;
const signature = crypto.createHmac('sha256', API_SECRET).update(origin).digest('base64');
const auth = Buffer.from(
  `api_key="${API_KEY}", algorithm="hmac-sha256", headers="host date request-line", signature="${signature}"`
).toString('base64');
const url = `wss://${host}/v2/iat?authorization=${encodeURIComponent(auth)}` +
            `&date=${encodeURIComponent(date)}&host=${host}&appid=${APP_ID}`;
// Per-frame payload (1280 B = 40 ms @16kHz/16-bit):
// { common:{app_id}, business:{language:'zh_cn',domain:'iat',accent:'mandarin',dwa:'wpgs'},
//   data:{status: 0|1|2, format:'audio/L16;rate=16000', encoding:'raw', audio: <base64>} }
```

### Deepgram: connect + finality buffering
```javascript
// Source: developers.deepgram.com/reference/speech-to-text/listen-streaming
// wss://api.deepgram.com/v1/listen?model=nova-3&language=en&encoding=linear16
//     &sample_rate=16000&channels=1&interim_results=true&endpointing=300
//     &punctuate=true&smart_format=true&vad_events=true&utterance_end_ms=1000
// Header: Authorization: Token <key>          <- scheme word is "Token", not "Bearer"
// Media frames: raw binary. Control frames: {"type":"Finalize"|"CloseStream"|"KeepAlive"} as TEXT.
// KeepAlive every 3-5s (10s window -> NET-0001); >=1 audio frame required in the first 10s.
```

### 火山 TTS: WSS frame + parse (port directly into Rust)
```javascript
// Source: tools/vendor-experiments/volc-tts-stream.mjs (repo, measured 1.3s / MOS 5.0)
const URL = 'wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream';
const payload = Buffer.from(JSON.stringify({
  user: { uid: 'nexTalk' },
  req_params: {
    text, speaker: VOICE,
    audio_params: { format: 'pcm', sample_rate: 24000 },   // pcm for streaming (wav discouraged)
  },
}));
const frame = Buffer.concat([Buffer.from([0x11, 0x10, 0x10, 0x00]), u32be(payload.length), payload]);
// handshake headers: X-Api-Key / X-Api-Resource-Id: seed-icl-2.0 / X-Api-Request-Id: uuid
// parse: msgType = (data[1] >> 4) & 0x0f;  0b1111 error | 0b1011 audio | 0b1001 json
// non-error layout: event u32@4, sid_len u32@8, sid, payload_len u32, payload
// event 352 = TTS_RESPONSE (audio), 152 = SESSION_FINISHED (check json.status_code == 20000000)
```

### 火山: the cross-lingual synthesis parameters (the fragile part)
```javascript
// Source: volcengine 音色查询HTTP / 声音复刻API docs (via mirror) + web search 2026-09-29
req_params: {
  text: englishText,
  speaker: 'S_xxx',                  // ICL 2.0 clone
  audio_params: {
    format: 'pcm', sample_rate: 24000,
    explicit_language: 'en',         // REQUIRED: "输入文本须包含指定语种的内容，否则请求将无法正常返回"
    tone_fidelity: false,            // REQUIRED: 还原模式 does NOT support cross-lingual synthesis
  },
}
// Training side stays Chinese: voice_clone body { language: 0 }  (language = training audio language, not target)
// X-Api-Resource-Id: seed-icl-2.0 for clones; seed-tts-2.0 for preset voices
```

### webrtc-audio-processing: the 10 ms loop
```rust
// Source: github.com/tonarino/webrtc-audio-processing examples/simple.rs + src/lib.rs
use webrtc_audio_processing::*;
use webrtc_audio_processing_config::{Config, EchoCanceller, NoiseSuppression};

let ap = Processor::new(48_000)?;                       // 480 samples per channel per frame
ap.set_config(Config {
    echo_canceller: Some(EchoCanceller::default()),
    noise_suppression: Some(NoiseSuppression::default()),
    ..Default::default()
});
let n = ap.num_samples_per_frame();                     // 480 @48k — panics if mismatched
assert_eq!(render[0].len(), n);

ap.process_render_frame(&mut render_out)?;              // render is NOT modified (the crate asserts this)
ap.process_capture_frame(&mut capture_out)?;            // capture IS modified (echo removed)
// Frames are planar: Vec<channel<Vec<f32>>>, NOT interleaved.
```

### rubato 5: exact-ratio low-latency resampling
```rust
// Source: docs.rs/rubato/latest — 5.0.0 API (completely rewritten from the pre-1.0 names)
use rubato::{Fft, FixedSync, Resampler, Indexing};

// 48 kHz processing -> 16 kHz STT input: exactly 3:1
let mut down = Fft::<f32>::new(48_000, 16_000, 480, 1, FixedSync::Both)?;   // chunk 480 -> 160
// 24 kHz TTS output -> 48 kHz playback: exactly 2:1
let mut up = Fft::<f32>::new(24_000, 48_000, 240, 1, FixedSync::Both)?;     // chunk 240 -> 480
// FixedSync::Both skips internal buffering ("sizes are calculated to fit the resampling ratio exactly")
// Always: let need = r.input_frames_next();  before process_into_buffer(...)
// Realtime-safe path is process_into_buffer on pre-allocated buffers; process() does NOT trim startup delay.
```

### Barge-in: epoch-guarded playout
```rust
// Source: Amadeus barge_in_aec_interrupt_notes.md + Zenodo "Where the Latency Actually Goes"
struct Playout { epoch: Arc<AtomicU64>, ring: RingBuffer<AudioChunk> }

impl Playout {
    fn interrupt(&self) {                                   // called from the VAD path
        self.epoch.fetch_add(1, Ordering::SeqCst);          // bump BEFORE flushing
        self.ring.clear();                                  // flush queued chunks
    }
    fn try_push(&self, chunk: AudioChunk) -> bool {
        if chunk.epoch != self.epoch.load(Ordering::SeqCst) { return false; }  // stale -> drop
        self.ring.push(chunk)
    }
}
// Instrument three numbers for the 02-01 waterfall:
//   interruption_detection_ms, interruption_to_silence_ms, total_barge_in_ms
```

---

## Deep Dive: The Eight Research Topics

## Topic 1 — 讯飞流式听写 (iat) WSS client

**Confidence: HIGH** (official spec at `xfyun.cn/doc/asr/voicedictation/API.html`, cross-checked against the repo's working `stt-ab.mjs`)
**macOS 12.7 risk: NONE** — pure TLS WebSocket to a public endpoint; no OS-version dependency.

### Endpoints and session limits
- Primary: `wss://iat-api.xfyun.cn/v2/iat`. Alternates: `ws-api.xfyun.cn` (Chinese/English), `iat-niche-api.xfyun.cn` (minority languages). The spec warns **not to hard-code IPs** — server addresses are not fixed.
- Max session **60 s**; the server drops the connection after **10 s without data** (error 10200).

### Signature (the part that must be exactly right)
1. `date` = RFC1123, **UTC+0/GMT only** (e.g. `Wed, 10 Jul 2019 07:35:43 GMT`). The server tolerates **at most 300 s of clock drift** (403 otherwise).
2. `signature_origin` = three lines joined with `\n`, **one space after each colon**: `host: $host\ndate: $date\nGET /v2/iat HTTP/1.1`.
3. `signature` = base64(HMAC-SHA256(`signature_origin`, `apiSecret`)).
4. `authorization_origin` = `api_key="$api_key",algorithm="hmac-sha256",headers="host date request-line",signature="$signature"` — note `headers` lists parameter **names**, not values.
5. `authorization` = base64(authorization_origin), then **URL-encoded** into the query string.
6. Query: `?authorization=…&date=…&host=…&appid=…`. HTTP 101 = upgrade succeeded.

Failure modes: 401 `{"message":"Unauthorized"}` / `"HMAC signature cannot be verified"` / `"HMAC signature does not match"`; 403 clock skew or IP allow-list (`"Your IP address is not allowed"`).

### Request framing
All frames are **JSON** (not binary). `common` and `business` are sent **only in the first frame** after the handshake; `data` is required in **every** frame.

| Field | Required | Notes |
|---|---|---|
| `common.app_id` | yes | APPID |
| `business.language` | yes | `zh_cn` / `en_us` / licensed minority |
| `business.domain` | yes | `iat` |
| `business.accent` | yes | `mandarin` |
| `business.eos` | no | silence timeout, **default 2000 ms** — this is the endpointing knob |
| `business.dwa` | no | `"wpgs"` enables dynamic correction (Chinese only, off by default) |
| `business.ptt` / `nunum` / `nbest` / `wbest` / `vinfo` | no | punctuation, Arabic numerals, sentence/word candidates, VAD offsets |

`data.status`: **0 = first audio frame, 1 = middle, 2 = final** (mandatory). A bare `{"data":{"status":2}}` signals session end.
`data.format`: `audio/L16;rate=16000` (or 8000). `data.encoding`: `raw` (or `speex`, `speex-wb`, `lame` for zh/en).

### Chunk sizing (exact)
- Send **every 40 ms**; each send must be an integer multiple of one audio frame.
- **1280 bytes of PCM per 40 ms** at 16 kHz/16-bit.
- The base64 `audio` field must stay **under 13000 bytes** or error **10163** fires.

### `dwa="wpgs"` reconstruction (the subtle part)
Fields: `data.result.pgs` and `data.result.rg`.
- `"apd"` — "该片结果是追加到前面的最终结果" → append.
- `"rpl"` — this result **replaces** prior output; `rg` gives the range of `sn` values replaced. E.g. `"rg":[2,5]` replaces results 2–5.
- **Reconstruction rule:** keep an ordered list keyed by `data.result.sn`; on `apd` append the new `ws` text; on `rpl` overwrite the entries whose `sn` falls inside `rg`, leaving the rest intact.
- `vinfo` is void when `wpgs` is enabled.
- **Consequence for this project:** any text emitted from a non-`status==2` frame is *provisional and may be retroactively rewritten*. It may never reach the translation/TTS path (see Topic 7).

### `result.ws` / `cw` and the confidence problem
`data.result` carries `sn`, `ls`, `bg`, `ed`. Each `ws` entry has `bg` (frame offset, 1 frame = 10 ms) and `cw[]`; each `cw` element has `w` (the word) plus `sc`, `wb`, `wc`, `we`, `wp`.

**No confidence score is documented.** In the official spec, `sc`/`wb`/`wc`/`we`/`wp` are classed as `均为保留字段，无需关心` ("all reserved fields, do not care about them"), and the spec adds that *if* you do parse `sc` you must accept both float and int. Every sample shows `sc: 0`.

**Impact on GOV-01:** the "STT 置信度" factor has **no vendor source on the Chinese path**. Options (Claude's Discretion per CONTEXT.md):
- (a) Define a **local proxy** from signals we do have: wpgs revision churn (how many `rpl` events a segment attracted), audio level / SNR at the VAD boundary, and term-hit rate.
- (b) Use the translation model's own confidence for the combined score and drop the STT factor to a binary gate.
- Recommendation: (a), with an explicit `confidenceSource: "proxy" | "vendor"` field on the trace so a proxy is never mistaken for a vendor number. The **interviewer path** does have a real vendor confidence (Deepgram `alternatives[].confidence`), so the two paths should not share a schema field without that discriminator.

### Error-code table (drives the D-09 retry classifier and the trace `errorCode`)

| Class | Codes |
|-------|-------|
| **Retryable** (transient) | 10014/10114 (session timeout), 10019 (session timeout / empty audio), 10043/10044 (decode / rate reset), 10047, 10200 (read timeout), 10222 (network), 10303, 10500/10600/10700 (internal/engine) |
| **Terminal — do not retry, surface + degrade** | 10005 (appid auth), 10010/10110 (insufficient authorization), 10163 (param length), 10160/10161 (malformed request), 10165 (frame order), 11200 (quota/unauthorized/expired), 11201/11202 (rate limits), 11203 (authorization expired) |
| **Client bug — fix the code** | 10006/10007 (sample rate), 10009/10109 (invalid data), 10101 (engine session already ended — client kept sending), 10313, 10317, 10404 |

Other limits from the FAQ: 50 concurrent channels by default; max 2000 app-level hotwords; 60 s max audio.

---

## Topic 2 — Deepgram Nova-3 streaming WSS

**Confidence: HIGH** for the protocol (official reference page); **MEDIUM** for the `multi`-excludes-Chinese finding (docs page + LiveKit community thread); **LOW/`[ASSUMED]`** for interim-result billing behaviour.
**macOS 12.7 risk: NONE** — public TLS WebSocket.

### Endpoint and auth
- `wss://api.deepgram.com/` + channel `/v1/listen` → **`wss://api.deepgram.com/v1/listen`**.
- Auth is a **WebSocket header**: `Authorization: Token YOUR_DEEPGRAM_API_KEY`. The scheme word is literally **`Token`**, not `Bearer` (this differs from MiniMax/Cartesia, which use `Bearer`).
- `/auth/token` can verify a key; short-lived tokens exist via the Auth API (not documented for WS specifically).

### Query parameters (verified against the streaming reference)

| Param | Default | Notes |
|-------|---------|-------|
| `model` | — | `nova-3`, `nova-3-general`, `nova-3-medical`, `nova-2` variants, `base`, … Omitting it falls back to `model=base` |
| `language` | `en` | ISO-639 style; **no `detect_language` on streaming** (batch-only) |
| `interim_results` | `'false'` | ongoing updates as audio arrives |
| `endpointing` | `'10'` | ms of silence before finalizing speech; **recommended 300–500 for speakers who pause mid-thought**; `false` disables |
| `utterance_end_ms` | — | emits `UtteranceEnd` after this much silence |
| `vad_events` | `'false'` | emits `SpeechStarted` |
| `punctuate` | `'false'` | |
| `smart_format` | `'false'` | |
| `encoding` | — | `linear16`, `opus`, `mulaw`, … |
| `sample_rate` | — | set explicitly to avoid resampling |
| `channels` | `'1'` | |
| `keyterm` | — | nova-3 keyword boosting (the future glossary hook, Phase 4) |
| `mip_opt_out` | `'false'` | data-usage opt-out |
| `numerals`, `profanity_filter`, `redact`, `diarize`, `multichannel`, … | — | |

**Correction to a common assumption:** there is **no `keepalive` query parameter**. KeepAlive is a client *message*.

### Server → client messages
- **Results** (required: `type`, `channel_index`, `duration`, `start`, `channel`, `metadata`; optional `is_final`, `speech_final`, `from_finalize`, `entities`). `channel.alternatives[]` has `transcript`, `confidence`, `words[]` (each `word`/`start`/`end`/`confidence`/`punctuated_word`/`speaker`), and `languages[]` on multilingual responses.
- **Metadata** — `request_id` (uuid), `sha256`, `created`, `duration`, `channels`; nested `metadata` on Results carries `model_uuid` and `model_info {name, version, arch}`.
- **UtteranceEnd** — `{type, channel:[0], last_word_end}`.
- **SpeechStarted** — `{type, channel:[0], timestamp}`.

**For D-08 (`modelVersion` per sentence):** `metadata.model_info.name` + `.version` + `.arch` is a genuine per-result vendor model identifier — prefer it over a hard-coded string.

### Client → server messages
- **Media**: a **binary** frame of raw audio — no JSON envelope.
- **Control**: JSON, each requiring only `type`: `{"type":"Finalize"}`, `{"type":"CloseStream"}`, `{"type":"KeepAlive"}`.

### `is_final` vs `speech_final` (the rule the pipeline depends on)
- `is_final: true` = "Finalized transcript for this audio segment."
- `speech_final: true` = "the speaker has paused" — endpointing fired.
- **Buffer `is_final`, flush on `speech_final`.** Official warning: *"Do not use `speech_final: true` alone to capture full transcripts."* Long utterances produce multiple `is_final: true` results before one `speech_final: true`.
- Recommended combos: chatbots/short utterances → default 10 ms; speakers who pause mid-thought → 300–500 ms; sentence segmentation → `interim_results=true` + `endpointing=300` + `punctuate=true`.

### KeepAlive and NET-0001 (operational requirement)
- **10-second window.** *"If no audio data or KeepAlive messages are sent within a 10-second window, the connection will close with a NET-0001 error"* — usually surfaced as WebSocket close code **1011**.
- **Send every 3–5 seconds** — the official examples use 3 s; a Dart package uses 8 s; a Rust client uses 5 s and observed a hard close at ~12 s. Never wait the full 10 s.
- Must be sent as a **text** frame: *"sending it as binary may result in incorrect handling and potential connection issues."* The server **does not respond**.
- **KeepAlive alone will not prevent closure — at least one audio message must be sent within ~10 s of opening**, otherwise the socket is closed regardless.
- Send `CloseStream` when finished so buffered audio is processed.
- Word timings follow the audio stream, not the socket lifetime, so KeepAlive gaps do not shift timestamps.

### Mandarin support and the `language=multi` trap
- Nova-3 **does** support Mandarin monolingually: `zh`, `zh-CN`, `zh-Hans` (Simplified output), `zh-TW`, `zh-Hant` (Traditional), `zh-HK` (Cantonese Traditional on Nova-3). Reported 65.21% relative WER reduction on Simplified Mandarin batch vs Nova-2.
- `language=multi` (Multilingual Code-Switching) covers only **10 languages: English, Spanish, French, German, Hindi, Russian, Portuguese, Japanese, Italian, Dutch**. **Chinese is not included.**
- Consequence: `model=nova-3&language=multi` on Chinese audio **force-maps Mandarin onto a non-Chinese language and returns garbled output with no error** ("silently degrades"). Streaming has no `detect_language`, and the no-language default is English.
- Guidance for this project: the interviewer path uses **`language=en`** explicitly. Never `multi`. If bilingual interviewer audio ever needs code-switched handling, that is a different vendor question, not a Deepgram config flag.

**Billing caveat `[ASSUMED]`:** the ROADMAP research note warns about "interim-result billing amplification (Deepgram per-message)". I could not verify Deepgram's interim-result billing model in this session. Treat per-message billing as unverified and measure it in 02-01 by counting messages per session against a known invoice.

---

## Topic 3 — Barge-in queue pattern

**Confidence: MEDIUM** (industry sources + one academic paper; barge-in is not standardized, so no official spec exists)
**macOS 12.7 risk: LOW** — depends only on the local VAD and the playout ring.

### The core mechanism: epoch-based logical ownership
The strongest, most transferable pattern found (Amadeus `barge_in_aec_interrupt_notes.md`, corroborated by the Zenodo paper *"Where the Latency Actually Goes — Deterministic State Machines, Barge-In, and Cancellation in Full-Duplex Voice Agents"*):

1. `interrupt()` **increments the epoch before clearing queues** — the bump is what makes already-dispatched work stale.
2. Every consumer checks its item's epoch after **every** async boundary: pulling from the pending buffer, waiting for the player to be ready, starting physical playback, writing PCM, pushing the AEC reference frame, and the turn-complete callback.
3. Anything whose epoch no longer matches is **discarded, never played**.

This matches the repo's existing `session_epoch` (`Arc<AtomicU64>` in `state.rs`) — the same concept, but Phase 2 needs a **separate playback epoch** (or a strictly defined relationship to the session epoch) because a barge-in is not a session restart.

### Cancellation semantics (server side)
- Barge-in **abandons** the turn: cancel generation mid-stream, drop queued-but-unspoken text, emit **no final frame**, and acknowledge with an interrupted marker instead.
- **Stop local playback immediately when you call cancel — do not wait for the acknowledgement.** A few audio frames already in transit may still arrive after the cancel; the ack marks the point after which nothing more comes.
- Where possible, cancel **without closing the WebSocket** (reopening a 火山/Deepgram session costs a handshake inside the latency budget).

### Cancellation semantics (client side)
- The canonical shape is an `AbortController`-equivalent that propagates to: pending TTS requests, the currently-playing source node, **and the queue of remaining chunks**.
- Acceptance criteria observed in the wild (Koi issue #829): abort playback **within ~100 ms**, no wasted API calls, a clean new turn, and **no audio artifacts (clicks, pops)**.
- Avoid chained `play(chunk)` calls — play one continuous stream and check a cancellation flag **between chunks**, which is what prevents boundary glitches (voice-agents-from-scratch).

### Detection thresholds — and the "100 ms" clarification
**Important:** the search found **no source describing a 100 ms silence requirement for trigger**. The numbers found are different things:
- **~100 ms** = the *abort latency target* (Koi acceptance criteria) — i.e. once barge-in is decided, playback must stop within ~100 ms.
- **300 ms** = telephony default `minSpeechDuration` before a barge-in is accepted.
- **300 ms hard silence** required before the agent re-speaks (Future AGI fix for the "cancel-and-restart loop")
- **350 ms** = extended minimum-duration guard that fixed a ~200 ms repeat-interrupt loop.
- **30 ms** = typical VAD frame chunk (Silero VAD: 2 MB model, 30 ms chunks, <1 ms per chunk on one CPU thread).

**Mapping to our criterion 4** ("English output stops within ~100ms with no overlapping audio"): `~100 ms` is best read as **abort-to-silence latency**, measured from the barge-in decision to the last emitted sample. A separate, larger **minimum-speech guard (300–350 ms)** must gate the *decision*, or the detector will trigger on the user's own breath/pauses and produce a cancel-restart loop. Design them as two independent constants, not one.

### Self-echo and AEC
- AEC must be fed the **TTS PCM as the render reference**, otherwise the system hears its own cloned voice through the microphone and false-triggers barge-in.
- Relevant knobs from the same source: `AEC_REALTIME_DELAY_MS`, and an ASR echo tail guard of ~650 ms.
- The barge-in detector should not exit during tiny sentence gaps — keep a short **TTS idle grace window** and apply cooldown only after a real trigger.

### Metrics to instrument in 02-01
1. `interruption_detection_ms` — speech onset → barge-in decided.
2. `interruption_to_silence_ms` — last old audio sample − barge-in confirmed. **This is the number criterion 4 asserts.**
3. `total_barge_in_ms` — last old audio sample − new speech onset.

### History alignment
Annotate the interrupted turn (`[interrupted by user]`) rather than silently truncating, and emit an explicit interruption event to the UI. Both sources flag a *missing* event as the root cause of stale UI after a barge-in — relevant here because the desktop pane and phone H5 both render the subtitle stream.

---

## Topic 4 — `webrtc-audio-processing` ~2.0: real API shape

**Confidence: HIGH** (source read directly from `github.com/tonarino/webrtc-audio-processing`: `examples/simple.rs`, `src/lib.rs`, `src/stats.rs`, `webrtc-audio-processing-config/src/lib.rs`, `webrtc-audio-processing-sys/build.rs` + `Cargo.toml` + `README.md`, plus the crates.io metadata).
**macOS 12.7 risk: HIGH for the build path, LOW for the runtime.** See the build section.

### The actual public API (0.5.0-era docs.rs names are wrong — `docs.rs` failed to build 2.1.0, so go to the source)

```rust
impl Processor {
    pub fn new(sample_rate_hz: u32) -> Result<Self, Error>;
    pub fn with_aec3_config(/* + experimental-aec3-config */) -> Result<Self, Error>;
    pub fn process_capture_frame<F, Ch>(&self, frame: F) -> Result<(), Error>;   // mic; IS modified
    pub fn process_render_frame<F, Ch>(&self, frame: F) -> Result<(), Error>;    // playback; NOT modified
    pub fn analyze_render_frame<F, Ch>(&self, frame: F) -> Result<(), Error>;    // playback; &self, no mutation
    pub fn set_config(&self, config: Config);
    pub fn get_stats(&self) -> Stats;
    pub fn num_samples_per_frame(&self) -> usize;
    pub fn reinitialize(&self);
    pub fn set_output_will_be_muted(&self, muted: bool);
    pub fn set_stream_key_pressed(&self, pressed: bool);
}
```

Key facts:
- Frame shape is **non-interleaved (planar)**: `frame` is "mutable iterator/Vec/array/slice of channels, which are Vecs/arrays/slices of `f32` samples" — i.e. `Vec<Vec<f32>>` channel-major. (One third-party consumer describes interleaved frames on v0.4.0; the 2.x documentation is planar.)
- **`# Panics`: "Panics if the number of samples doesn't match 10 ms @ `sample_rate_hz` passed to constructor."** 480 samples/channel at 48 kHz. In a cpal callback this is a crash, not an error.
- The render frame is **not modified** — the crate's own example asserts `render_frame == render_frame_output`.
- `Processor` is the thread-safe wrapper (the raw `AudioProcessing` FFI pointer is wrapped and `unsafe impl Send/Sync` for the pointer only).
- The processor "dynamically adapts to the number of channels at the cost of partial reinitialization" — so channel-count changes are legal but not free.

### `Config` (from `webrtc-audio-processing-config` — pure Rust, NO FFI)

```rust
pub struct Config {
    pub pipeline: Pipeline,
    pub capture_amplifier: Option<CaptureAmplifier>,
    pub high_pass_filter: Option<HighPassFilter>,
    pub echo_canceller: Option<EchoCanceller>,       // enum EchoCanceller (AEC3 / AECM variants)
    pub noise_suppression: Option<NoiseSuppression>, // struct + NoiseSuppressionLevel
    pub gain_controller: Option<GainController>,     // GainController1 | AnalogGainController | GainController2
}
pub struct Pipeline {
    pub maximum_internal_processing_rate: PipelineProcessingRate, // Max32000Hz | Max48000Hz (default)
    pub multi_channel_render: bool,
    pub multi_channel_capture: bool,
    pub capture_downmix_method: DownmixMethod,
}
```
The config crate exists so that configuration can be built in a WASM/no-FFI context — useful for testing config permutations without linking C++.

### **VAD is NOT exposed** — this corrects CLAUDE.md
- Grepping the config crate for `voice`/`vad` returns **nothing**; there is no `voice_detection` field in `Config`.
- The FFI `Stats` struct does contain `voice_detected`, but the safe `Stats` wrapper **drops it** — the public `Stats` exposes only `echo_return_loss`, `echo_return_loss_enhancement`, `residual_echo_likelihood`, `residual_echo_likelihood_recent_max`, `delay_ms`.
- The crate's own C++ tests **assert `!stats.voice_detected.has_value`**, i.e. upstream never populates it.
- Therefore **CLAUDE.md's "AEC3 + NS + AGC + VAD" is wrong about VAD.** Voice activity detection must come from another source: Deepgram's `vad_events`/`endpointing`, 讯飞's `eos`/`vinfo`, a local energy-based VAD, or `silero-vad-rs` (which carries its own viability risk — see Topic 6 / Environment Availability).

### Other `Stats` caveats
`residual_echo_likelihood` and `residual_echo_likelihood_recent_max` are documented **"Always `None` when not using the `bundled` feature."** So if the AEC is dynamically linked, half the observability is unavailable. `delay_ms` (instantaneous AEC delay estimate, ms) is usable and is a good latency-rig input.

### Versioning and pinning
- Latest: **2.1.0, published 2026-05-13**. The workspace uses `edition = "2024"`.
- **Not semver:** "version `2.3` can introduce an API-breaking change over `2.2`. Patch versions are backwards compatible." The README explicitly recommends a tilde requirement: `webrtc-audio-processing = "~2.0"`.
- Major version tracks the PulseAudio WebRTC AudioProcessing upstream, not the Rust API.
- Direct deps: `webrtc-audio-processing-config ^2.1.0`, `webrtc-audio-processing-sys ^2.1.0`.

### Build paths on macOS
Two paths, and **only one is viable on macOS**:

**A. Default — dynamic linking (NOT viable on macOS).** `build.rs` runs `find_pkgconfig_paths()` and **bails** with *"Couldn't find libwebrtc-audio-processing-2. Please install it or set WEBRTC_AUDIO_PROCESSING_INCLUDE and WEBRTC_AUDIO_PROCESSING_LIB"*. There is no macOS system package (the documented packages are Ubuntu/Debian `libwebrtc-audio-processing-dev` and Arch `webrtc-audio-processing`).

**B. `bundled` — build the vendored C++ (the macOS path).** Verified facts from reading the crate tarball and `build.rs`:
1. **The C++ source IS vendored in the crates.io tarball.** `webrtc-audio-processing-sys-2.1.0/webrtc-audio-processing/` contains **666 files** — so the README's "clone recursively" warning applies only to building from git, not from crates.io.
2. **Required tools** (from the README): `clang` or `gcc`, **`pkg-config`**, **`meson`**, **`ninja-build`**. `build.rs` asserts on failure with *"Failed to execute meson. Do you have it installed?"* / *"Failed to execute ninja. Do you have it installed?"*.
3. **abseil-cpp is NOT vendored** — only `subprojects/abseil-cpp.wrap` ships. `build.rs`'s comment: *"Otherwise use the local build fetched and built by meson"* (`abseil-cpp-20240722.0`). **Meson will fetch it over the network at build time.** Consequence: the first build requires network access, and a fully offline build fails.
4. `build.rs` tolerates a missing system abseil: it probes `pkg_config::Config::new().atleast_version("20240722").probe("absl_base")` and falls back to the meson-fetched copy if that fails — so **pkg-config itself may be optional** at the Rust level, but meson's own dependency resolution should be verified. `[ASSUMED]` — not confirmed.
5. **Symbol prefixing:** under `bundled`, all C++ symbols are prefixed with **`v2_`** using `objcopy --redefine-sym`, where objcopy is `rust-objcopy` resolved from the Rust sysroot (`<sysroot>/lib/rustlib/<HOST>/bin/rust-objcopy`). **Present on this machine** for `x86_64-apple-darwin`.
6. **macOS deployment target:** `build.rs` reads `MACOSX_DEPLOYMENT_TARGET` and passes `-mmacos-version-min=<ver>` to the C++ build, **defaulting to 10.10 (x86_64) / 11.0 (aarch64)** when unset. This crate *does* honor the variable CLAUDE.md already tells us to set — so `MACOSX_DEPLOYMENT_TARGET=12.0` must actually be exported in the build environment, not just written in a config file.
7. `experimental-aec3-config` **activates `bundled`** and needs private WebRTC headers. `experimental-unlink-ns` also activates `bundled` and needs `patch` installed.
8. Meson is invoked as `meson setup --prefix <OUT_DIR> --reconfigure`, then `ninja`, then `ninja install`; the sources are first `cp -a`'d into `OUT_DIR` so patching does not touch the registry cache.

### Where this lands in the plan
- The `bundled` build's prerequisites are **not installed on the target machine** (see Environment Availability). A Wave 0 task must install/verify `meson`, `ninja`, and (probably) `pkg-config` before the AEC task runs.
- Consider scoping AEC (02-05) last in the wave order so the rest of the pipeline can be validated while the native toolchain is sorted out. The AI-SPEC's "audio/" module list assumes AEC3 is available; the pipeline does not *require* it to reach criterion 1–3.

---

## Topic 5 — cpal audio graph on macOS 12.7

**Confidence: HIGH** for version/API facts (docs.rs + crates.io); **MEDIUM** for the "no hotplug notification API" negative claim (verified by absence across docs.rs, the crate README and community code — flagged per the negative-claim rule).
**macOS 12.7 risk: LOW-MEDIUM.** CoreAudio is stable at 12.7; the risk is in the `objc2`-based dependency chain and must be confirmed by an actual build on this machine.

### Version and model
- **cpal 0.18.2**, Apache-2.0, published **2026-08-16**, MSRV **1.85** (local rustc is 1.98.0 — fine).
- Model: **Host → Device → Stream**. `HostTrait::default_input_device()` / `default_output_device()` return **`Option<Device>`** — `None` means no device of that type.
- `HostTrait::devices()` / `input_devices()` / `output_devices()` return `Result`s.
- `DeviceTrait::supported_input_configs()` / `supported_output_configs()` **"could return an error … if the device has been disconnected"** — so post-unplug config queries can fail.
- Streams are created **paused**; `StreamTrait::play()` / `pause()` control them. Callbacks run on a dedicated high-priority thread; creating a stream does not block.

### Error handling (0.18's unified `Error` / `ErrorKind`)
Hot-plug-relevant kinds:
| Kind | Meaning | Action |
|------|---------|--------|
| `DeviceNotAvailable` | device disconnected or missing | re-enumerate, re-acquire |
| `DeviceChanged` | audio route changed (e.g. headphones unplugged) | often **auto-reroutes** — informational |
| `StreamInvalidated` | configuration no longer valid | **triggers a stream rebuild** |
| `DeviceBusy` | transient | retry after a short delay |
| `HostUnavailable`, `PermissionDenied`, `UnsupportedConfig`, `BackendError`, `Xrun` | — | surface / degrade |

**Two error channels:** synchronous setup calls (`Host::new()`, `devices()`, `build_output_stream()`) return `Result<T, Error>`; **asynchronous errors during playback arrive via the error callback** passed to `build_input_stream`/`build_output_stream`. Documented examples merely `eprintln!` them — for this project the callback must **send into a channel** that a supervisor task consumes, or hot-plug recovery never happens.
Pre-0.18 used per-operation error types (`BuildStreamError`, `StreamError`, `BackendSpecificError`, …) — any snippet found online using those is for the older API.

### Hot-plug: there is no notification API
No `device_changed` callback or hotplug listener is documented. The implied (and only) pattern is **error-driven rebuild**:
`detect (None from default_*_device | config-query error | error callback firing DeviceNotAvailable/StreamInvalidated) → re-enumerate → re-acquire device → re-query configs (or take the default) → build_*_stream → play()`.

`StreamInvalidated` is the explicit rebuild trigger. This is the design 02-05 ("device hot-change does not kill the session") must implement; there is nothing to subscribe to.

### macOS specifics
- Backend is CoreAudio; Apple-target dependencies listed: `coreaudio-rs ^0.14.2`, `mach2`, `objc2`, `objc2-audio-toolbox`, `objc2-core-audio`, `objc2-core-audio-types`, `objc2-core-foundation`, `objc2-foundation`, `block2`, `objc2-avf-audio`.
- `build_*_stream(..., timeout)` — `None` means **"wait indefinitely"**. In a UI app prefer `Some(Duration)` so a wedged backend cannot freeze startup.
- Virtual drivers on macOS (Camo, Loom, Zoom, Teams — and later BlackHole) report `supports_output() == true`, so input-device categorisation by capability alone is unreliable; Phase 3 will need name-based logic. Relevant now only as a reason not to write capability-only device selection.
- When input and output are the **same physical device**, build both streams from the **same `Device` object** (community guidance, MEDIUM).

### The resampling chain (cpal × rubato × AEC)
```
mic native (44.1k or 48k)  ──rubato──▶  48k  ──AEC3 (10 ms / 480 samples)──▶  VAD
                                              └──rubato (3:1, exact)──▶ 16k  ──▶ 讯飞 (L16;rate=16000)

火山 TTS (24k pcm)  ──rubato (2:1, exact)──▶ 48k  ──▶ playout ring ──▶ cpal output
                                   └── also mirrored to the AEC render reference
```
AEC3 requires a fixed 10 ms frame at its construction rate — so **48 kHz is the natural graph rate** and every boundary crossing must preserve frame alignment. Getting 480 samples exactly right is what makes `FixedSync::Both` attractive for the 3:1 and 2:1 cases.

---

## Topic 6 — Low-latency playback buffering (jitter buffer ≤200 ms)

**Confidence: LOW — design guidance, not verified vendor/standards material.** No authoritative specification for a TTS playout jitter buffer was located in this session; what follows is a design recommendation derived from the verified barge-in sources (Topic 3) and standard streaming practice. The planner should treat the specific numbers as starting points to be tuned by the 02-01 rig, not as researched constants.

### Why a buffer exists at all here
TTS audio arrives as discrete WSS frames (火山) whose inter-arrival time varies with network jitter, while the cpal output callback demands a fixed number of samples at a fixed cadence. Without a buffer, every late frame is a dropout (audible click or gap). Without a bound, latency grows without limit and the 2 s budget silently dies.

### Recommended shape
- **Single producer / single consumer ring buffer** of f32 PCM at 48 kHz, sized in milliseconds (not samples) so the budget is legible.
- **Pre-roll before the first output**: do not start draining until the ring holds a minimum depth. Starting at depth 0 means the first underrun is guaranteed.
- **Target depth** = observed p95 frame inter-arrival + one frame, not a fixed number. Measure it in 02-01 and write the constant only after the measurement.
- **Hard cap 200 ms** (per the ROADMAP/02-03 requirement): if the depth exceeds it, drain (play) faster for a short window to catch up rather than dropping audio — dropping TTS audio corrupts the sentence.
- **Low-water mark** (e.g. ~60 ms): below it, log an imminent-underrun event; repeated events are the signal that the cap is too small for the network.
- **Underrun policy:** insert a short fade rather than raw silence to avoid a click; count the event in the trace.
- **Barge-in:** the ring is exactly what `interrupt()` clears (Topic 3). It must be clearable without blocking the audio callback — hence the epoch check at push time rather than a mutex held across the callback.

### Interaction with the rest of the budget
The buffer's depth is **pure added latency**, so it competes directly with the 2 s budget. It should be counted as its own stage in the 02-01 waterfall (`tts_last_frame → first_sample_played`), otherwise a buffer tuned to 180 ms will look like a TTS vendor regression.

---

## Topic 7 — partial/final state machine: "spoken English ⊆ committed finals"

**Confidence: MEDIUM-HIGH.** The *protocol* facts behind it are HIGH (Topics 1 and 2); the *policy* is a design recommendation and is the planner's to fix.

### Why it is not a one-line rule
Both vendors have a **two-level** finality model, and they differ:
- **讯飞:** a preview can be *retroactively rewritten* by a later `pgs:"rpl"` frame (`rg` names the `sn` range). "Final" only exists at `data.status == 2`.
- **Deepgram:** `is_final` means "this audio segment is finalized", `speech_final` means "the speaker paused". Both must be combined to get an utterance.

So the local state machine needs an explicit notion of **committed**, distinct from either vendor's flag.

### Concrete states
```
        ┌──────────┐  partial text (preview only)
        │ LISTENING│──────────────────────────────▶ renderer (unstable style)
        └────┬─────┘
             │ segment finality reached
             ▼
        ┌──────────┐
        │  PENDING │  text is stable but not yet released downstream
        └────┬─────┘
             │ COMMIT rule satisfied
             ▼
        ┌──────────┐
        │ COMMITTED│  → translate → validate → TTS (the ONLY path to audio)
        └────┬─────┘
             │ barge-in epoch bump
             ▼
        ┌──────────┐
        │  STALE   │  dropped; never enqueued for playback
        └──────────┘
```

**Commit rule:**
1. **Chinese path:** committed = the text reconstructed from the `data.status == 2` frame only. All `status != 2` text is preview. (`wpgs` `rpl` rewrites are therefore structurally unable to affect committed text.)
2. **English path:** committed = the buffer content flushed at `speech_final: true` (the official pattern). `is_final` alone moves text into PENDING.

**Invariant:** *no audio chunk may be produced from any segment not in the COMMITTED set.* Enforce it as a runtime assertion at the single choke point (the TTS stage's input), not as a convention at call sites.

### Testable form (AUDI-05 / GOV-15 acceptance)
Deterministic test with an injected clock (the Phase 1 `TimeSource` pattern from `sim/source.rs`) plus a scripted `SttSource` double:
1. Feed a script: `partial("我们通过慢查询日志发现了")` → `partial("商品详情页的连表查询瓶颈")` → `rpl` rewriting the second fragment → `final(full correct sentence)`.
2. Assert: the TTS double received **exactly one** string, equal to the final text, and **zero** strings containing the preview or the pre-rewrite wording.
3. Add a **poison partial** (a fragment that would be embarrassing if spoken) and assert the invariant holds — this is the failure that costs the interview.
4. Add a barge-in mid-stream and assert no chunk with a stale epoch reaches the playout ring.

`termHits` (Phase 4's foundation) is computed on the committed text, so the invariant also protects the future glossary metrics from preview churn.

---

## Topic 8 — Confidence levels and macOS 12.7 compatibility annotations

### Per-topic confidence summary

| # | Topic | Confidence | Basis | What would raise it |
|---|-------|-----------|-------|---------------------|
| 1 | 讯飞 iat WSS | **HIGH** | Official spec + working repo script + full error table | Nothing — settled |
| 2 | Deepgram WSS | **HIGH** protocol / **MEDIUM** `multi` language set / **LOW** billing | Official reference + endpointing guide + keepalive docs; LiveKit community thread for the `multi` set | A live billing measurement in 02-01 |
| 3 | Barge-in | **MEDIUM** | Industry implementations + one paper; no standard exists | First-hand measurement with the 02-01 rig |
| 4 | webrtc-audio-processing | **HIGH** API / **MEDIUM** build recipe | GitHub source read directly; build prerequisites from README + `build.rs` | A successful `bundled` build on this machine |
| 5 | cpal graph | **HIGH** API / **MEDIUM** no-hotplug negative claim | docs.rs + crates.io + DeepWiki | A cpal version that adds a device-change event (none in 0.18) |
| 6 | Playout jitter buffer | **LOW** | Design inference from Topic 3 sources | A dedicated search, or measurement-driven tuning |
| 7 | partial/final invariant | **MEDIUM-HIGH** | Derived from Topics 1–2 (HIGH) + AI-SPEC policy | — |
| 8 | Cross-lingual clone | **LOW — unresolved** | Chinese docs say yes, BytePlus English docs say no, `tone_fidelity` explicitly blocks it | **The probe** (highest-priority open item) |

### macOS 12.7 (Monterey, WebKit = Safari 15.6) risk annotations per component

| Component | Risk | Detail and mitigation |
|-----------|------|-----------------------|
| cpal 0.18.2 | **MEDIUM** | MSRV 1.85 satisfied (rustc 1.98). CoreAudio itself is fine at 12.7. The unverified part is the `objc2`-based dependency chain building against the 12.7 SDK on x86_64 — confirm with a real build before planning around it. The machine IS a real Monterey box (12.7.6), so first-party verification is available, not theoretical |
| rubato 5.0.0 | **LOW** | MSRV 1.87 satisfied. Pure Rust (`fft_resampler` default feature), no OS dependency |
| webrtc-audio-processing `bundled` | **HIGH** | Requires meson + ninja + pkg-config + clang; **three of four are absent** on this machine and there is no Homebrew. Also fetches abseil-cpp over the network at build time. `build.rs` honors `MACOSX_DEPLOYMENT_TARGET` (defaults 10.10/x86_64) so export `MACOSX_DEPLOYMENT_TARGET=12.0`. `rust-objcopy` is present. **This is the single most likely build blocker in the phase** |
| silero-vad-rs | **HIGH (viability)** | Pins `ort =2.0.0-rc.9` exactly (an ONNX Runtime RC); 3,004 lifetime downloads; last release 2025-04-04. Whether prebuilt ONNX Runtime binaries exist for macOS 12.7 x86_64 is unverified. **Recommend deferring**; use Deepgram `vad_events`/`endpointing` + 讯飞 `eos` + a local energy VAD for Phase 2 |
| tokio-tungstenite | **LOW** | Pure Rust; needs custom handshake headers for 火山 — supported |
| hound | **LOW** | Pure Rust |
| Frontend (Safari 15.6) | **LOW** | Phase 2 adds only `confidence` (a string union), a `trace` object, and an `abstained` event to the existing protocol. No new CSS/JS platform features are required for a red badge. Keep to the Phase 1 floor: no `Object.hasOwn`, no `structuredClone`, no `@property`/`color-mix`/`oklch` in new styles |
| TLS to all four vendors | **LOW** | macOS 12.7 supports TLS 1.2/1.3; both rustls and native-tls work. No OS-level blocker |
| Clock skew (讯飞 HMAC) | **LOW** | 300 s tolerance; macOS `SystemTime` is NTP-synced. A machine with a badly wrong clock fails auth with 403 — surface it as a distinct error, not a generic auth failure |

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact on this phase |
|---|---|---|---|
| Single WebSocket to a realtime S2S model | **Cascade with per-stage vendor choice** (STT → MT → TTS) | Continuously re-litigated through 2026; the Dec-2025/2026 S2S options still cannot do a *voice clone* | Forces the four-vendor wiring and the whole latency budget problem — this is the phase's raison d'être |
| `webrtc-audio-processing` 0.5.x (`AudioProcessing` struct, `SeparateAec`, per-config setters) | **2.x**: `Processor` + a pure-Rust `Config` crate + `Stats` | 2.0 (2025) → 2.1.0 (2026-05-13) | Every tutorial/snippet online targets 0.4/0.5. Reading docs.rs is actively harmful here (it fails to build 2.1.0 and serves the 0.5.0 API) — use the GitHub source |
| `rubato` `FftFixedIn` / `FftFixedInOut` / `SincFixedIn` | **rubato 5.0**: `Fft` / `Async` / `Slip` + `FixedSync` / `FixedAsync` + `audioadapter` buffers | 5.0.0 (2026) | A complete rewrite. Any pre-5.0 resampling example will not compile. `FixedSync::Both` is new and is exactly right for 3:1 and 2:1 |
| cpal per-operation error enums (`BuildStreamError`, `StreamError`, `BackendSpecificError`) | **cpal 0.18**: unified `Error` + `ErrorKind` (`DeviceNotAvailable`, `StreamInvalidated`, `DeviceChanged`, `DeviceBusy`) | 0.18 | Makes device hot-swap handling tractable: one `ErrorKind` match instead of four error enums. `ErrorKind` is the design surface for AUDI-06's second half |
| Whole-utterance translation (buffer the sentence, then translate) | **Incremental/streaming partial translation** | Ongoing | The 2 s budget is unreachable without it; this is why Topic 7's commit gate exists and why the "no whole-sentence serial" constraint is in CLAUDE.md |
| ElevenLabs as the default clone TTS | **火山 ICL 2.0** (this project's blind test: MOS 5.0/5.0) | 2026-09-29, D-11 | Settled. Not researched further |
| Python/Node vendor prototyping | **Rust clients in the Tauri core** | This phase | The `tools/vendor-experiments/*.mjs` scripts are the *reference implementation* for the wire protocols, not the production path. Port their logic, do not shell out to them |

**Deprecated / do not use:**
- `docs.rs/webrtc-audio-processing` — serves the 0.5.0 API for a crate whose current version is 2.1.0. Use the GitHub source.
- `KeepAlive` as a query parameter on the Deepgram URL — it is a client *message*.
- Deepgram `language=multi` for anything involving Chinese — Chinese is not in the `multi` set.
- 讯飞's `sc` field as a confidence score — reserved and undocumented.
- 火山's v1 HTTPS auth headers (`X-Api-App-Id`, `X-Api-Access-Key`) — the v2/v3 unidirectional-stream endpoint uses `X-Api-Key` + `X-Api-Resource-Id`. (`volc-tts-stream.mjs`'s header comment still lists the stale pair; the code is correct.)

---

## Assumptions Log

Claims in this research that are **not** verified and need user/experiment confirmation before they become locked plan decisions.

| # | Claim | Section | Risk if wrong |
|---|-------|---------|---------------|
| A1 | **Cross-lingual synthesis with a 火山 ICL 2.0 clone produces the *user's voice* speaking English.** | Summary finding 1; Topic 4 pitfalls; Topic 8 | **Product-critical.** The blind MOS 5.0/5.0 test used Chinese sentences only; `tone_fidelity` (还原模式) explicitly does not support cross-lingual synthesis, and BytePlus English docs contradict the Chinese docs. If cross-lingual cloning fails, D-11's primary vendor fails and a second TTS vendor (MiniMax / Cartesia) returns as mandatory — which changes the plan, the cost model and the schema |
| A2 | The 1–3 minute enrollment recording in AUDI-05 is compatible with `voice_clone`. | Phase requirements AUDI-05; Pitfall: clone enrollment | The Volc `voice_clone` API caps audio at **10 MB** and its docs describe 10–30 s reference audio; there is a WER gate (error 45001109) that rejects poor-quality/reference-mismatched samples. A 3-minute recording may exceed the cap or fail the gate. Requires an actual enrollment attempt |
| A3 | Deepgram does not bill per interim-result message (the ROADMAP's "interim billing amplification" note). | Topic 2, billing caveat | Cost model (GOV-18/D-13) and the ≤$3.5/interview-hour target. Mitigation is cheap: the 02-01 rig counts messages per session and 02-xx compares to a real invoice |
| A4 | `webrtc-audio-processing` `bundled` builds successfully on macOS 12.7 with meson+ninja+pkg-config+clang once those are installed. | Topic 4 build section; Environment Availability | **Highest-probability build blocker.** No first-hand build has been performed. The `build.rs` path is read but not executed; abseil is fetched at build time over the network, so an offline/firewalled build fails |
| A5 | meson's dependency resolution for `absl_base` succeeds without a system abseil / pkg-config. | Topic 4, item 4 | `build.rs` tolerates a missing system abseil at the *Rust* level, but meson still has to resolve it. Unverified |
| A6 | The Deepgram `utterance_end_ms` → "utterance complete" mapping is safe to use as the segment boundary detector for the English path. | Topic 2 / Topic 7 | If it is unreliable, the English segmenter needs a different trigger; also affects the cue/subtitle grouping (criterion 3's "<= 2 s after segment end") |
| A7 | A playout jitter buffer of ≤200 ms depth is sufficient for the observed TTS frame inter-arrival distribution. | Topic 6 | If real jitter exceeds it, either the cap is breached (latency over budget) or audio is dropped (corrupted sentence). Must be measured in 02-01 before the constant is written |
| A8 | 讯飞 `eos` default (2000 ms) is an appropriate segment boundary for Mandarin interview speech. | Topic 1 / Topic 7 | Too long → segment latency blows the budget; too short → fragments get committed and spoken separately. Requires tuning against real interview speech |
| A9 | Nothing in the pipeline needs a warm session at cold start — i.e. the ≤2 s criterion is achievable *including* WebSocket handshakes. | Summary finding 2; AUDI-04 | The criterion says "含冷启动". 讯飞 HMAC + TLS + Deepgram handshake + 火山 session create are all serial before any audio. 02-01 must measure cold vs warm separately; if cold fails, the plan needs a pre-warm step (connect on app launch, keep the sessions alive) |
| A10 | The `silero-vad-rs` crate is not needed for Phase 2 (Deepgram `vad_events` + 讯飞 `eos` + local energy VAD suffice). | Topic 8 / Environment Availability | If segment detection proves unreliable without a proper VAD, this crate's viability (pinned `ort` RC, macOS 12.7 x86_64 prebuilts unverified) becomes a blocker rather than a deferral |
| A11 | `MACOSX_DEPLOYMENT_TARGET=12.0` exported at build time is sufficient for the `bundled` C++ build (the `build.rs` default is 10.10/x86_64). | Topic 4, item 6 | If the C++ side ignores it or the SDK's min-version flag conflicts, the produced binary may not load on the target Mac. Low risk but cheap to verify with `otool -l` |

**How to use this table:** A1 and A2 are *probe-first* items — they should be the **first executable tasks** of their respective plans (02-04), each producing a recorded artifact (audio file + measurement JSON) that the user listens to / reads, before any UI or plumbing is built on top. A3/A7/A9 are *measurement-first* items that ride on the 02-01 rig. A4/A5/A6/A10/A11 are *verify-during-execution* items.

---

## Open Questions

1. **(RESOLVED → 02-04 T4.0a/T4.0b：跨语种探针 + blocking-human 盲听判定)** Does 火山 ICL 2.0 cloning survive cross-lingual synthesis (Chinese enrollment audio → English output)?**
   - What we know: `explicit_language` selects the synthesis text's language and requires the input text to actually contain that language; `tone_fidelity: false` is mandatory for cross-lingual; the Chinese market docs describe cross-lingual support; the BytePlus (international) English docs read as though cross-lingual cloning is not supported; the blind MOS test was Chinese-only.
   - What is unclear: whether the voice *identity* is preserved when the text language differs from the enrollment language, and whether `explicit_language:'en'` is required, sufficient, or silently ignored on the international endpoint.
   - Recommendation: run a single-purpose probe as the first task of 02-04 — enroll from a 30 s Chinese sample, synthesize an English paragraph with `explicit_language:'en'` + `tone_fidelity:false`, and have the user judge identity. Persist the artifact as a failure-case-library entry either way. **Do not design the enrollment UI before this returns.**

2. **(RESOLVED → 02-01 T1：rig 五边界流式 TTFB + 预算硬断言 + 重叠证明)** What are the *streaming* first-byte latencies per stage?
   - What we know: 讯飞 0.7 s (whole clip → final), Deepgram 222 ms RTT, DeepSeek 223 ms TTFT, 火山 1.3 s (whole sentence → `SESSION_FINISHED`).
   - What is unclear: the time from "first mic frame sent" to "first partial text received" per stage, and how much of each vendor's number is network vs processing on this specific machine.
   - Recommendation: 02-01's rig timestamps at five boundaries (mic callback → STT first partial → MT first token → TTS first PCM frame → first PCM drained by cpal) and emits a waterfall. The AI-SPEC budgets (STT ≤500 ms, MT ≤500 ms, TTS ≤800 ms) are the per-stage acceptance gates; measure them, do not derive them.

3. **(RESOLVED → 02-01 T1：冷/热双数实测，失败则预暖启动缓解)** Is a cold start (no warm sessions) able to meet ≤2 s?
   - What we know: AUDI-04 explicitly says "含冷启动". Four handshakes (讯飞 HMAC+TLS, Deepgram TLS, DeepSeek TLS, 火山 session create) precede any audio.
   - What is unclear: the exact handshake cost on this network, and whether the criterion means "first press of the button" or "first session within a warm app".
   - Recommendation: measure both cold and warm in 02-01 and record both numbers. If cold fails, add a pre-warm step (open sessions at app launch / on `start_session`) and document it as the mitigation.

4. **(RESOLVED → 02-02/02-03：confidenceSource "vendor"|"proxy" 判别字段 + 本地代理，02-03 T4 校准)** Where does the local STT-confidence proxy come from, and how is it labelled?
   - What we know: 讯飞's `sc` is reserved/undocumented; Deepgram's `alternatives[].confidence` is real. So the two paths have asymmetric inputs to D-01's triple.
   - What is unclear: which local signals are actually predictive (wpgs revision count? audio SNR? segment duration? term-hit rate?).
   - Recommendation: implement a `confidenceSource: "vendor" | "proxy"` field on the trace so the asymmetry is visible in the JSONL, then calibrate the proxy's weight in 02-01/02-02 using the failure-case library.

5. **(RESOLVED → 02-03 T5：计量归因 + 重译膨胀计数，预算提示 D-15)** Does the ≤$3.5/interview-hour target hold once the pipeline is streaming?
   - What we know: per-stage unit prices are documented (Deepgram ~$0.0048/min, DeepSeek tokens, 火山 per character, 讯飞 per session/minute). The AI-SPEC sets the target; D-13 fixes the metering dimensions.
   - What is unclear: the actual token/character volumes of a 60-minute bilingual interview, and whether interim results or ws partial re-translation inflate MT token counts (translation is re-run on each committed fragment, so the *same* content may be charged more than once).
   - Recommendation: the 02-01 rig should emit `analyze`-style usage counters alongside latency so one real session produces both numbers at once.

6. **(RESOLVED → 02-03 T3.8：tools/vendor-experiments/failure-cases/ + run.mjs + CI 第四车道)** Which failure-case library location and CI form? (D-20 explicitly delegates this to the planner.)
   - What we know: `tools/vendor-experiments/failure-cases/0001-0002` exist; the AI-SPEC names a 20-case starting set; D-20 suggests `tools/vendor-experiments/failure-cases/` or `.planning/ref/`.
   - Recommendation: keep it under `tools/vendor-experiments/failure-cases/` (where the first two already live), with a Node runner consistent with the existing `tools/vendor-experiments/*.mjs` style, and add it as a fourth CI lane alongside `pnpm -r test`, `playwright test`, and `cargo test`.

---

## Environment Availability

Probed on this machine (macOS 12.7.6, x86_64) during research.

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| macOS Monterey | Whole project constraint | ✓ | 12.7.6 (x86_64) | — (this IS the target floor; first-party testing possible) |
| rustc / cargo | Tauri core | ✓ | 1.98.0 | — (MSRV 1.85 cpal / 1.87 rubato satisfied) |
| `rust-objcopy` | `webrtc-audio-processing` bundled symbol prefixing | ✓ | present in the x86_64-apple-darwin sysroot | — |
| clang | `webrtc-audio-processing` bundled C++ build | ✓ | 14.0.0 (Apple) | — |
| **meson** | `webrtc-audio-processing` bundled | ✗ | — | **None.** `build.rs` bails with "Failed to execute meson" |
| **ninja** | `webrtc-audio-processing` bundled | ✗ | — | **None.** `build.rs` bails with "Failed to execute ninja" |
| **pkg-config** | `webrtc-audio-processing` (system abseil probe; possibly optional) | ✗ | — | Meson-fetched abseil fallback exists in `build.rs` but is unverified at the meson layer (`[ASSUMED]` A5) |
| Homebrew | Installing the three missing tools | ✗ | — | Install via Homebrew (itself an install step), MacPorts, or prebuilt meson/ninja binaries |
| Node.js | Tooling, Playwright, failure-case runner | ✓ | v24.15.0 | — |
| pnpm | Workspace package manager | ✓ | 11.24.0 | — |
| Deepgram reachability | English STT | ✓ (222 ms RTT measured) | — | — |
| 讯飞 reachability | Chinese STT | ✓ (0.7 s measured) | — | — |
| DeepSeek reachability | Translation | ✓ (223 ms TTFT measured) | — | — |
| 火山 reachability (api.minimax.io equivalent: `openspeech.bytedance.com`) | Clone TTS | ✓ (1.3 s measured) | — | — |
| BlackHole 2ch | Phase 3 only | n/a | — | Out of scope for Phase 2 |
| `silero-vad-rs` viability | Optional local VAD | ✗ (unverified) | pins `ort =2.0.0-rc.9`; 3,004 lifetime downloads; last release 2025-04-04 | Use Deepgram `vad_events`/`endpointing` + 讯飞 `eos` + a local energy VAD (Assumption A10) |

**Missing dependencies with no fallback:**
- **meson**, **ninja** — block the `webrtc-audio-processing` `bundled` build entirely. A Wave 0 task must install these (e.g. `brew install meson ninja pkg-config`, which first requires Homebrew) or the AEC task cannot compile. This is the single most likely execution blocker in the phase.
- **pkg-config** — blocks the *system* abseil path; the meson fallback may cover it, but that fallback is unverified (`[ASSUMED]` A5). Install it alongside meson/ninja.

**Missing dependencies with fallback:**
- `silero-vad-rs` — optional; the vendored vendor VAD signals cover Phase 2. Revisit only if segment detection proves unreliable.
- No Homebrew — a fallback chain exists (MacPorts, prebuilt binaries, or a manual meson/ninja install), but the plan should treat "install a package manager or these three tools" as an explicit Wave 0 task rather than assuming it.

**Network dependency at build time:** the `bundled` build fetches `abseil-cpp-20240722.0` through meson's `subprojects/abseil-cpp.wrap`. The first `cargo build` therefore requires network access and a working subproject download. Cache `OUT_DIR`/`~/.cargo` in any CI cache so this cost is paid once.

---

## Validation Architecture

Nyquist validation is enabled (`.planning/config.json` has no explicit `workflow.nyquist_validation: false`).

### Test Framework

| Property | Value |
|---|---|
| Framework | **Vitest** (packages + frontend units) · **Playwright** (E2E + visual regression) · **cargo test** (Rust core) · **Node ESM scripts** (vendor experiments / failure-case runner) |
| Config file | `packages/*/vitest.config.*`, `apps/desktop/playwright.config.*`, `apps/desktop/src-tauri/Cargo.toml`. Failure-case runner config does not exist yet — **Wave 0** |
| Quick run command | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` (Rust, the bulk of this phase's logic) |
| Full suite command | `pnpm -r test && pnpm exec playwright test && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml && node tools/vendor-experiments/failure-cases/run.mjs` |

**Correction to 02-AI-SPEC.md:** the CI line it cites (`... && cargo test --workspace`) is inaccurate — **no Cargo workspace exists** in this repo. Only `apps/desktop/src-tauri/Cargo.toml`. Use `--manifest-path` (or create a workspace deliberately as part of Wave 0, which would also let `pnpm -r`-style invocation work).

**Determinism rule (Phase 1 convention, carried forward):** every pipeline unit test injects a clock. The existing pattern is `sim/source.rs`'s `pub trait TimeSource: Send + 'static { fn elapsed_ms(&self) -> u64; }` with `TICK_MS: u64 = 100`. The real pipeline's stage timers must be built on the same trait so the whole cascade is testable without network access.

**Vendor isolation rule:** no unit test may touch a vendor endpoint. The four vendors sit behind traits (`SttSource`, `Translator`, `TtsSink`), and tests use scripted doubles. Live-vendor checks live only in the experiment scripts and the opt-in latency rig.

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|---|---|---|---|---|
| AUDI-03 | Cascaded mic→STT→MT→TTS→playback path produces audio for a committed segment | integration (mock vendors) | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cascade -- --nocapture` | ❌ Wave 0 |
| AUDI-03 | Stages overlap (no whole-sentence serialization) — assert stage-start timestamps interleave | integration | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cascade_overlap` | ❌ Wave 0 |
| AUDI-04 | Stage-boundary timestamps are emitted for every segment | unit | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml latency_waterfall` | ❌ Wave 0 |
| AUDI-04 | Cold-start e2e ≤ 2 s (opt-in, live vendors, `#[ignore]`) | live/manual-gated | `cargo test ... -- --ignored latency_e2e_cold` | ❌ Wave 0 |
| AUDI-05 | Voice-clone enrollment: audio size/format validated before upload; 10 MB and WER-gate errors surfaced distinctly | unit | `cargo test ... clone_enrollment_guards` | ❌ Wave 0 |
| AUDI-05 | **Cross-lingual clone probe** (enroll zh → synthesize en → user listens) | manual-only, gated artifact | `node tools/vendor-experiments/cross-lingual-clone-probe.mjs` | ❌ Wave 0 — **highest priority** |
| AUDI-06 | Barge-in: playout ring drained, epoch bumped, no stale-era chunk reaches output | unit (injected clock) | `cargo test ... barge_in_epoch` | ❌ Wave 0 |
| AUDI-06 | Abort-to-silence latency measured and ≤ ~100 ms | unit (injected clock) | `cargo test ... barge_in_abort_latency` | ❌ Wave 0 |
| AUDI-06 | Device disappearance mid-session → rebuild, session survives | integration (fault-injected cpal double) | `cargo test ... device_hot_swap` | ❌ Wave 0 |
| GOV-01 | Three-factor confidence produces a `high`/`medium`/`low` label; the `confidenceSource` discriminator is present | unit | `cargo test ... confidence_triple` | ❌ Wave 0 |
| GOV-02 | Low-confidence subtitle carries the red badge; **never** suppresses output | unit + Playwright | `cargo test ... low_confidence_badge` · `pnpm exec playwright test low-confidence.spec.ts` | ❌ Wave 0 |
| GOV-03 | Empty/no-text audio → `abstained` event; non-empty low-confidence → **not** abstained | unit | `cargo test ... abstain_only_on_empty` | ❌ Wave 0 |
| GOV-06 | Every committed segment appends exactly one JSONL line with the required fields | unit | `cargo test ... jsonl_append` | ❌ Wave 0 |
| GOV-07 | `segmentStartMs` is relative to session start and monotonic | unit (injected clock) | `cargo test ... segment_offset` | ❌ Wave 0 |
| GOV-08 | Wire shapes: Rust `ServerEvent` ↔ `packages/protocol/src/index.ts` agree, incl. `confidence`/`trace`/`abstained` | unit (mirrors Phase 1's `wire_shapes_match_protocol_package`) | `cargo test ... wire_shapes_match_protocol_package` · `pnpm -r test` | ⚠️ exists, must be **extended** |
| GOV-12 | Segment-level retry: exactly 2 retries, 100 → 200 ms backoff, 500 ms budget, then next segment | unit (injected clock) | `cargo test ... segment_retry_policy` | ❌ Wave 0 |
| GOV-13 | Breaker: 2 consecutive failures → open 120 s → half-open probe → close | unit (injected clock) | `cargo test ... circuit_breaker` | ❌ Wave 0 |
| GOV-14 | Degraded display: original text + 「翻译服务暂时不可用」 + 「正在重试」, Chinese only | Playwright + unit | `pnpm exec playwright test degraded.spec.ts` | ❌ Wave 0 |
| GOV-15 | **`spoken English ⊆ committed finals`** — poison-partial test (Topic 7 design) | unit (injected clock + scripted `SttSource`) | `cargo test ... committed_finals_invariant` | ❌ Wave 0 — **the phase's signature test** |
| GOV-19 | Failure-case library schema + at least 20 cases load and validate | Node | `node tools/vendor-experiments/failure-cases/run.mjs --validate` | ⚠️ 2 cases exist (`0001-0002`); runner + 18 more missing |
| GOV-20 | Regression: all correct cases pass and each promoted failure case passes | Node | `node tools/vendor-experiments/failure-cases/run.mjs` | ❌ Wave 0 |
| — | Output pre-validation drops/repairs numeric drift (D-04) | unit | `cargo test ... numeric_prevalidation` | ❌ Wave 0 |
| — | Safari 15.6 floor: no Safari 16.4+ CSS/JS in new frontend code | build | `pnpm -r build` (esbuild `safari15` target) | ⚠️ exists, must cover new styles |

### Sampling Rate

- **Per task commit:** `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` (fast; the majority of this phase is Rust) plus `pnpm -r test` when protocol/TS files change.
- **Per wave merge:** the full suite command above, including Playwright.
- **Phase gate:** full suite green, plus the AUDI-04 live latency run recorded as an artifact, plus the AUDI-05 cross-lingual probe artifact, before `/gsd:verify-work`.

### Wave 0 Gaps

- [ ] `apps/desktop/src-tauri/src/pipeline/` test module scaffold — `SttSource`, `Translator`, `TtsSink` scripted doubles + the injected `TimeSource` harness (covers AUDI-03, AUDI-04, GOV-12, GOV-13, GOV-15)
- [ ] `apps/desktop/src-tauri/tests/` — integration tests for the cascade and the audio graph
- [ ] `packages/protocol` — extend the TS union with `confidence`/`trace`/`abstained` and extend the existing Rust↔TS shape test (GOV-08; the Phase 1 test exists and must be updated in the same commit)
- [ ] `tools/vendor-experiments/failure-cases/run.mjs` — the D-20 runner (validation + regression modes) and 18 additional seed cases to reach the 20-case starting set
- [ ] `tools/vendor-experiments/cross-lingual-clone-probe.mjs` — the A1 probe (highest-priority new script)
- [ ] Toolchain: `meson`, `ninja`, `pkg-config` install task (or an explicit decision to descope AEC3 from Phase 2)
- [ ] Decide whether to create a Cargo workspace at `apps/desktop/src-tauri` root or keep `--manifest-path` invocation; if a workspace is created, update 02-AI-SPEC's CI line

---

## Security Domain

`security_enforcement` is enabled with `security_asvs_level: 1` and `security_block_on: high`.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---|---|---|
| V2 Authentication | **yes** | Vendor auth only. 讯飞 HMAC-SHA256 request signing (secret never leaves the process), Deepgram `Authorization: Token …` header, 火山 `X-Api-Key`. No user accounts in Phase 2 (D-17 defers accounts to Phase 8) |
| V3 Session Management | **partial** | The LAN pairing/session from Phase 1 continues; Phase 2 adds a second (provider) session concept with an epoch. No new user-facing session tokens |
| V4 Access Control | **no** | Single local user; no multi-user surface in Phase 2 |
| V5 Input Validation | **yes** | Per-vendor message validation (all four protocols emit untrusted JSON/binary that must be shape-checked before use); **output pre-validation (D-04)** is a correctness *and* safety control; device names / config values from CoreAudio are untrusted input |
| V6 Cryptography | **yes (use-only, never hand-roll)** | TLS via rustls/native-tls for all four WSS/HTTPS links; HMAC-SHA256 for 讯飞 via a vetted crate (`hmac` + `sha2`), **never a hand-written HMAC**. No custom crypto anywhere |

### Credential handling (the phase's highest-value security rule)

The repo already documents the policy in `tools/vendor-experiments/README.md` / `.env.example`, and Phase 2 promotes it from a scripts convention to a product requirement:

- Four provider keys (讯飞 APPID + APIKey + APISecret, Deepgram, DeepSeek, 火山) live **only** in environment variables / a gitignored local `.env`.
- **Never in source, never in a log line, never in a JSONL trace record, never in a committed file, and never as a CLI argument.**
- The JSONL trace (D-05) records `provider` and `modelVersion` — **not** keys, not request URLs containing `authorization=`. The 讯飞 URL carries the signature in its query string, which makes "log the URL" an easy accidental leak; the client must log a redacted form.
- Errors from vendors must be surfaced with the key scrubbed before display (this is also an ASVS L1 "error messages don't leak sensitive data" item).
- Startup validation: fail loudly and specifically if any required key is absent, rather than at the first API call (per `.env.example` field conventions).
- Exposure response: if a key is ever pasted into a committed file, treat it as leaked and **rotate it in the vendor console first**, then clean the file.

**Threat-registry entry (per the `T-01-xx` convention in 02-CONTEXT.md):** register pipeline-introduced threats at planning time — key handling, prompt injection into the translation/copilot path, and output-validation bypass.

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---|---|---|
| API-key leakage via log/JSONL/trace records | Information Disclosure | Redaction at the single logging choke point; keys read from env only; a test that asserts no key appears in a produced JSONL file |
| API-key leakage via the 讯飞 signed URL (`authorization=` query param) | Information Disclosure | Never log full request URLs for the STT client; log host + path only |
| Prompt injection via STT text (user or interviewer speech, or audio content) reaching the translation/copilot prompt | Tampering / Elevation | Treat all transcript text as data, never instructions; keep transcripts in a data slot separate from the system prompt; D-04's deterministic validation runs on the *output* regardless of the prompt's content |
| Prompt injection via retrieved web content (a later phase's concern, seeded here) | Tampering | Phase 2 only needs the data-boundary discipline; note it for Phase 5 |
| Untrusted vendor frames (malformed/oversized JSON, binary frames) | Tampering / DoS | Reuse Phase 1's `MAX_FRAME_BYTES = 64 * 1024` pattern for every new socket; validate every parsed message against an explicit shape before use; reject unknown fields explicitly |
| Numeric/unit drift in translated output (800毫秒 → 800ms or 800s) | Tampering (integrity) | **D-04 deterministic pre-validation** on every translation before it reaches TTS; failure → fall back to source text rather than speaking a wrong number |
| Unbounded inbound data from a vendor socket | DoS | Frame size cap + a per-session byte/time budget; the breaker (D-10) also bounds pathological retries |
| Device-name / CoreAudio-supplied strings | Tampering | Treat as display-only strings; never interpolate into a shell command or a path |
| Mixed `http://` LAN access from the phone (Phase 1 surface) | Information Disclosure | Unchanged from Phase 1; Phase 2 does not widen the LAN surface — the new event types ride the existing channel |

---

## Sources

### Primary (HIGH confidence)

**Repo artifacts (authority for the wire protocols — the scripts demonstrably work):**
- `tools/vendor-experiments/stt-ab.mjs` — 讯飞 HMAC signature, 1280-byte/40 ms framing, `wpgs` handling, Deepgram comparison path
- `tools/vendor-experiments/translation-probe.mjs` — DeepSeek SSE consumption (`\n\n` boundary parsing) and the `failure-case-0001` fix
- `tools/vendor-experiments/volc-tts-stream.mjs` — 火山 unidirectional-stream binary framing, event IDs (152 / 352), header set, parse logic
- `tools/vendor-experiments/volc-voice-clone.mjs` — `voice_clone` REST contract, 10 MB cap, WER-gate error `45001109`
- `tools/vendor-experiments/blind-clone-results.json` — D-11's evidence (ICL 2.0 MOS 5.0/5.0; **Chinese test sentences only**)
- `apps/desktop/src-tauri/src/lan/server.rs` — the `ServerEvent` serde mirror to extend (`deny_unknown_fields`, `MAX_FRAME_BYTES`)
- `apps/desktop/src-tauri/src/state.rs` — `SessionState`, `append_event`/`publish`, `session_epoch`, `broadcast::channel(64)`
- `apps/desktop/src-tauri/src/sim/source.rs` — `TimeSource` / `TICK_MS` injected-clock pattern
- `packages/protocol/src/index.ts` — the TS closed union that must stay in lockstep
- `.planning/phases/02-real-cloud-pipeline-audio-core/02-AI-SPEC.md` — failure modes, budgets, guardrails, 20-case eval set, cost target
- `.planning/ref/ai-governance-requirements.md` — GOV-01..GOV-23 and the phase mapping

**Official vendor documentation:**
- iFlytek 讯飞 `xfyun.cn/doc/asr/voicedictation/API.html` — signature algorithm, frame structure, `wpgs`/`apd`/`rpl`/`rg`, `eos` default 2000 ms, 13000-byte audio limit (error 10163), full error-code table, `sc` = 保留字段
- Deepgram `developers.deepgram.com/docs/streaming` — endpoint, `Token` auth scheme, complete parameter table, server/client message shapes, `is_final` vs `speech_final`
- Deepgram endpointing guide + KeepAlive/NET-0001 docs — 10 s window, 3–5 s cadence, text-frame requirement, close code 1011
- Deepgram language docs — Nova-3 Mandarin support (`zh-CN`/`zh-Hans`/`zh-Hant`/`zh-HK`) and the 10-language `multi` set that excludes Chinese
- 火山引擎 Seed-TTS / ICL 2.0 synthesis docs — `explicit_language`, `tone_fidelity` (还原模式 does not support cross-lingual), `seed-tts-2.0` / `seed-icl-2.0` resource IDs, `X-Control-Require-Usage-Tokens-Return`, `enable_subtitle`
- `github.com/tonarino/webrtc-audio-processing` — `src/lib.rs`, `src/stats.rs`, `examples/simple.rs`, `webrtc-audio-processing-config/src/lib.rs`, `webrtc-audio-processing-sys/build.rs`, `Cargo.toml`, `README.md` (excerpted at `cdn.jsdelivr.net/gh/tonarino/webrtc-audio-processing@main/README.md`)
- docs.rs / crates.io for `cpal 0.18.2`, `rubato 5.0.0`, `tokio-tungstenite 0.30.0`, `reqwest 0.13.5`, `hound 3.5.1`, `thiserror 2.0.21`, `anyhow 1.0.104` (versions, MSRV, publish dates, dependency lists)

### Secondary (MEDIUM confidence)
- `webrtc-audio-processing-config` design notes — config decoupled from FFI for testability
- Koi issue #829 (barge-in acceptance criteria: ~100 ms abort, no artifacts) and the Zenodo paper on deterministic state machines + barge-in + cancellation (epoch pattern, `interrupt()` = cancel + respawn)
- Amadeus `barge_in_aec_interrupt_notes.md` — `AbortController` propagation, "do not wait for the abort ack", AEC render reference, `AEC_REALTIME_DELAY_MS`, 650 ms echo tail
- Future AGI — minimum-speech/cooldown parameters, 300/350 ms guards, cancel-restart loop
- silero-vad documentation — 30 ms chunks, <1 ms per chunk on one CPU thread
- LiveKit community thread — Deepgram `language=multi` language set
- crates.io metadata for `silero-vad-rs` — pinned `ort =2.0.0-rc.9`, 3,004 lifetime downloads, last release 2025-04-04

### Tertiary (LOW confidence — flagged for validation)
- Playout jitter-buffer sizing guidance (Topic 6) — design inference, no authoritative source located
- Deepgram interim-result billing behaviour (Assumption A3)
- Volc Engine docs reached only through a mirrored copy (`cdn.jsdelivr.net/gh/liangdabiao/STEMViz@main/tts.md`) because `volcengine.com/docs/...` returns JS-rendered shells; the in-repo script remains the authority for binary framing

**Tooling note:** Context7 was unavailable in this environment (no `mcp__context7__*` tools, no `ctx7` CLI). `docs.rs` failed to build `webrtc-audio-processing` 2.1.0 and served the 0.5.0 API instead, so the GitHub source was read directly. Where possible, findings were confirmed against the repo's own working experiment code, which outranks any vendor doc page for wire-protocol questions.

---

## Metadata

**Confidence breakdown:**
- **Standard stack: HIGH** — every crate version verified against crates.io/cargo metadata; the API shapes for the four vendors verified against the repo's own working scripts plus official docs.
- **Architecture: MEDIUM-HIGH** — the tier map and project structure follow the existing `SessionState`/`ServerEvent` pattern and the AI-SPEC's module list; the commit-gate design (Topic 7) is a recommendation derived from HIGH-confidence protocol facts but is not itself vendor-documented.
- **Pitfalls: HIGH** for the protocol-level items (error codes, framing, `wpgs`, NET-0001, `multi`, `tone_fidelity`) — each is grounded in official docs or the working scripts; **LOW** for the playout-buffer sizing guidance.
- **Environment: HIGH** — probed directly on the target machine; the meson/ninja/pkg-config absence is a first-hand finding, not an inference.

**Research date:** 2026-09-29
**Valid until:** ~2026-10-29 (30 days for the protocol/API surface). Re-verify sooner if: any vendor ships a new streaming API version, `webrtc-audio-processing` publishes a breaking minor (`2.2` per its non-semver policy), `rubato` publishes 6.x, or the cross-lingual probe (A1) returns a negative result — that last one invalidates the TTS section's premise.

**Explicitly out of scope and NOT researched (per CONTEXT.md):** virtual audio device / BlackHole routing (Phase 3), true stealth/aggregate-device work (Phase 4), copilot strategy engine and glossary UI (Phase 5/4), recording and review (Phase 6), accounts/telemetry/remote gateway (Phase 8).
