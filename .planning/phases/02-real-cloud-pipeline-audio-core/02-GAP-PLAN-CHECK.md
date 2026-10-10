---
phase: 02-real-cloud-pipeline-audio-core
check: gap-plan-check (02-06 / 02-07 / 02-08)
status: concerns
date: 2026-10-10
plans_checked: [02-06, 02-07, 02-08]
blockers: 1
warnings: 6
info: 6
---

# Phase 2 Gap-Plan Check — 02-06 / 02-07 / 02-08

**结论：concerns** — 1 个 blocker（02-06 的 epoch 错配会在第二次会话起静默拒收全部音频），6 个 warning，6 个 info。
三个缺口（SC1 装配 / SC2 live 延迟门 / GOV-09 计量）在结构上都有对应任务，且接口摘录与原码一致；但 02-06 的播放世代（play generation）与取消票据（session epoch）被混用——首会话之外全部失效，且计划的测试拓扑（全是首次会话）抓不到它。该缺陷会同时污染 02-07 的 live 延迟测量。

未修改任何 PLAN.md，未提交任何内容。

---

## Blockers（必须先改）

### B1. [task_correctness / key_link 接线] 播放世代与 session epoch 错配——第二次会话起全部 TTS 音频被静默拒收

- Plan: `02-06`（Task 1 drive_live / Task 3 start_live_session）；`02-07` 继承同一驱动契约。
- 事实链（均已对码验证）：
  - `PlayoutQueue.epoch` 从 **0** 起（`audio/playout.rs:315` `AtomicU64::new(0)`）；`begin_session()`/`end_session()` = `new_generation()` = `fetch_add(1)+1`（playout.rs:595-624）→ 一条**新建**链首次 `begin_session()` 后世代恒为 **1**。
  - `PlayoutChain::push` 拒绝 `epoch != queue.epoch()`（playout.rs:928-936 → `StaleChunk`）；
  - `impl PlayoutSink for PlayoutChain` 里是 `let _ = self.push(...)`（playout.rs:1186）——**静默丢弃**，无日志无事件。
  - `SessionState.session_epoch` 从 0 起（`state.rs:116`），**start 与 stop 各 +1**（`state.rs:383`、`state.rs:406`）。
  - 计划：`start_live_session` "额外 `chain.begin_session()`"（02-06 L313，返回值被丢弃），随后把 **session epoch** 一路穿过 `run → drive_live(epoch) → speak_segment(segment, epoch)` 用作入队戳（02-06 L249-251）。
- 账：首会话（进程内第一次启动真实会话，N=1）恰好碰上链世代 1 → 通过。此后任意情况下：模拟会话后切真实（N≥2）、或真实会话**停止再开始**（N=3,5,…）→ 链是每次 `assemble()` **新造**的（世代永远回到 1）→ 所有 chunk 以戳 N≠1 被拒 → 无音频、无 `PlaybackFirstSample` 打点（真机"第二次就没有声音"，且 02-07 的 live 瀑布会以"缺末级打点"的形式假阴性）。
- 根因一句话：**模拟路径世代与 session_epoch 锁步**（同一持久队列，start/stop 都各 +1），**真实路径每次重建队列**，世代不随 N 走——计划的既有心智模型在这里不成立。
- 计划的测试抓不到：Test 2（生命周期）与 Test 7（停止撕裂）都是**单次会话内**的拓扑；没有任何 stop→start→再来一段的用例。
- 计划自己的 must_haves 已被证伪：truth 3"epoch 语义与 02-03 一致"（02-06 L25）在重启场景下不成立；key_link L54 又把 session_epoch 正确定义为"取消票据"——恰恰说明播放戳不该搭这班车。
- Fix（二选一，建议 a）：
  a) 拆成两个值：`start_live_session` 保留 `chain.begin_session()` 的返回值作为 `play_generation`（随 `live_playout` 一起存），`run → drive_live(feed, play_generation, current_epoch…)` 入队戳用 play_generation，`current_epoch` 只做取消检查；供应商会话 start 打标用哪一个均可（注明即可）。
  b) `PlayoutChain::epoch()`（playout.rs:875）已存在——run 在驱动前读取 `kit.chain.epoch()` 作为入队戳，禁止把 session epoch 传进入队路径；并在 drive_live 签名里把两个 epoch 明确命名区分。
  - **配套必加测试**：live 会话 start→一段→stop→start→再一段，断言第二会话 `accepted_chunks ≥ 1`（或 `PlaybackFirstSample` 打点存在 / `stale_chunks == 0`）。02-07 的 rig 也必须改用链自身的世代，不得传任选值。

