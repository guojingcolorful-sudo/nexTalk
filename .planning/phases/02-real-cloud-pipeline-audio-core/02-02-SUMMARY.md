---
phase: 02-real-cloud-pipeline-audio-core
plan: 02
subsystem: pipeline (vendor clients + wire protocol)
tags: [xfyun, deepgram, deepseek, volc-tts, websocket, sse, hmac-sha256, wpgs, keepalive, serde, protocol-mirror]

# Dependency graph
requires:
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 01
    provides: Stage boundaries + LatencyMark rig (budget.rs), TimeSource contract, the `latency` diagnostics channel the stage marks feed
provides:
  - "Stage contracts: SttSource / Translator / TtsSink (provider() + model_version() on each; partialvs-committed semantics)"
  - "StageError + RetryClass classification (讯飞 code table, HTTP status, transport/protocol) — D-09/D-10 consume it in 02-03"
  - "RoutingConfig (GOV-18 fixed routing) + env-only credentials behind Secret<String>"
  - "Four real vendor clients: 讯飞 iat (STT zh), Deepgram Nova-3 (STT en, interviewer line), DeepSeek SSE (translation), 火山 ICL 2.0 (cloned-voice TTS)"
  - "Four deterministic offline mock servers with failure injection (mock_vendors.rs)"
  - "D-07 wire extension: confidence / trace / abstained on both protocol ends, backward compatible"
affects: [02-03 cascade assembly, 02-04 live probes, 02-05 audio core, Phase 4 term base (TermHit), Phase 5-6 cost/telemetry (usage fields)]

# Tech tracking
tech-stack:
  added:
    - hmac 0.13.0 + sha2 0.11.0 (讯飞 RFC1123 HMAC-SHA256 signing)
    - tokio-tungstenite 0.30.0 (runtime dep, native-tls; supersedes the 0.28 dev-dep)
    - base64 0.22.1, thiserror 2.0.20, reqwest 0.13.4 (json/stream/native-tls), uuid 1.26.0 (v4)
  patterns:
    - "Three-stage trait contracts where each method boundary IS a 02-01 Stage mark"
    - "Preview-vs-committed semantics per vendor (讯飞 status 0/1/2 + wpgs, Deepgram is_final/speech_final)"
    - "Deterministic mock-first testing: every vendor client is driven against 127.0.0.1:0 mocks, zero keys, zero network"
    - "Serde mirror of the TS wire union with deny_unknown_fields + camelCase rename + Option fields absent (never null)"

key-files:
  created:
    - apps/desktop/src-tauri/src/pipeline/stages/traits.rs
    - apps/desktop/src-tauri/src/pipeline/stages/error.rs
    - apps/desktop/src-tauri/src/pipeline/stages/config.rs
    - apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs
    - apps/desktop/src-tauri/src/pipeline/stages/deepgram.rs
    - apps/desktop/src-tauri/src/pipeline/stages/deepseek.rs
    - apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs
    - apps/desktop/src-tauri/tests/mock_vendors.rs
  modified:
    - apps/desktop/src-tauri/Cargo.toml (deps above) + Cargo.lock
    - apps/desktop/src-tauri/src/pipeline/mod.rs
    - apps/desktop/src-tauri/src/pipeline/stages/mod.rs (Vendor* dispatch enums)
    - apps/desktop/src-tauri/src/lan/server.rs (serde mirror of the D-07 extension)
    - apps/desktop/src-tauri/src/sim/source.rs (construction sites gain confidence/trace)
    - apps/desktop/src-tauri/tests/session_integration.rs (provenance + abstain over the real LAN transport)
    - packages/protocol/src/index.ts (TS union + isServerEvent guards)
    - packages/protocol/src/index.test.ts

