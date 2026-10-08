---
phase: 02-real-cloud-pipeline-audio-core
verified: 2026-10-08T10:12:00Z
status: gaps_found
score: 2/5
overrides_applied: 0
gaps:
  - truth: "SC1: 用户对着默认麦克风说中文，≤2s（含冷启动）在耳机里听到本人克隆音色的英文，桌面与手机显示双语字幕"
    status: failed
    reason: "真实链路未装配进任何可运行路径：state.rs::start_session 仍启动 SimSource 模拟（连拒绝信息都是 'a simulated session is already running'）；'Cascade' 在 src/ 中除 cascade.rs 自身外仅剩一处文档注释（breaker.rs:131，运行时调用者为零）；UserAudioFeed::live（cascade.rs:378，真实采集进入级联的唯一入口）零构造点；CaptureChain / PlayoutChain / RoutingPlan::open 只被 tests/ 使用；lib.rs 的 13 个 #[tauri::command] 中没有任何一个启动真实会话，CpalStreamFactory 仅在两个只读状态命令里出现。02-05-PLAN Task 2 第 3 条明文要求『真实采集替换 02-03 的脚本输入；会话启动/停止接线到 state.rs 的既有生命周期』——未完成，02-05-SUMMARY 的 Known Stubs（L250『无阻塞性 stub』）未披露该缺口。更糟的是文档与代码相反：audio/mod.rs:8-9 声称 PlayoutChain 是『the jitter-buffered one the session runs on, with the real device attached』，:122-126 声称它是『what the cascade uses』；STATE.md L162 的 RESOLVED 记录同样失实。"
    artifacts:
      - path: "apps/desktop/src-tauri/src/state.rs"
        issue: "start_session (L338-363) 仅 replace_sim / reset_timeline / open_trace_writer / publish；持有的播放端是 PlayoutQueue（L23/101/108），不是 02-05 的 PlayoutChain"
      - path: "apps/desktop/src-tauri/src/pipeline/cascade.rs"
        issue: "Cascade / UserAudioFeed::Live 无生产调用者（全仓仅定义、impl 与 Debug impl）"
      - path: "apps/desktop/src-tauri/src/audio/mod.rs"
        issue: "模块文档（L8-9、L122-126）与运行时接线相反"
      - path: "apps/desktop/src-tauri/src/lib.rs"
        issue: "无真实会话命令；CpalStreamFactory::shared() 仅用于 audio_device_status / audio_routing_status"
    missing:
      - "把 CaptureChain → UserAudioFeed::Live → Cascade → PlayoutChain 装配进 state.rs 的会话生命周期（或由人工裁决改由 Phase 3 承接并同步修正 ROADMAP SC1 归属、STATE 记录）"
      - "修正 audio/mod.rs 与 STATE.md L162 中与代码相反的表述"
  - truth: "SC2: 延迟装置输出 mic→STT→翻译→TTS→播放 的逐级瀑布且 e2e 数字可见；超支可归因到阶段"
    status: failed
    reason: "装置机制本身已在脚本车道验证通过（2026-10-08 复跑 exit 0：冷 p50 1180ms / p95 1180ms，热 p50 1225ms / p95 1300ms，全部 ≤2000ms；assert_within_budget 归因超支阶段；CI latency-rig 车道硬失败），但真实链路一侧不可达：(1) latency_rig.rs::latency_e2e_cold 主体为 unimplemented!（L235），且 live_precondition（L197）在凭据齐全时也固定返回『真实级联阶段尚未接线（02-02/02-03）』——即使人工带齐全部密钥也永远拿不到 live 数字（本次以 --ignored 实跑证实：exit 101，前置门直接拒绝）；(2) Rust 侧零处 emit('latency')（全 src/ grep 零命中），而 useLatencyWaterfall.ts L269 监听该事件、L255 注释还声称『02-02 起由级联阶段发出』——02-01 SUMMARY 自列的收口项从未关闭；/diagnostics 在真实会话只能显示显式标注的『预览数据：未连接桌面测量装置』fixture（DiagnosticsPage.tsx L201-203）；(3) 02-REVIEW WR-02：Stage::PlaybackFirstSample 在排队（enqueue）而非设备消费时打点，e2e 系统性低估最多约 120ms（jitter pre-roll），与 budget.rs 的文档定义相悖。"
    artifacts:
      - path: "apps/desktop/src-tauri/tests/latency_rig.rs"
        issue: "live 变体 unimplemented!；precondition 第二道门保证永不通过"
      - path: "apps/desktop/src-tauri/src/lib.rs"
        issue: "零处 'latency' 事件 emit——前端契约无生产者"
      - path: "apps/desktop/src/hooks/useLatencyWaterfall.ts"
        issue: "监听 'latency'（L269）；无数据时回退 PREVIEW_SCRIPT（L188-194，硬编码 rig 数字，IN-03）"
      - path: "apps/desktop/src-tauri/src/audio/playout.rs"
        issue: "Stage::PlaybackFirstSample 在 push/入队时打点（WR-02）"
    missing:
      - "级联接线后实现 latency_e2e_cold（真实阶段 + WaterfallRecorder over RealClock）并 emit('latency') 供面板消费"
      - "把 PlaybackFirstSample 移到设备消费点（或同时保留入队点并显式命名），使预算数字与文档定义一致"
  - truth: "GOV-09 / 02-03 Task 5：真实会话的分阶段成本计量（STT 音频 ms / 翻译 tokens / TTS 字符）"
    status: partial
    reason: "StageUsage 零占位 stub 未被 02-04 关闭：TraceRecord::with_usage（trace/jsonl.rs:242）唯一调用者是同文件测试助手 usage_record()（L691，位于 mod tests 内，起始 L611）；全仓 grep StageUsage/stage_usage 在 jsonl.rs 之外零命中。02-03-SUMMARY 与 STATE.md L159 均登记为已知 stub 并点名挂载点（顺延 Phase 3/8 telemetry），但运行时用量面板与成本折算恒读 0（真实会话）。面板槽位与费率折算逻辑本身存在且测试通过（UsageMinutesPanel + QUOTA_HINT_TEXT 80% 提示，GOV-17）。"
    artifacts:
      - path: "apps/desktop/src-tauri/src/trace/jsonl.rs"
        issue: "with_usage 无生产调用者；StageUsage 默认全零（且 WR-05：费率表是 Gemini 价格，与实际的 DeepSeek 管线不一致）"
      - path: "apps/desktop/src-tauri/src/state.rs"
        issue: "会话生命周期未采集/写入阶段计数"
      - path: "apps/desktop/src/components/UsageMinutesPanel.tsx"
        issue: "UI 就绪，但真实会话读 0"
    missing:
      - "在级联/阶段层采集三类计数并在 TraceRecord 落盘处调用 with_usage；或由人工裁决正式改判为 Phase 8 遥测条目并同步 GOV-09 的阶段性完成度"