---

## Warnings（建议修，不修会降质或误诊）

### W1. [task_correctness] `End` 在供应商 final 之后才发送——空队列上的 SendError 会杀死真实会话；且脚本双打与真实时序相反

- Plan: `02-06` Task 1（L251-252）。
- 实现事实：`Closed` 只在 `push_committed(is_final=true)` 时产生，而 live 的 is_final 来自 xfyun 收到 `status==2` 后的 `FrameOutcome::Finished → run_session return`——**任务返回时 upstream receiver 与 events sender 一起被 drop**。等驱动看到 `Closed` 再 `stream.upstream.send(SttUpstream::End)`（mpsc::Sender::send 的 `Result<_, SendError>`）大概率对已关闭的队列发送；`stream.next_event().await` 之后也只会拿到通道关闭。若计划代码用 `?`/`expect`，真实会话在第 1 段后即死。
- 脚本双打的时序恰好相反（双打等 `End` 才发 final、并保持 receiver 存活），所以计划 Test 1"每片段恰一次 End 收尾"的断言**结构上验证不到**真实路径的行为。
- Fix：明确 `.send(End)` 为 best-effort（`let _ =` / log），排空阶段必须容忍通道已关闭（收到 None 即正常退出并转入 speak_segment）；新增一个"final 后立即关闭双通道"的测试双打钉住该容忍行为。
- 附带需人工核实的期望管理：本流程的片段闭合**依赖服务端 eos 自决**（`DEFAULT_EOS_MS=2_000`，随每帧发送）。若实跑 30s 超时（"未检测到语音"），第一排查项应是"服务端是否必须收到客户端 End 才 finalized"——若是，则需把 End 提前到本地静默判定点（当前 Segmenter 事件面没有该信号，需要小改驱动，属计划应预案的分支）。

### W2. [task_completeness] `SegmentScript.pcm16` 新增字段的编译波及面超出 files_modified

- Plan: `02-06` Task 1 加 `pub pcm16: Vec<i16>`（cascade.rs 的 `SegmentScript`）。全仓构造点：`tests/cascade_integration.rs`（在 Task 1 files ✓）、`tests/stability.rs:463`（**不在**任何 files 列表）、`tests/voice_resolution.rs`（约 8 处，**不在**列表）。
- `cargo test`（Task 3 verify 的全量）与 Task 1 的 `cargo test … cascade::` 都编译全部 test target → 不更新 stability/voice_resolution 就编译失败。
- Fix：把 `tests/stability.rs`、`tests/voice_resolution.rs` 加入 Task 1 的 `<files>` 与 frontmatter `files_modified`（两处只是补 `pcm16:` 字段，机械改动）。
- 另：frontmatter `files_modified` 缺 `tests/audio_devices.rs`（Task 2 `<files>` 有）；建议 frontmatter 与任务 files 并集对齐，避免下游工具按 frontmatter 装订文件清单时漏项。

### W3. [task_correctness] FaultSender 接线缝未定义——计划声称的动作在其自己声明的签名下做不到