key-decisions:
  - "【规则 3 - 阻塞】新增常驻依赖 base64/thiserror/reqwest/uuid 与 native-tls 特性：三件审核门批准的包之外，这些已在 Cargo.lock 中作为传递依赖存在，被提升为直接依赖（讯飞 base64 分帧、StageError、SSE 流式、握手 UUID）；sha2 选 0.11 代数，与 lockfile 中传递依赖 sha2 0.10.9 并存（正常，RustCrypto 语义）"
  - "讯飞：TranscriptBuilder 用有序 Vec<(sn, text)> 重建整句（wpgs rpl 覆盖 rg 区间），未知区间追加而非丢弃；只有 data.status == 2 为 committed；sc 字段永不读取，confidence 恒 None + ConfidenceSource::ProxyUnavailable"
  - "讯飞：SttUpstream{Audio,End} 取代「丢弃发送端即结束片段」——片段结束是一条消息，不是通道关闭"
  - "Deepgram：language=en 硬锁（multi 不含中文，会静默产出垃圾）；Token 鉴权；is_final 进缓冲、speech_final 才提交、UtteranceEnd 兜底；3s KeepAlive 具名常量；channel 字段不类型化解析（VAD 帧是数组、Results 是对象——类型化会静默丢掉抢话检测需要的帧）"
  - "面试官线不打 Stage::SttFirstPartial 埋点（02-01 瀑布只测用户主链路，断言而非注释）"
  - "DeepSeek：SSE 先按 \\n\\n 切帧再解析（失败案例 0001 不可复现）；take_valid_utf8 保住跨 chunk 的多字节字符；温度 0、R1 系模型上线前拒绝、上下文只带前一句译文（AI-SPEC §4）；畸形输出 = 可重试失败，绝不宽松回退为空译文"
  - "火山：资源头随 VoiceRef 切换（Clone → seed-icl-2.0，Preset → seed-tts-2.0）；跨语种 audio_params 显式发送（pcm 24kHz / explicit_language=en / tone_fidelity=false）并把「.env.example 声称仅支持同语种」的风险注释在代码里，02-04 T4.0 探针是裁决者"
  - "D-07 双端：内部 ConfidenceSource 有三个值（Vendor/Proxy/ProxyUnavailable），线上联合只有两个——ProxyUnavailable 表示「字段缺失」，代理值永远无法冒充供应商原值（T-02-09）；新增字段全部可选且 #[serde(default, skip_serializing_if)]，Phase-1 形态事件继续可解析"

patterns-established:
  - "TDD 每任务 RED/GREEN 两提交（test → feat），所有测试无 key 可跑，真实 key 冒烟路径 #[ignore]"
  - "供应商客户端的错误与日志只记 host+path+status；签名 URL / headers / token 永不出现在 Display 或 Debug 中（测试断言）"
  - "TermHit 现在定型、Phase 2 恒为空数组：Phase 4 术语库直接填充，协议不再改"

requirements-completed: [AUDI-03, GOV-05, GOV-08, GOV-18]

# Metrics
duration: ~4h active over two sessions (2026-10-03 22:38 → 2026-10-04 16:40 +0800 wall, idle gaps included)
completed: 2026-10-04
---

# Phase 2 Plan 02: 供应商接入 Summary

**Four streaming vendor clients (讯飞 iat / Deepgram Nova-3 / DeepSeek SSE / 火山 ICL 2.0) behind typed stage contracts with latency marks at every boundary, deterministic offline mocks for all four, and the D-07 confidence/trace/abstained extension mirrored on both wire ends**

## Performance

- **Duration:** ~4h active across two sessions (~18h wall; the 2026-10-04 date rollover sits between Task 5 and the Task 6 GREEN)
- **Started:** 2026-10-03T14:38:00Z
- **Completed:** 2026-10-04T08:40:00Z
- **Tasks:** 6 auto tasks + 1 blocking-human gate cleared (Task 0)
- **Files modified:** 17 unique (10 Rust source, 2 Rust test files, Cargo.toml/lock, 2 TS protocol files, sim/source.rs)

## Accomplishments
- Three stage contracts with `provider()` / `model_version()` on every implementation — the ground floor of D-08 per-sentence attribution; every contract boundary is a 02-01 `Stage` mark (讯飞 first partial, translation first token, 火山 first audio).
- 讯飞 iat client: byte-exact HMAC-SHA256 signing (pinned against the reference script), wpgs `apd`/`rpl` reconstruction with an ordered frame list, `data.status == 2` as the only commit gate, 1280-byte audio framing under the 10163 base64 cap, 60s session rotation, and `sc` never read as confidence (it is a reserved zero — research correction 3).
- Deepgram interviewer-line client: `language=en` locked (multi refused outright), `Token` auth, buffer/flush + `UtteranceEnd` fallback, a 3s KeepAlive that defeats NET-0001, and `SpeechStarted` surfaced for 02-03 barge-in.
- DeepSeek translator: `\n\n` boundary buffer pinned against failure case 0001, deterministic config (temperature 0, no CoT model, ≤2-sentence window), structured output only (malformed → retryable, never an empty translation), usage frames captured for D-13.
- 火山 ICL 2.0 TTS: binary frame protocol ported field-for-field from `volc-tts-stream.mjs` with bounds-checked lengths, one binary request frame with a fresh UUID per connection, resource header following the voice, cross-lingual params explicit and risk-annotated for the 02-04 probe.
- D-07 protocol extension landed on both ends in one crate: TS union + `isServerEvent` guards and the Rust serde mirror agree field-for-field, new fields are optional, and Phase-1 shaped events still parse (backward compatible).
- Full verification green with zero keys: `cargo test` 116 lib + 31 mock_vendors + 3 latency_rig (1 `#[ignore]` live) + 5 session_integration; `pnpm -r test` 145 tests across 4 packages; root `pnpm build` green.