deferred:
  - truth: "audio_device_status / audio_routing_status 无前端消费者（设备选择器、路由状态 UI）"
    addressed_in: "Phase 3"
    evidence: "Phase 3 SC1『detects it via system_profiler, and shows routing status』；STATE.md L165 明记『设备选择器与 BlackHole 引导安装是 Phase 3 的界面面』"
  - truth: "回采（loopback）STT 副线的真实消费端"
    addressed_in: "Phase 3"
    evidence: "Phase 3 SC2『loopback-captured for STT』；02-05-SUMMARY L263-264 明记 Loopback 默认不启用、真实消费端属 Phase 3"
human_verification:
  - test: "真机拔插：cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test audio_devices a_real_device_unplug_is_survived -- --ignored（需真人拔耳机）"
    expected: "会话不中断；重建后音频恢复；开流 5s 超时、退避 250ms、上限 3 次、冷却 5s 生效"
    why_human: "需要真实硬件与人工拔插动作；自动化等价物（脚本化工厂 31 条）已全绿"
  - test: "真机采集+播放+回声回路：cargo test --test audio_chain a_real_microphone_feeds_the_chain -- --ignored 与 playout_a_real_device_plays_the_jitter_buffer -- --ignored；GUI 麦克风回路实测（11.55 dB 为合成回路数值）"
    expected: "1 秒输入恰 16000 样本；AEC 参考=设备实际消费；无回授"
    why_human: "需麦克风/扬声器；且 02-REVIEW CR-01（AEC 镜像帧几何）修复落地后必须重测"
  - test: "应用内注册全流程（GUI）：注册录音(1-3min) → 训练 → 试听 zh/en → 删除 voice/profile.json 回退检查"
    expected: "60-180s 录音校验、WER 门、预置回退；T4.0b 跨语种探针已 approved，此为 UI 端到端确认"
    why_human: "需 GUI + 麦克风；STATE.md L161 列为未跑人工项"
  - test: "live 延迟测量：cargo test --test latency_rig -- --ignored（需真实供应商凭据）"
    expected: "真实冷/热瀑布并以 ≤2000ms 判定，作为阶段门禁的人工记录工件"
    why_human: "需真实凭据与网络；且当前被 G2 代码性阻断（unimplemented! + precondition 双门），接线后才有意义"
  - test: "端到端核心链路演示：对默认麦克风说中文 → 耳机听到克隆英文（≤2s 含冷启动），桌面+手机双语字幕"
    expected: "ROADMAP SC1 全链路；当前被 G1 阻断（无可运行入口）"
    why_human: "真实设备 + 人耳判定；本阶段目标的最终验收动作"
