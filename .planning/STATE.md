---
gsd_state_version: 1.0
milestone: v3.4
milestone_name: milestone
status: executing
stopped_at: Phase 2 executing — 02-03 complete (2026-10-05)
last_updated: "2026-10-05T13:13:00.000Z"
last_activity: 2026-10-05 -- Completed 02-03-PLAN.md: cascade stability gate (commit gate / sentence aggregation / barge-in / retry+breaker / abstention+degraded) + JSONL trace & cost metering + 20-case failure library + usage panel
progress:
  total_phases: 8
  completed_phases: 1
  total_plans: 28
  completed_plans: 8
  percent: 29
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-26)

**Core value:** 让用户以母语思考、以本人音色讲出地道英文——端到端延迟 ≤ 2 秒
**Current focus:** Phase 2: Real Cloud Pipeline + Audio Core (02-03 done: cascade assembled behind the stability gate; 02-04 live probes next)

## Current Position

Phase: 2 of 7 (Real Cloud Pipeline + Audio Core)
Plan: 3 of 5 in current phase (02-03 complete; 02-04 next)
Status: Phase 2 executing — the cascade is assembled and stability-gated (spoken ⊆ committed enforced at one gate), barge-in/retry/breaker/abstention verified, JSONL trace + cost meter live, 20-case failure library in CI; 02-04 (live probes: 火山 cross-lingual clone arbitration, real stage counters) is next
Last activity: 2026-10-05 -- Completed 02-03-PLAN.md: 提交门（partial 渲染/仅 committed 下潜）+ 句子聚合与本地能量 VAD + epoch 守卫抢话队列（丢弃未播/封顶淡出/300ms 最小语音门限）+ 片段重试（2 次/100→200ms/500ms 预算）与熔断（2 次/120s/半开）+ 双语降级展示与静默弃权 + 数字校验复用锁定降级文案 + 单写者有界 JSONL 溯源与成本/用量面板 + 失败案例库 20 例（第五条 CI 车道）

Progress: [███░░░░░░░] 29% (8/28 plans, Phase 1/7 done, Phase 2: 3/5)

## Performance Metrics

**Velocity:**

- Total plans completed: 8
- Average duration: ~1d wall (01-01 26h active-session + gap; 01-02 6d wall, ~7h active; 01-03 ~1h active; 01-04 ~2.5h active over two sessions; 01-05 ~30 min active; 02-01 ~4d wall over two sessions; 02-02 ~4h active over two sessions; 02-03 ~5h active over three sessions, ~28h wall)
- Total execution time: 26h + ~7h + ~1h + ~2.5h + ~0.5h + ~2h + ~4h + ~5h active

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation + Simulation Mode | 5 | 5 | ~1d wall avg (incl. idle gaps) |
| 2. Real Cloud Pipeline + Audio Core | 3 | 5 | ~2d wall avg (all three span idle gaps) |

**Recent Trend:**