## Task Commits

Each task was committed atomically (TDD pairs):

1. **Task 0: dependency-legitimacy gate (hmac / sha2 / tokio-tungstenite 0.30)** — checkpoint:human-verify, `gate="blocking-human"`; user approved 2026-10-01 before any install
2. **Task 1 (T2.1+T2.6): stage contracts, error classification, routing config, four mocks** — `491749b` (test) → `0a00a55` (feat)
3. **Task 2 (T2.2): 讯飞 iat client** — `e1173dc` (test) → `a5bfe67` (feat)
4. **Task 3 (T2.3): Deepgram Nova-3 client** — `4186f8b` (test) → `fbc9ad0` (feat)
5. **Task 4 (T2.4): DeepSeek streaming translation** — `78304e5` (test) → `198f953` (feat)
6. **Task 5 (T2.5): 火山 ICL 2.0 streaming TTS** — `bc9b36b` (test) → `8a54a6b` (feat)
7. **Task 6 (T2.7): D-07 protocol extension, both ends** — `d5626b2` (test) → `f129caa` (feat)

**Plan metadata:** (this summary commit)

## Files Created/Modified
- `apps/desktop/src-tauri/src/pipeline/stages/traits.rs` — `SttSource`/`Translator`/`TtsSink` contracts, `SttPartial` (committed/revision/confidence/source), `VoiceRef`, `Vendor*` dispatch enums, scripted doubles
- `apps/desktop/src-tauri/src/pipeline/stages/error.rs` — `StageError` + `RetryClass`, 讯飞 code table, `classify_http_status`, sanitized endpoint
- `apps/desktop/src-tauri/src/pipeline/stages/config.rs` — `RoutingConfig`/`Endpoints`/credentials from env (`Secret<String>`, Debug `***`), missing-variable listing
- `apps/desktop/src-tauri/src/pipeline/stages/xfyun.rs` — 讯飞 iat WebSocket client (signing, wpgs, rotation, marks)
- `apps/desktop/src-tauri/src/pipeline/stages/deepgram.rs` — Nova-3 streaming client (en lock, KeepAlive, buffer/flush)
- `apps/desktop/src-tauri/src/pipeline/stages/deepseek.rs` — SSE translator (boundary buffer, window, structured output, usage)
- `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs` — 火山 ICL 2.0 TTS (binary frames, resource by voice, cross-lingual params)
- `apps/desktop/src-tauri/tests/mock_vendors.rs` — four deterministic mocks + failure injection; the whole plan's client tests live here
- `apps/desktop/src-tauri/src/lan/server.rs` — serde mirror gains `ConfidenceLevel`/`ConfidenceSource`/`AbstainReason`/`TermHit`/`SubtitleTrace`, `Subtitle.confidence|trace`, `Abstained` variant
- `apps/desktop/src-tauri/src/sim/source.rs` — two `Subtitle` construction sites gain `confidence: None, trace: None`
- `apps/desktop/src-tauri/tests/session_integration.rs` — provenance + abstain crossing the real LAN transport and surviving into the resume timeline
- `packages/protocol/src/index.ts` / `index.test.ts` — TS union, guards, 17 new tests
- `apps/desktop/src-tauri/Cargo.toml` / `Cargo.lock` — the dependency set above