---

# Phase 2: Real Cloud Pipeline + Audio Core 验证报告

**Phase Goal:** The real mic→headphones cascaded streaming translation chain works end-to-end within the 2s budget, with cloned-voice output and honest latency instrumentation gating all downstream phases
**Verified:** 2026-10-08T10:12:00Z —— 以 **main @ 53b06d4**（2026-10-06 15:02:24 +0800）为准，工作树干净（仅遗留 `.review-fix-recovery-pending.json` 与本报告两个未跟踪文件）。代码审查修复（CR-01 + WR-01..09）在验证进行中才有首个提交落到分支 `gsd-reviewfix/02-62927`（`3a3e286`，2026-10-08 18:03:41 +0800，7 files，+299/-42，仅含 CR-01），未并入 main——全部结论以 main @ 53b06d4 为准，修复落地后需按新 HEAD 重核相关条目。
**Status:** gaps_found
**Re-verification:** No — initial verification

**MVP 模式备注：** ROADMAP Phase 2 标记 `Mode: mvp`，但 goal 文本不是 "As a …" 用户故事格式（gsd-sdk 不可用，无法走 user-story.validate）。按编排者指示，以 ROADMAP goal + 5 条 Success Criteria 执行目标回溯验证，不生成 User Flow Coverage 段。

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| - | ----- | ------ | -------- |
| 1 | SC1：真实麦克风 → 克隆英文 ≤2s（含冷启动）+ 双端双语字幕 | ✗ FAILED | 真实链路零运行时装配（见 Gap 1）：`state.rs:338-363` 仍是 SimSource；`Cascade`/`UserAudioFeed::live` 运行时调用者为零；无命令启动真实会话。文档（audio/mod.rs:8-9,121-126；STATE L162）与代码相反 |
| 2 | SC2：逐级瀑布 + e2e 可见 + 超支归因 | ✗ FAILED | 脚本车道验证通过（复跑 exit 0，冷 p50 1180 / 热 p50 1225ms）；但 live 变体 `unimplemented!` + precondition 双门保证永不产出真实数字（实跑 exit 101）；Rust 零处 `emit('latency')`，面板仅显示显式标注的预览 fixture；WR-02 打点位置使 e2e 低估最多约 120ms |
| 3 | SC3：1-3 分钟录音注册克隆；未注册可用库存音色 | ✓ VERIFIED | 代码：60-180s/10MB/静音比校验（enroll/capture.rs）、训练+WER 门、`resolve_voice` 每片段解析、预置回退（voice_resolution 6/6 绿）；真实供应商：跨语种探针 verdict=approved（2026-10-05T13:38:11Z）+ 6 个 mp3 工件在盘。尚余应用内 GUI 人工过场（STATE L161） |
| 4 | SC4：抢话 ~100ms 停声无重叠；拔插不致会话崩溃 | ⚠️ PARTIAL | 自动化等价物全绿：epoch 守卫 + `FADE_OUT_MS=5ms` 幸存尾音、`playout_after_interrupt_never_overlaps`、设备重建脚本测试（audio_devices 31 条）。但应用内不可行使（Gap 1），真机拔插/回环为人工作业；`a_real_device_unplug_is_survived` 仍 `#[ignore]` |
| 5 | SC5：spoken English ⊆ committed finals 零违反 | ✓ VERIFIED | `admit()`（cascade.rs）唯一执法点；签名测试 `poisoned_partials_never_reach_tts`（cascade_integration.rs:177）过；`every_segment_records_a_complete_waterfall`、`interviewer_line_is_subtitle_only` 均过 |