- 02-03 stability gate (2026-10-05): 14 commits (6 RED + 6 GREEN + 1 pin-test + 1 feat-only non-TDD task), cargo 232 tests (165 lib + 8 cascade_integration + 3 rig + 31 mock_vendors + 5 session_integration + 19 stability + 1 doctest; 1 live variant #[ignore]d) + 162 vitest across 4 packages + 37 playwright (4 skipped) green, failure-cases 20/20, build 428.06 kB JS / 135.26 kB gz, 5 auto-fixed deviations (3 Rule 3 + 1 Rule 1 + 1 Rule 2), zero keys required
- 02-02 vendor clients (2026-10-04): 12 commits (6 RED + 6 GREEN), cargo 155 tests (116 lib + 31 mock_vendors + 3 rig + 5 session_integration; 1 live variant `#[ignore]`d) + 145 vitest across 4 packages green, root `pnpm build` green, 3 auto-fixed deviations (all Rule 3 blocking) + 1 plan-accuracy note (`wire_shapes` test location), zero keys required
- 02-01 latency rig (2026-10-03): 5 commits (2 RED + 2 GREEN + 1 chore), 50 cargo tests (43 lib + 3 rig + 4 integration; 1 live variant `#[ignore]`d) + 35 vitest (8 files) + 33 playwright green, build 420.93 kB JS / 133.45 kB gz, 2 auto-fixed deviations + 4 documented design/scope decisions (overlap-aware constructor, CI build step, requirement numbering reconciliation, no literal workspace flag in ci.yml)
- 01-04 phone teleprompter (2026-09-11): 4 commits (1 RED + 1 GREEN), 27 vitest + 5 new playwright specs (10/10 with --repeat-each=2, 26/26 full suite) green, build 93.30 kB gz JS / 4.48 kB gz CSS, 8 auto-fixed deviations
- 01-03 desktop surface (2026-09-10): 5 commits (1 RED + 1 GREEN), 11 vitest + 19 desktop e2e (21 total across projects) green, build 128.70 kB gz JS / 5.37 kB gz CSS, 7 auto-fixed deviations
*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [02-03]: 提交门是 GOV-15「spoken ⊆ committed」的唯一执法点——Cascade 只在 committed 文本上向翻译/TTS 放行，partial 仅渲染；句子聚合 + 本地能量 VAD 决定 silence 闭合的段边界（cascade_integration / stability 两套集成测试钉住）
- [02-03]: 抢话 = epoch 自增先行（未播出音频整段丢弃、已播出部分带上限淡出 ~100ms）；最小语音门限 300–350ms 防自打断循环；02-05 的 jitter buffer 在此 playout 队列上扩展
- [02-03]: 片段级重试恰好 2 次、退避 100→200ms、500ms 预算内放弃该片段；供应商连续 2 次失败 → 熔断 120s → 半开探测（BreakerState::HalfOpen）
- [02-03]: 弃权只发生在「无有效文本」（低置信永不弃权）；字幕面不做置信标记（GOV-01/02 2026-09-30 修订，置信呈现归 Phase 5 策略卡）；数字/单位/日期校验失败 → 拒绝译文并复用锁定降级文案（T-02-14），不新造措辞
- [02-03]: 置信来源三值内部枚举（Vendor/Proxy/ProxyUnavailable）落 JSONL，线上仍两值（D-07：ProxyUnavailable = 字段缺席）；TraceRecord 用字段级 #[serde(default)] 保前向兼容，刻意不 derive Default（保 GOV-10 缺失即编译错的见证 doctest）
- [02-03]: JSONL 单写者：有界 mpsc 队列（1024，满即拒绝并计数 dropped_records，生产者永不阻塞磁盘）、8MB 滚动、0600、slug 防路径逃逸、手写 civil 日期（不引入 chrono）；费率表为具名常量——火山无公开费率，用 Fish S2.1 Pro CJK 参考价并注明
- [02-03]: 分阶段用量字段已就位但真实计数器未接（StageUsage 默认全零）——挂载点 TraceRecord::with_usage；接线后成本面板/剩余分钟数才有真实读数（02-04 装配或 Phase 3 telemetry）
- [02-03]: 失败案例 regression_test 必须是可执行测试标识（cargo:/vitest:），runner 先 grep 存在再执行；0001/0002 散文目标重写为真实测试；CI 失败案例车道为第五条（计划文本「第四车道」按现状修正）
- [02-03]: 用量面板为提示式（80% 阈值只提示不阻断，D-15/GOV-17）；用户面板只给分钟数、分阶段明细留 /diagnostics（D-13 分层）；无 IPC 桥 → 类型化 fixture + 显式「预览数据」标注（同延迟瀑布习惯）
- [02-03 Requirement numbering]: frontmatter requirements（GOV-01/02/03/04/06/07/09/10/12/13/14/15/17/19/20 + 内验 AUDI-06）属治理/内验编号；REQUIREMENTS.md 27 条 v1 编号中本计划唯一触点 = **AUDI-04**（级联装配与稳定性门）——已推进为 Complete (02-02/02-03)
- [02-02]: Dependency set beyond the three gate-approved packages — base64/thiserror/reqwest/uuid promoted from existing transitive deps to direct (讯飞 wire base64, StageError, SSE streaming, handshake UUID) and tokio-tungstenite gets `native-tls` (macOS Security.framework; rustls would drag in aws-lc-rs' cmake build). sha2 runs the 0.11 generation while 0.10.9 stays in the lockfile for other consumers — normal RustCrypto coexistence
- [02-02]: 讯飞 — only `data.status == 2` is `committed` (status 0/1 always false, GOV-15's ground floor); the `sc` field is never read (reserved zero), so confidence stays `None` with `ConfidenceSource::ProxyUnavailable` until 02-03's local proxy; wpgs `rpl` rewrites the frames inside `rg` via an ordered `Vec<(sn, text)>`, not string surgery
- [02-02]: Deepgram — `language=en` hard-locked (`multi` refused outright: it excludes Chinese and returns confident nonsense); `channel` is deliberately parsed untyped because VAD frames send an array while Results sends an object; the interviewer line emits NO `SttFirstPartial` mark (the 02-01 waterfall measures the user path only — asserted, not just commented)
- [02-02]: DeepSeek — `\n\n` framing happens before any parse (failure case 0001 cannot recur), `take_valid_utf8` holds cross-chunk multi-byte characters, temperature 0 / R1-family refusal / ≤2-sentence window are asserted, and a malformed response is a retryable failure — never an empty translation
- [02-02]: 火山 — the resource header follows the `VoiceRef` (Clone → `seed-icl-2.0`, Preset → `seed-tts-2.0`); `explicit_language=en` + `tone_fidelity=false` are sent explicitly, and the code documents that the 02-04 T4.0 probe is the arbiter of cross-lingual cloning (the .env.example contradiction is a known open risk, not a hidden assumption)
- [02-02]: D-07 wire shape — the internal `ConfidenceSource` has three values but the wire union has two: `ProxyUnavailable` means *the field is absent*, so a proxy estimate can never masquerade as a vendor one (T-02-09). New subtitle fields are optional and Phase-1 shaped events still parse
- [02-02 Requirement numbering]: AUDI-03/AUDI-04 reconciliation — GOV-05/GOV-08/GOV-18 are delivered by this plan (the governance ref doc tracks no checkboxes); the cascade requirement's acceptance (e2e ≤2s + spoken ⊆ committed) stays open because 02-02 only lands its raw materials — assembly is 02-03
- [02-01]: The five `Stage` boundaries are **streaming TTFB** instants (first partial / first token / first audio frame / first consumed PCM), not whole-request latencies — so the vendor numbers (讯飞 0.7s / 翻译 0.223s / 火山 1.3s) enter the rig only as per-stage *service durations*. `Waterfall::from_marks` derives durations as adjacent boundary gaps (serial reading, `serial_sum == e2e`); `Waterfall::from_marks_with_durations` takes the stages' own durations so the naive 2223ms sum can be compared against a ≤2000ms stopwatch. Without both, research correction 2 is unassertable
- [02-01]: Overlap is a *measured* quantity (`overlap_ms = serial_sum_ms - e2e_ms`), never an assumption — the rig prints it and the live variant refuses to pass without real stages
- [02-01]: Clock injection reuses `crate::sim::source::TimeSource` (Phase 1 contract); no second clock trait. `cold` comes from the session-start path, never from an elapsed-time heuristic (T-02-02)
- [02-01]: Aggregation keeps cold and warm in separate bounded rings (`MAX_TRACKED_SEGMENTS = 512`) and reports nearest-rank p50/p95 — cold-start numbers can never be averaged into the warm budget claim
- [02-01]: Requirement numbering reconciliation — ROADMAP success criterion AUDI-04 ("延迟测量装置") is REQUIREMENTS.md **AUDI-06** (marked complete by this plan); REQUIREMENTS.md AUDI-04 is the cascade pipeline itself (partial-render/final-speak gate) and stays open for 02-02/02-03
- [02-01]: The diagnostics panel reads a `latency` Tauri event that Rust does not emit yet — until 02-02 wires the stage marks, a real run shows 暂无测量数据; the browser/jsdom preview fixture appears ONLY when the IPC bridge is absent and is labelled 预览数据 on screen
- [01-05]: Tauri 2.11 has NO ACL namespace for app-defined commands (`gen/schemas/acl-manifests.json` lists only `core*`), so T-01-06 is enforced in the commands themselves — `interrupt`/`repeat` return Err unless the session is `generating`, `start_session` returns Err while one is live; `capabilities/default.json` was deliberately left untouched
- [01-05]: One event model, two transports holds end-to-end — the SimSource only appends to `SessionState.timeline`; the Tauri `session` emit and the WS broadcast are two projections of the same list, so console/dual/phone cannot drift
- [01-05]: `phone_count` is desktop-only telemetry on a Tauri event, never a WS ServerEvent — the locked 01-01 protocol union gained nothing for the client counter
- [01-05]: ChatBubble language resolution is `localPref ?? session mode ?? speaker default` — the phone's mode seeds every bubble the user has not personally toggled, keeping 01-03's per-bubble independence intact
- [01-05]: `repeat` (重听) re-emits a round under `-r{n}` ids with fresh seq (the phone's resume dedupe can never swallow a replay); `interrupt` (打断) cuts immediately and opens the next round at `+1000 ms` (`INTERRUPT_LEAD_MS`)
- [01-05]: SimSource determinism contract — `script_state(elapsed_ms)` is pure/IO-free and the scheduler takes an injectable `TimeSource`, so engine tests never sleep; scheduler cancellation is a u64 epoch ticket, not JoinHandle bookkeeping
- [01-05]: Vendor framework (D-04) ships zero dependencies and zero keys — Node built-ins only, TLS-only RTT tool that takes the credential's environment variable NAME (`--auth-env`, with `--auth-header`/`--auth-scheme` for Bearer/Token/raw vendors), and the report records `hasKey` (presence) only
- [01-04]: The phone owns ONE session-level language mode (中/EN/EN+中) pushed as {t:control,language} — per-bubble toggles stay desktop-only; the inbound `language` ServerEvent renders nothing on the phone (it is the desktop observation channel for 01-05)
- [01-04]: The phone's wake-lock fallback is a bundled 977-byte H.264 loop fetched with Vite `?no-inline` (real cached asset, zero CDN, no runtime media synthesis); it needs the same 开始提词 gesture as `wakeLock.request`
- [01-04]: Teleprompter tab is URL state (`?tab=ai`) written with history.replaceState so `?token=` survives; a phone waking from sleep returns to the tab it was reading
- [01-04]: Playwright clock discipline — `page.clock.install()` alone still lets real time through (a leaked tick made the 20-char typewriter checkpoint read 21); freeze with `install()` + `pauseAt()` before navigation and poll the DOM from Node, because Playwright auto-wait polls with page rAF
- [01-04]: e2e mocks the desktop with its own `ws` server on an EPHEMERAL port reached through the `?ws=` override — no port is reserved, and 8787 stays the untouched product default
- [01-03]: Language preference is per-bubble local state (not a global store) — each ChatBubble seeds itself from the speaker default (interviewer `bilingual`, user `all-zh`) and toggles independently, satisfying SYNC-03
- [01-03]: The locked @nextalk/protocol ServerEvent union has NO draft variant, so the UI-SPEC green draft timeline node has no producer in Phase 1 — AiTimeline ships context + strategy nodes only; a protocol decision is needed before the draft node can exist
- [01-03]: Testing Library auto-cleanup is not active (vitest globals are off) — React specs MUST call afterEach(cleanup) explicitly or a prior render leaks into the next query
- [01-03]: Voice enrollment probes the real microphone via navigator.mediaDevices.getUserMedia as the Phase-1 permission path, releasing all tracks on stop/unmount; actual capture/cloning is Phase 2+. e2e injects a deterministic navigator.mediaDevices (deny + grant paths) rather than depending on headless-Chromium permission behavior
- [01-03]: PageStub deleted — all six previously-stubbed routes (setup / voice / glossary / resume / recordings / review) render real pages with locked Chinese copy and 模拟数据 badges
- [01-03]: ReviewPage exposes 生成报告/重新生成 so the 暂无复盘报告 empty state is reachable, not dead; RecordingAssetCard export actions render disabled until Phase 6; the voice-sample 试听 tile is an explicitly labelled Phase-2 placeholder
- [01-03 Open Question 3]: H5 accepts an optional `ws=` URL override param (default stays ws://{same-host}:8787) — required for e2e mock-server isolation; token stays mandatory so no added spoofing surface (T-01-01 gate unchanged)
- [01-02 e2e infra]: Playwright teleprompter preview moved 8787 → 8791 — an unrelated long-running local tool (tools/jd-inbox-server.mjs) squats 127.0.0.1:8787 on this dev machine; product default port 8787 unchanged (see Blockers)
- [01-02]: axum 0.8 dropped root nesting — ServeDir mounts via fallback_service + no-store override header; serde internally-tagged enums put deny_unknown_fields at the CONTAINER level (variant-level is a compile error)
- [01-02]: rand 0.10 API — OsRng → SysRng + TryRng::try_fill_bytes for the 128-bit pairing token (T-01-01)
- [01-01 Task 1 gate]: USER APPROVED all 20 npm audit-table packages (2026-08-28) after re-verifying @fortawesome/fontawesome-free 6.7.2 against the npm registry — repo matches FortAwesome/Font-Awesome, scripts={}, publisher fortawesome-admin, dist.integrity present; observed weekly downloads 2.5M vs audit's ~15M (same magnitude, not a risk)
- [01-01]: Rust stable 1.98.0 (2026-08-18) builds against Xcode 14.2 — Open Question A2 resolved favorably, no 1.85 toolchain pin needed
- [01-01]: pnpm 11.24.0 via corepack; locked `packageManager: "pnpm@11.24.0"`; pnpm 11 `allowBuilds` approved for core-js + esbuild postinstall scripts
- [01-01]: @vitejs/plugin-react pinned 5.1.4 (plan's 6.1.0 requires vite 8; plan locks vite 7.3.6)
- [01-01]: @playwright/test pinned 1.53.2 (plan's 1.62.1 cannot install browsers on macOS 12; 1.53.2 is the newest mac12-compatible line — upgrade blocked until OS upgrade)
- [01-01]: contract tests read tokens.css via node:fs (vitest 4 stubs .css imports incl. ?raw)
- [Roadmap]: Follow research build order — protocol/UI first on SimSource (driver off critical path), real pipeline + latency rig before virtual device, copilot after transcript pipeline, consent gate ships with recording
- [Roadmap]: AUDI-07 glossary term protection mapped to Phase 4 (per research); glossary page UI built in Phase 1, wiring into Phase 2 pipeline stages happens in Phase 4
- [Roadmap]: Phase 7 Productization carries no v1 requirement mappings — distribution hardening per research (signing/notarization, clean-machine test, compliance)

### Pending Todos

[From .planning/todos/pending/ — ideas captured during sessions]

None yet.

### Blockers/Concerns

[Issues that affect future work]

- Phase 1 must run the decisive vendor experiments (STT A/B, clone listening test, network RTT) before Phase 2 stack wiring — results may change provider choices
- Claude Sonnet 5 intro pricing and Fish Audio free tier end 2026-08-31 — cost model must assume post-intro pricing
- BlackHole install/signing is a process risk (Gatekeeper, notarization, meeting-app device caches) — isolated in Phase 3
- Playwright capped at 1.53.2 while this machine runs macOS 12 (1.62.1 refuses mac12) — OS upgrade unblocks newer Playwright; e2e runs on the 1.53.2 chromium build
- ~~01-03/01-04 entry CSS must `import '@nextalk/design-tokens'` and `import 'core-js/proposals/promise-with-resolvers'` (Pitfall 5)~~ RESOLVED 2026-09-10: both entries comply — the desktop `main.tsx` boots with the core-js polyfill import first and keeps `@nextalk/design-tokens` before `./styles/global.css`; the teleprompter entry complied in 01-02
- `tsc` is unusable as a gate: `@types/react` / `@types/react-dom` are not installed in the workspace, so typecheck output is dominated by pre-existing errors (see 01-03 deferred-items.md)
- 01-03 Task 4 `<human-check>` (`pnpm --filter @nextalk/desktop tauri dev` manual walkthrough) is still outstanding — the executor ran the full automated suite (21 e2e across both projects) but cannot drive a GUI session
- Dev-machine port collision: an unrelated long-running tool (tools/jd-inbox-server.mjs) holds 127.0.0.1:8787 — the desktop LAN server binds 0.0.0.0:8787 and will EADDRINUSE while that tool runs (app degrades gracefully: bind failure logged, app continues); e2e previews already moved to 8791. Stop the tool before real-device pairing tests
- 01-04 Task 3 `<human-check>` (real-device pass: QR scan → 开始提词 → screen awake ≥2 min → wifi kill 10s → reconnect/resume) is still outstanding — the executor has no phone or camera. Automated equivalents are green (mock-WS e2e covers pairing mount, wake fallback and reconnect/resume); run the hardware pass before `/gsd:verify-work`
- 01-05 Task 2 `<human-check>` (interactive `pnpm --filter @nextalk/desktop tauri dev` demo pass: QR scan → 开始模拟会话 → r1 flows to console + dual + phone in sync → phone count flips to 已连接 1 台设备 → phone mode switch → 打断/重听 → ended) is outstanding — needs a GUI session + phone + camera. Every leg has a green automated equivalent (29 cargo tests incl. the real-WS integration test; 29 playwright specs incl. demo.spec.ts); run it before `/gsd:verify-work`
- Phase 1 is code-complete (5/5 plans, all automated gates green at HEAD) but its three human passes (01-03 desktop walkthrough, 01-04 real-device phone, 01-05 full demo) are the remaining end-of-phase manual checks
- Playwright e2e now proves the mock-WS flow for BOTH surfaces; the true QR → phone path (real LAN server + real token) is the manual end-of-phase check per plan
- 02-03 Known stub: TraceRecord 的 StageUsage（STT 音频 ms/翻译 tokens/TTS 字符）默认全零——真实会话的成本面板与用量分钟数读 0，直到阶段计数接线（挂载点 TraceRecord::with_usage；02-04 装配或 Phase 3 telemetry）
- 02-03: 失败案例 0017（跨语种克隆握手被拒）目标已是真实测试（volc_client_classifies_a_rejected_handshake_and_leaks_nothing），但案例措辞是占位——02-04 T4.0 锁定跨语种克隆结论后回填

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 20260930-dualpane-stop-button | 双栏扩展视图「停止」按钮（锁定确认 + stop_session + 终态回空态） | 2026-09-30 | 56f90f4 | [20260930-dualpane-stop-button](./quick/20260930-dualpane-stop-button/) |
| 20260930-stop-keeps-stream | 停止后双栏保留字幕与策略（与手机端一致），仅 session_started 清空（撤销 246baae） | 2026-09-30 | a442841 | [20260930-stop-keeps-stream](./quick/20260930-stop-keeps-stream/) |

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-10-05T13:13:00.000Z
Stopped at: Completed 02-03-PLAN.md (stability gate: preview-vs-commit enforcement, sentence aggregation + local VAD, epoch-guarded barge-in, fragment retry + per-vendor breaker + Chinese-locked degraded display, deterministic numeric validation + silent abstention, single-writer bounded JSONL trace + cost/usage panels, 20-case failure library in CI) — 02-04 (live probes: cross-lingual clone arbitration + real stage counters) is next
Resume file: .planning/phases/02-real-cloud-pipeline-audio-core/02-03-SUMMARY.md