- Plan: `02-06` Task 2（L279）。
- 计划先定义新缝 `open_output_with_render(request, render)`，随后又说"错误回调闭包在 manager 的 open_stream 里组装（`let sender = self.fault_sender(); …`）"。但 `OpenRequest`/新方法都没有携带 sender 的位置——manager 无法把闭包递给工厂。现状佐证：`CpalStreamFactory::open` 的错误回调是 `|_error| {}`（device.rs:855-856），`FaultSender::callback()` 机制存在（device.rs:308）但无路径进入输出流。
- 后果（SC1 缺口）：输出设备故障（流失效）永远到不了 `DeviceManager`，`report_fault/attempt_rebuild` 对输出侧形同虚设。
- Fix：二选一并写明——新方法加第三参 `on_error: impl FnMut(cpal::Error) + Send + 'static`（manager 用 `fault_sender.callback()` 组装），或 `OpenRequest` 增 `Arc<FaultSender>` 字段；并补一条失败注入测试（模拟 on_error 触发 → `poll_faults()` 产出 Fault）。

### W4. [task_correctness] run 退出未调用 `manager.stop_session()`——输出流泄漏，重启会叠开第二条输出流

- Plan: `02-06` Task 3（L310）："退出前 capture.stop + chain.end_session"——`LiveKit::run` 持有 `manager`，但退出路径没有 `manager.stop_session()`。
- 实现事实：输出流在 `assemble()` 第 ⑤ 步 `DeviceManager::start_session()` 时打开；只有 `DeviceManager::stop_session()`（device.rs:554-564）会停它。停止后 render 闭包与 cpal 输出流继续存活；下一次开始又 `assemble()` 开新流——同一设备两条输出流。
- Fix：run 的退出路径（epoch 超越 / 错误退出）补 `self.manager.stop_session()`；Test 7 顺手断言（ProbeFactory 可记录 stop 次数）。

### W5. [verification_derivation] 文档修正的否定式 grep 是空门（源码换行，永远 grep 不到）

- Plan: `02-06` Task 3 verify：`! grep -q "the jitter-buffered one the session runs on, with the real device attached" src/audio/mod.rs`。
- `audio/mod.rs` 第 8-9 行该句**跨行**（L8 结尾 "…is the"，L9 开头 "//!   jitter-buffered one…"）→ 该 grep 对修改前后的文件都 **mismatch**，`!` 后恒真——它守护不了任何东西；真正生效的只有后面的 `grep -q "02-06"`（弱：任意一处 "02-06" 即通过）。旧的失实句若原样遗留、同时别处补了 "02-06"，门照过。
- Fix：改成行内子串断言 `! grep -q "with the real device attached"`（该子句完整落在 L9 内），并把 `grep -q "02-06"` 收紧为对新句（如 "the live session runs on (02-06"）的断言；或直接对三个点名位置（L1/L8-9/L121-126）逐句断言。

### W6. [verification_derivation] live e2e 计时口径包含整句发言 + 供应商 eos（2s）——首次真实测量大概率超支；需要预告知"超支是阶段发现，不是放松断言的理由"

- Plan: `02-07` Task 1。
- 事实：e2e 打点跨度 = `MicCallback`（VAD 起讲即开段）→ `PlaybackFirstSample`（设备消费点）；真实片段闭合依赖服务端在静音 `eos=2_000ms` 后自决（drive_live 不会提前发 End）。本句 ~1-3s + eos ~2s + 管线 ~0.9s ⇒ 首次诚实 live 数字结构性 >2000ms，`assert_within_budget` 硬失败。
- 这不是测量造假——恰恰是门在干活。但计划应写明：(a) e2e 口径含发言时长与端点等待的既有事实；(b) 人工门若记录超支，这是**阶段级发现**（降 eos / 提前 End / 重新定义口径三选一），不得以放宽断言"修复"。
- Fix：在 02-07 的 verification/人工项指引中加一句口径说明与升级路径；断言保持硬。

---

## Info（可选优化）