**Score:** 2/5 条成功标准完整达成（SC3、SC5）；SC4 部分（自动化等价物绿、真机与应用内路径未行使）；SC1、SC2 被结构性缺口阻断。

### Deferred Items

| # | Item | Addressed In | Evidence |
| - | ---- | ------------ | -------- |
| 1 | 设备/路由状态命令无前端消费者 | Phase 3 | Phase 3 SC1「shows routing status」；STATE L165 |
| 2 | 回采 STT 副线的真实消费端 | Phase 3 | Phase 3 SC2「loopback-captured for STT」；02-05-SUMMARY L263-264 |

## Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `pipeline/budget.rs` | 五边界瀑布 + 预算断言 + 冷/热分离 | ✓ VERIFIED | `E2E_BUDGET_MS=2000`、`assert_within_budget`、双有界环；rig 复跑打印归因 |
| `tests/latency_rig.rs` | rig 集成车道 | ⚠️ PARTIAL | 脚本车道 3 passed；live 变体 `unimplemented!`（L235）+ 永不通过的前置门（L197） |
| `pipeline/cascade.rs` | 级联装配 + 提交门 | ⚠️ ORPHANED（运行时） | 实质、测试全绿；生产调用者为零（唯一 src 引用是 breaker.rs:131 注释） |
| `pipeline/stages/{xfyun,deepgram,deepseek,volc_tts}.rs` | 四家真实客户端 | ✓ VERIFIED | 契约/错误分类/固定路由齐；mock 全绿；`temperature:0` 锁定（deepseek.rs:237 + 测试）；真 key 冒烟为 `#[ignore]` 人工项 |
| `audio/{capture,playout,aec,resample,device,routing}.rs` | 实时音频内核 | ✓ VERIFIED（测试级） | 帧纪律 480/10ms、抖动缓冲硬上限 200ms、设备重建、双流路由；**但仅被 tests/ 构造，未接进会话**；CR-01（AEC 镜像帧几何）经 02-REVIEW 确认，修复首个提交已在分支（3a3e286） |
| `state.rs` | 会话生命周期 | ✗ FAILED（接线） | `start_session` 仍为模拟；播放端为 PlayoutQueue；无 Cascade/设备接线 |
| `trace/jsonl.rs` | JSONL 溯源 + 用量 | ⚠️ PARTIAL | 单写者 0600、schema、聚合与费率测试齐；`with_usage` 无生产调用者（Gap 3） |
| `enroll/*` | 注册采集/训练/档案/回退 | ✓ VERIFIED | 常量与门限齐；voice_resolution 6/6；真实探针工件在盘 |
| UI：`LatencyWaterfall/DiagnosticsPage/UsageMinutesPanel/ChatBubble`（两前端） | 面板与降级/弃权展示 | ✓ VERIFIED（UI）/ ⚠️ 数据 | GOV-14 文案双端到位；无置信微章（GOV-01/02 修订）；延迟面板数据源在真实会话断开（Gap 2），用量面板读 0（Gap 3） |

## Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `useLatencyWaterfall.ts` | Rust `latency` 事件 | `listen('latency')` (L269) | NOT_WIRED | 后端零 `emit('latency')`（含 lib.rs 的 4 处 emit 全部为 enrollment/session 类） |
| `Cascade` | 真实音频链（PlayoutSink / CaptureChain） | `UserAudioFeed::Live` / PlayoutChain | NOT_WIRED | 两条链的实现与测试都在，装配点缺一个构造者（零调用） |
| `state.rs` 会话 | 真实管线 | start_session → SimSource | NOT_WIRED | 模拟调度器仍为唯一会话实现 |
| `resolve_voice` | 火山 TTS 每片段资源 | per-fragment 解析 | WIRED | voice_resolution 6/6（含训练中生效、删除后不复活） |
| 协议（TS 闭集 ↔ Rust serde） | confidence/trace/abstained | D-07 双端 | WIRED | packages/protocol 测试 + src/lan/server.rs 镜像 |
| `usage_summary` | `UsageMinutesPanel` | Tauri command | WIRED（数据恒零） | 面板与 80% 提示就绪；数据源是零占位 StageUsage（Gap 3） |

## Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| 延迟面板 | waterfall report | `latency` 事件 | 否（无生产者；回退 PREVIEW_SCRIPT 并显式标注） | ✗ DISCONNECTED |
| 用量/成本面板 | minutes/usage | JSONL usage 字段（`with_usage`） | 否（StageUsage 恒零，仅测试填充） | ⚠️ STATIC |
| 双语字幕（双端） | subtitle events | state.rs timeline（模拟） | 是（模拟路径）；真实路径不存在 | ✓（模拟）/ ✗（真实，Gap 1） |
| 降级/待翻译气泡 | degraded/abstained 事件 | 级联 outcome → 协议 → UI | 是（mock/e2e 全绿；真实路径随 Gap 1 待接） | ✓（模拟） |

## Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Rust 全量套件 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`（含 MACOSX_DEPLOYMENT_TARGET=12.0 + pip-user-bin PATH） | 354 passed / 0 failed / 5 ignored，exit 0（本 HEAD；输出留存 /tmp/nextalk-cargo-test.txt） | ✓ PASS |
| 前端单测 | `pnpm -r test` | 176 tests（4 包）全绿，exit 0 | ✓ PASS |
| E2E | `pnpm exec playwright test` | 36 passed / 4 skipped（skips 为跨项目守卫：abstention/degraded 各 2 条在非所属项目跳过，正体均已执行），exit 0 | ✓ PASS |
| 延迟 rig（脚本车道） | `cargo test --test latency_rig -- --nocapture`（2026-10-08 复跑） | 冷 p50 1180 / p95 1180；热 p50 1225 / p95 1300（≤2000 判定在预算内），3 passed / 1 ignored，exit 0 | ✓ PASS |
| 失败案例库 | `run.mjs --run`（PATH 含 meson/ninja）+ 单独 `--check` 复跑 + 直查法复现存在性校验 | `--run` 20/20 exit 0（`main()` 先执行 runCheck、失败即中止，run.mjs L295-302）；`--check` 复跑『校验通过：20 条案例，字段/枚举/id/回归目标全部合规』CHECK_EXIT=0（约 6 分钟：每次 grep 全量遍历 18GB target/）；20 条 regression_test 目标直查逐条复现 0 缺失。注一：裸跑（不带 pip PATH）会因缺 meson 误报 1/20。注二：重负载窗口曾观察到 2 条假阴性（0002/0007：目标 mtime 未变、HEAD 含名、直查全中）——运行器把任何非零 grep 结果当『missing』的偶发误报，非案卷缺陷 | ✓ PASS |
| live 延迟探针 | `cargo test --test latency_rig latency_e2e_cold -- --ignored` | exit 101，panic：缺凭据（凭据齐全分支同样固定在代码层拒绝——见 Gap 2） | ✗ FAILED（设计性拒绝） |
| 债务标记扫描 | `grep -rn -E "TBD|FIXME|XXX|TODO|HACK|PLACEHOLDER|console\.log"`（阶段改动文件） | 零命中 | ✓ PASS |

## Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| live 冷启动测量 | `cargo test --test latency_rig -- --ignored` | exit 101；前置门拒绝（缺凭据分支实跑；凭据齐全分支经代码路径确认同样拒绝，unimplemented!） | FAILED（代码性阻断，非环境） |
| 真机拔插 | `cargo test --test audio_devices a_real_device_unplug_is_survived -- --ignored` | 未执行（需真实硬件+人工） | → human |
| 真机麦克风入链 / 真机输出播放 | `cargo test --test audio_chain a_real_microphone_feeds_the_chain / playout_a_real_device_plays_the_jitter_buffer -- --ignored` | 未执行（需真实设备） | → human |
| 真机麦克风注册采集 | `cargo test --test enrollment_capture a_real_microphone_take_records_and_saves_a_wav -- --ignored` | 未执行（需麦克风+TCC） | → human |
| 跨语种克隆探针（02-04 已执行） | `node tools/vendor-experiments/cross-lingual-clone-probe.mjs`（真 key） | verdict=approved @2026-10-05T13:38:11Z；6 个 mp3 工件在 `tools/vendor-experiments/artifacts/` | PASS（既往执行，用户裁决在案） |

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| AUDI-03 | 02-04（frontmatter 记 AUDI-05，编号漂移见下） | 音色注册 + 未注册库存音色 | ✓ SATISFIED（+GUI 人工项） | 采集门限/WER/档案/回退代码 + approved 探针 + 6 工件 |
| AUDI-04 | 02-02/02-03 | 级联流式管线 + 稳定性门 | ⚠️ PARTIAL | 阶段契约、提交门、重试/熔断、弃权/降级全绿；但真实链路端到端未装配（Gap 1），≤2s 仅有脚本双打证据 |
| AUDI-05 | 02-05 | AEC + 设备热变更 | ⚠️ PARTIAL | 内核与脚本测试完整；真机未验（human）；CR-01 修复首个提交在分支（3a3e286）未并入 |
| AUDI-06 | 02-01（frontmatter 记 AUDI-04） | 延迟测量装置门禁下游 | ⚠️ PARTIAL | 机制/CI/面板壳已验证；live 测量不可达 + 无事件生产者（Gap 2） |
| GOV-01/02（2026-09-30 修订） | 02-03 | 字幕不做置信标记 | ✓ | 双端 ChatBubble 注释与渲染均无置信微章；待翻译/降级为唯一特殊态；e2e 断言覆盖 |
| GOV-03 | 02-03 | 无声弃权「待翻译」 | ✓ | `abstention_fires_only_when_there_is_no_valid_text`、`low_confidence_never_abstains`；双端渲染 |
| GOV-04 | 02-03 | 数字/单位确定性校验 | ✓ | validate.rs + `numeric_mismatch_falls_back_to_the_original`（IN-01 为误弃权风险，fail-closed） |
| GOV-05 | 02-02 | 温度 0 / 非推理模型 / 滑窗 ≤2 | ✓ | deepseek.rs:237 + 锁定测试（L658） |
| GOV-06/07 | 02-03 | 逐句 JSONL 溯源 + 时间戳 | ✓ | 单写者 0600、schema、会话关闭落盘测试 |
| GOV-08 | 02-02 | 协议溯源字段双端 | ✓ | Rust serde 镜像 + TS 闭集测试 |
| GOV-09 | 02-03 | 分阶段成本计量 + 双面板槽位 | ⚠️ PARTIAL | 面板/费率/80% 提示在；计量恒零（Gap 3） |
| GOV-10 | 02-03 | 埋点复用 JSONL 结构 | ✓（结构） | 结构就位，上报属 Phase 8（非缺口） |
| GOV-12 | 02-03 | 片段级重试 2 次/100→200ms/500ms | ✓ | stability.rs retry 组 6 条全绿 |
| GOV-13 | 02-03 | 熔断 2 次/120s/半开 | ✓ | `retry_recovers_through_the_half_open_probe` 等 |
| GOV-14 | 02-03 | 降级展示（红微章+原文+正在重试） | ✓ | 双端 ChatBubble（desktop L165-166 / teleprompter L148-149）；degraded e2e 正体已执行 |
| GOV-15 | 02-03 | spoken ⊆ committed | ✓ | 签名测试 `poisoned_partials_never_reach_tts` |
| GOV-16 | 02-04 | 火山主选、v1 不配备用 key | ✓ | 探针 approved → 免备用；降级路径存在 |
| GOV-17 | 02-03 | 阈值提示式预算提示 | ✓ | UsageMinutesPanel L16/L19（80% 提示、非硬闸门） |
| GOV-18 | 02-02 | 固定路由（每阶段单一供应商） | ✓ | stages/config.rs 路由 + 漂移守护测试 |
| GOV-19/20 | 02-03 | 失败案例库 + 回归评测 | ✓ | 20 例 + runner + CI 第 5 车道；--run 20/20 且 --check 复跑校验通过 |
| GOV-11 | — | 管理后台看板 | n/a（Phase 8 定义内，不属 Phase 2） | ref/ai-governance-requirements.md L60 |