## Decisions Made
See `key-decisions` in the frontmatter — dependency promotion rationale, per-vendor commit semantics, the two-value wire `ConfidenceSource`, and the 火山 cross-lingual risk note.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Dependencies beyond the three gate-approved packages**
- **Found during:** Task 1 (stage contracts + mocks)
- **Issue:** The clients need base64 (讯飞's wire format on both directions), thiserror (`StageError`), reqwest (SSE streaming with `stream` feature), uuid (火山 handshake request id) and a TLS backend for `wss://` (tokio-tungstenite has none by default). The plan's Task 1 action only named hmac/sha2/tokio-tungstenite.
- **Fix:** Promoted the four already-present transitive packages to direct dependencies at their resolved versions and enabled `native-tls` on tokio-tungstenite (macOS Security.framework; the rustls provider would drag in aws-lc-rs' cmake build). sha2 0.11 requires the 0.11-generation `digest`; the lockfile keeps sha2 0.10.9 for other transitive consumers — normal coexistence, verified in Cargo.lock.
- **Files modified:** `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/Cargo.lock`
- **Verification:** `cargo build` + full suite green; `cargo tree -d` shows only the expected duplicate `sha2`/`digest` generations
- **Committed in:** `491749b`

**2. [Rule 3 - Blocking] Two `ServerEvent::Subtitle` construction sites outside the plan's file list**
- **Found during:** Task 6 (D-07 extension)
- **Issue:** Once `Subtitle` gained `confidence`/`trace`, `sim/source.rs` failed to compile (E0063 missing fields) — the plan's Task 6 files list did not include it.
- **Fix:** Added `confidence: None, trace: None` at both sites (the simulated engine emits Phase-1 semantics; 02-03 supplies the real values).
- **Files modified:** `apps/desktop/src-tauri/src/sim/source.rs`
- **Verification:** `cargo test` full suite green (the engine's own byte-exact content tests included)
- **Committed in:** `f129caa`

**3. [Rule 3 - Blocking] Plan's verify command references a non-existent build script**
- **Found during:** Task 6 verification
- **Issue:** `pnpm --filter @nextalk/protocol build` exits `ERR_PNPM_RECURSIVE_RUN_NO_SCRIPT` — `@nextalk/protocol` has no build script (it is consumed as raw TypeScript via `exports: ./src/index.ts`).
- **Fix:** Substituted the root `pnpm build` (teleprompter + desktop production builds, which typecheck the consumers against the changed union) as the TS build gate, plus `pnpm -r test`.
- **Files modified:** none
- **Verification:** `pnpm build` green (teleprompter 421.82 kB JS / 133.81 kB gz), `pnpm -r test` 145 tests green
- **Committed in:** n/a (verification-only)

---

**Total deviations:** 3 auto-fixed (all Rule 3 blocking)
**Impact on plan:** No scope creep. All three were required to compile, install, or verify the planned work; the dependency promotion was disclosed in the Task 1 commit message.

## Issues Encountered
- The plan's Task 6 action says to extend `wire_shapes_match_protocol_package` "in tests/session_integration.rs" — that test actually lives in `src/lan/server.rs`'s test module. Both intents were honored: the serde shape test was added next to its sibling in `server.rs` (its own function `wire_shapes_carry_the_d07_provenance_fields`, leaving the Phase-1 assertions of the original untouched), and `tests/session_integration.rs` gained a transport-level test proving the new fields cross the real LAN WebSocket and survive into the resume timeline.
- The protocol package's TS files are not prettier-clean at HEAD, so no repo-wide format write was applied to them (no format gate exists in any package script); the new code follows the file's own local style.
- Pre-existing `unused variable: zh` warning and rustfmt drift in `sim/source_test.rs` remain out of scope (recorded in `deferred-items.md` since 02-01).

## User Setup Required
No new USER-SETUP.md was generated. The live smoke path (02-04 / manual) needs the credentials already documented in the PLAN frontmatter and `tools/vendor-experiments/.env.example` (`XFYUN_APP_ID`/`XFYUN_API_KEY`/`XFYUN_API_SECRET`, `DEEPGRAM_API_KEY`, `DEEPSEEK_API_KEY`, `VOLC_TTS_APP_ID`/`VOLC_TTS_ACCESS_TOKEN`). All tests in this plan run without any of them.

## Next Phase Readiness
- 02-03 can assemble the cascade: three contracts + `StageError`/`RetryClass` + the fixed routing config are in place, and every stage already emits its 02-01 `LatencyMark`.
- 02-04's probes inherit two explicitly flagged open risks: 火山 ICL 2.0 cross-lingual synthesis (T4.0 is the arbiter) and 讯飞's absence of real confidence (`sc` is a reserved zero — the proxy path is already typed as such).
- The abstain channel and provenance fields are on the wire but not rendered — 02-03 consumes them (red badges / D-08 reasoned cards), per the plan's explicit no-renderer-change scope.
- The `confidence` field the older product revisions moved to strategy cards (GOV-01/02) is now on the subtitle wire shape; 02-03 decides where it renders. The 2026-09-30 revision moved confidence presentation to strategy cards, so no renderer change is implied here either way.

---
*Phase: 02-real-cloud-pipeline-audio-core*
*Completed: 2026-10-04*

## Self-Check: PASSED

- Summary file: FOUND (`.planning/phases/02-real-cloud-pipeline-audio-core/02-02-SUMMARY.md`)
- Task commits: FOUND all 12 (`491749b`, `0a00a55`, `e1173dc`, `a5bfe67`, `4186f8b`, `fbc9ad0`, `78304e5`, `198f953`, `bc9b36b`, `8a54a6b`, `d5626b2`, `f129caa`)
- Key files: FOUND all 8 created artifacts (traits/error/config/xfyun/deepgram/deepseek/volc_tts + mock_vendors)