1. **[task_correctness]** 02-06 L250 的 `drive_live` 动作文字类型混乱：说"把 `script.pcm16` 转字节后按 1280 字节切片经 `SttUpstream::Audio` 送入"，但 `SttUpstream::Audio(Vec<i16>)`（计划 L133 自己引用是对的），且 LE 字节化与 40ms 分帧已在 xfyun 客户端内部完成（xfyun.rs:689-697 / audio_frames）。编译会强制收敛，但建议把动作文字改为"直接 `send(SttUpstream::Audio(script.pcm16.clone()))`，不改帧纪律"。
2. **[research_resolution]** 02-RESEARCH.md 的 Open Questions：Q1-5 均有 "(RESOLVED → …)" 标记，Q6（failure-case library 位置）无标记——它由 D-20 委托规划者裁决且 02-03 已落地；补个 RESOLVED 标记即可。
3. **[verification_derivation]** 02-07 Task 1 verify 的 `env -u … | grep -q "XFYUN_APP_ID"` 只断言输出含 key 名，cargo 的退出码被管道吞掉（无 pipefail）——与"点名缺失 key"的设计意图一致，但测试本身的失败未被独立断言；可加 `test ${PIPESTATUS[0]} -ne 0` 双保险。
4. **[dependency_correctness]** 02-06 frontmatter `wave: 1` 与 `depends_on: [02-05]` 并存（缺口计划自成波次集）；调度以 depends_on 为准则无害。
5. **[task_completeness]** 02-06 Task 1 `<files>` 列了 `pipeline/mod.rs` 但该任务动作不碰它（Task 3 才为 `pub mod live;`）——冗余，无害。
6. **[scope_sanity]** 02-06 = 3 tasks / frontmatter 9 文件（实际 ~11，修 W2 后 ~13；frontmatter 的 10 文件 warning 线附近）。三任务分层严格（drive_live → 渲染缝 → 装配），当前切分合理；若执行中上下文吃紧，Task 2 可独立拆出。

---

## 维度速览（其余维度均通过）

| 维度 | 结论 |
|---|---|
| 需求覆盖 | PASS：AUDI-04/05→02-06，AUDI-06→02-07，GOV-09（D-13）→02-08；02-VERIFICATION Gap 1/2/3 各映射到具体任务 |
| 任务完整性 | PASS（W2 修文件清单、W1/W3 改动作细节后更稳）；全部任务有 files/action/verify/done + `<automated>` |
| 依赖/波次 | PASS：02-06(w1,dep[02-05]) → 02-07+02-08(w2,dep[02-06])，无环、无前向引用；02-07∩02-08 文件交集 = ∅，可并行 |
| 关键接线 | B1 除外全在：assemble→start_live_session→run→drive_live→speak_segment、publish_waterfall、publish_with_usage 均有任务 |
| must_haves 推导 | 基本 PASS（truth 均为用户可观察）；02-06 truth 3 "epoch 语义一致" 被 B1 证伪 |
| 上下文合规 | PASS：2026-09-30 修订全部兑现（翻译链无置信度标记→`confidence: None`；弃权仅限无音频；火山 ICL 2.0 TTS；D-13 计量轴；D-18 纯本地 JSONL） |
| 范围缩减检测 | PASS：无 "v1/简化/暂不接线" 类降级话术；三缺口均为完整交付 |
| 架构层级合规 | PASS：RESEARCH 责任图（Desktop Rust core 持有全部能力）与任务落点一致 |
| Nyquist | PASS：02-VALIDATION.md 存在；任务自动化验证齐备（门禁无 watch 模式、无全量 E2E 依赖） |
| 跨计划数据契约 | PASS：02-08 只读 02-06 的 ledger/事件，无冲突变换 |
| CLAUDE.md 合规 | PASS（无冲突指令命中） |

---

## 六问回答