**孤儿需求检查：** REQUIREMENTS.md 把 Phase 2 映射为 AUDI-03/04/05/06，四者均被计划认领，无孤儿。**编号漂移（INFO）：** 各 PLAN frontmatter 的 AUDI 编号与 REQUIREMENTS.md 的内容口径不一致（如 02-01 是 rig 却记 AUDI-04；REQUIREMENTS 把 AUDI-06=rig、AUDI-04=级联）——REQUIREMENTS.md 自带注记承认该口径差；按内容而非编号对照，覆盖完整。

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| （阶段改动文件全集） | — | TBD/FIXME/XXX/TODO/HACK/PLACEHOLDER/console.log | — | 零命中：债务标记门禁 PASS |
| `src/audio/mod.rs` | 8-9, 121-126 | 文档声称 PlayoutChain 是会话运行/级联使用的那条链——与 state.rs 实际相反 | ⚠️ Warning | 误导后续装配者；STATE L162 同病 |
| `02-05-SUMMARY.md` | 250 | Known Stubs 声明「无阻塞性 stub」，未披露 Task 2 第 3 条会话接线缺失 | ⚠️ Warning | 完成度记录失真 |
| `tests/latency_rig.rs` | 197, 235 | live 变体保证失败（unimplemented! + 双门前置）却登记为“已就位” | 🛑 Blocker（Gap 2） | 阶段门禁的人工记录工件事上不存在 |
| `src/audio/playout.rs` | 275 | PlaybackFirstSample 入队打点（02-REVIEW WR-02） | ⚠️ Warning | e2e 低估 ≤~120ms，与文档定义相悖 |
| 02-REVIEW.md 交叉引用 | — | CR-01（AEC 镜像帧几何，真机回声消除退化）+ WR-01..09 | 🛑/⚠️ | 修复进行中（分支 gsd-reviewfix/02-62927 首个提交 3a3e286，仅 CR-01，2026-10-08 18:03 +0800，未并入 main）；CR-01 影响 SC1/SC4 真机语义，WR-02/03/08 分别并入 Gap 2、计量与装配议题 |
| `tools/vendor-experiments/failure-cases/run.mjs` | 96 | `grepFiles` 把所有非零 grep 结果（无匹配/被杀/报错）一律当作「目标不存在」 | ℹ️ Info | 重负载窗口产生过 2 条假阴性；建议区分 exit 1 与 signal/exit 2，并排除 target/ |

## Human Verification Required

1. **真机拔插**（STATE 人工门 a）— `cargo test --test audio_devices a_real_device_unplug_is_survived -- --ignored`，需真人拔耳机。预期：会话不崩、重建后恢复。自动化等价物已绿。
2. **真机采集+播放+回声回路**（STATE 人工门 b）— 两条 `#[ignore]` 音频测试 + GUI 麦克风回路；11.55 dB 为合成回路数值。**注意：CR-01 修复落地后必须重测**（AEC 镜像帧几何正是真机回声路径）。GUI 全链回路需待 Gap 1 接线。
3. **应用内注册全流程 GUI**（STATE L161）— 录音→训练→试听→回退。跨语种探针已 approved（T4.0b），此为 UI 端确认。
4. **live 延迟测量**（STATE 人工门 c）— `cargo test --test latency_rig -- --ignored` + 真凭据。**当前被 Gap 2 代码性阻断，接线前不可执行**；执行后其瀑布报告是 AUDI-06 真机达成的唯一记录工件。
5. **端到端核心链路演示**（ROADMAP SC1 的最终验收）— 说中文→听到克隆英文（≤2s 含冷启动）+ 双端字幕。**当前被 Gap 1 阻断**。

## Gaps Summary

Phase 2 的"部件"质量高且自动化门全绿（354 cargo / 176 vitest / 36 e2e / 20 失败案例 / rig 复跑 exit 0），五个成功标准中 SC3（克隆注册）与 SC5（spoken⊆committed）完整达成；SC4 的自动化等价物全绿但真机与应用内路径未行使。**但阶段目标的后半句没有在代码里成立："the real mic→headphones chain works end-to-end" —— 真实链路（CaptureChain → UserAudioFeed::Live → Cascade → PlayoutChain）没有任何运行时装配点：`start_session` 仍跑模拟器，级联与音频链仅有测试调用者，02-05 计划明文要求的会话接线未完成且未披露（Gap 1，BLOCKER）。由此，STATE 所谓"三项人工门/唯一证据缺口"的表述偏轻：人工门 (b) 的 GUI 回路与 (c) 的 live 测量在接线前对任何人都不可执行——(c) 甚至被 `unimplemented!` + 前置门代码性保证永不通过（Gap 2，BLOCKER）；同时 `latency` 事件永远没有生产者，/diagnostics 在真实会话只能显示显式标注的预览数据，而 WR-02 的打点位置让"诚实的 e2e 数字"再打折扣。Gap 3（GOV-09 的 StageUsage 零占位，02-04 未接、系统已登记为已知 stub）是面板数据恒零的根因，属部分达成。

另注：CR-01（02-REVIEW 判定 critical）与 WR-01..09 的修复在 `gsd-reviewfix/02-62927` 分支上进行（验证进行中出现首个提交 `3a3e286`，仅 CR-01，2026-10-08 18:03 +0800；未并入 main）；其修复完成后应针对本报告的 Gap 1/2 相关条目复核，但修复清单本身不包含"真实链路装配"这一结构性问题——它需要一份显式的计划（或人工裁决将其改判为 Phase 3 03-03 的交付并修正 ROADMAP/STATE 归属）。

---

_Verified: 2026-10-08T10:12:00Z_
_Verifier: Claude (gsd-verifier)_