**Q1（缺口覆盖）**：三缺口都各有能闭合它们的任务集——Gap 1（02-06：assemble/LiveKit/run/start_live_session/drive_live/attach_render/resolve_backend）、Gap 2（02-07：前置门改写 + cold/warm 真实主体 + 生产者契约）、Gap 3（02-08：publish_with_usage + stage_usage_for + 契约测试）。但 Gap 1 是"半闭合"：**B1 不修则重启即静默无声**（且拖累 Gap 2 的测量）；W1/W3/W4 决定真实设备侧是否完整（End 容错、输出故障上报、流释放）。

**Q2（Gap 1 对码正确性与停止/重启）**：装配顺序（factory→profile→plan→processor→capture→chain→manager→stage 构造）与全部被引接口（`Cascade::new` 签名、`VendorStt/Translator/Tts`、`from_lookup`、`CaptureChain::start`、`DeviceManager` API、`PlayoutChain` 的 epoch 语义）逐条对码**一致，无 skeleton 漂移**。但停止/重启**未被覆盖**：计划声明的 Test 7（停止撕裂）+ Test 2（生命周期）都是单会话拓扑；B1 恰好只在第二次会话暴露——"停止撕裂后重启正常"目前是计划声称、实际不成立。修复 = B1 的世代拆分 + 一条 stop→start→再驱动 的回归测试。

**Q3（Gap 2 前置门与降级）**：**是，能防住两者**——前置门只剩凭据检查，带齐 key 不再是设计性拒绝；无 key 路径 `env -u` 实跑非零退出并点名缺失 key（verify 用 grep 断言名字）；装配失败 → panic（code+message）；30s 无语音 → 显式失败，不挂起、不产假瀑布；脚本车道与 live 变体分离（零网络 Test 5 亦复用同一驱动助手，防止两套逻辑漂移），live 变体 `#[ignore]` 仅在人工门执行。两个待修点：rig 必须用链自身世代（B1 继承，否则 live 测量以"缺 PlaybackFirstSample"假阴性）；e2e 口径预期管理（W6）。

**Q4（Gap 3 协议闭集）**：**未触碰**。`lan/server.rs` 的 `ServerEvent` 无 usage 变体、`SubtitleTrace` 无 usage 字段（该文件 "usage" 零命中）；用量只写入本地 `TraceRecord`/JSONL（0600 既有边界）；02-08 Test 4 以 serde 输出无 `usage` 键钉死该守护；TS 协议包零改动。

**Q5（依赖/波次）**：**valid且可并行**。02-06 wave 1（`depends_on [02-05]` 存在）→ 02-07/02-08 wave 2（均 `depends_on [02-06]`）；无环、无缺失引用、无前向引用；02-07 只碰 `tests/latency_rig.rs`，02-08 碰 state.rs/live.rs/jsonl.rs/live_session.rs——**零重叠**。（02-08 与 02-06 共享 state.rs/live.rs，但波次序贯，安全。）

**Q6（must_haves vs 生产者 / 漂移 / 上下文一致性）**：三张 must_haves 的生产者全部落在计划内且对码存在：`publish_waterfall`（02-06 Task 3）、`live::assemble`（02-06 Task 3）、`publish_with_usage`（02-08 Task 1）、`CascadeLedger`/`take_waterfalls`/`Segment.closed_at_ms`（既有码）。骨架无漂移（~15 个被引签名逐一对码）。CONTEXT/RESEARCH 的 2026-09-30 修订全部兑现（无置信度标记、弃权限无音频、火山 TTS、D-13 轴、D-18 本地 JSONL；模型选 DeepSeek-chat 与 AI-SPEC "Gemini Flash-Lite 或 DeepSeek-chat" 相容）。唯二瑕疵：02-06 truth 3 的 epoch 声明（B1 证伪）与 RESEARCH Q6 缺 RESOLVED 标记（Info 2）。

---

**建议路径**：修 B1（含重启测试）→ W1-W5 文案/清单级修正 → W6 补口径说明 → 重新过门。B1 是唯一阻挡执行的；其余可在同次修订一并清理。
