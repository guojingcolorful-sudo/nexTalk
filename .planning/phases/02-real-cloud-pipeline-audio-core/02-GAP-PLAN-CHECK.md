---
phase: 02-real-cloud-pipeline-audio-core
check: gap-plan-check 复检（02-06 / 02-07 / 02-08，修订提交 3bec513）
status: concerns
date: 2026-10-10
plans_checked: [02-06, 02-07, 02-08]
blockers: 1
warnings: 3
info: 6
---

# Phase 2 Gap-Plan Check（复检） — 02-06 / 02-07 / 02-08

**结论：concerns**。B1 的修复**未正确完成**——修订把「链代次 / 会话 epoch」的拆分写进了 `start_live_session` 与 `run` 的签名，但 `drive_live` 的入参语义与循环取消检查仍把两个值混用一个参数（02-06 L250/L254），且 `generation` 从 `live_generation` 到入队戳的整条线**没有任何读取者**（无访问器、lib.rs 调用点缺参、02-07 的接口摘录还是旧三参签名）。按计划文本字面实现，新增的 Test 8 在两种自洽读法下**都必然失败**（要么第二轮会话驱动循环立即退出、要么 TTS 块继续因代次错配被静默拒收）——B1 原症状与 B1 修复后新症状两者都抓得住，但计划没有给出能通过它的实现。

其余：W1 / W4 / W6 / RESEARCH Q6 四项经复检**确认已正确应用**；W2、W5 **未应用**（与修订声明不符）；W3 的修复方向正确但新签名 `on_error: impl Fn(...)` 在现有 `Arc<dyn StreamFactory>` 架构下**无法编译**（rustc E0038 实测复现），且 `StreamError` 类型在 cpal 0.18.2 中不存在。

复检未修改任何 PLAN.md，未提交；本文件为唯一写入。

---

## 逐项裁决（调用方 1-8）

| # | 项目 | 裁决 | 一句话证据 |
|---|------|------|-----------|
| 1 | B1 代次拆分 + Test 8 + rig 读 `kit.chain.epoch()` | **未完成（blocker）** | `drive_live(epoch, current_epoch)` 单参数兼职「入队戳+取消基线」；`live_generation` 无读取者；lib.rs 调用 `run(state, epoch, app_root)` 对 4 参签名；02-07 L71 仍摘录三参 `run` |
| 2 | W1 End best-effort + 排空容忍 + Test 2 顺序覆盖 | **已应用** | 02-06 L252（`let _ = …End`、读到 None 即结束本片段→speak）、L240（final 先于 End 的双打）、L253（交替收敛要求） |
| 3 | W2 stability/voice_resolution 进 Task 1 files + frontmatter 一致 | **未应用** | 02-06 L237 无这两个文件；frontmatter L7-16 仍缺 `tests/audio_devices.rs`、`tests/cascade_integration.rs`（沿用旧清单） |
| 4 | W3 `on_error` 参数 + 默认实现委托 + Test 4 注入 | **部分/错误应用** | L280 有 on_error 与默认委托、L276 有 Test 4；但 `impl Fn` 参数使 trait 失去 dyn 兼容（E0038 实测），`StreamError` 在 cpal 0.18.2 不存在 |
| 5 | W4 run 两条退出路径 stop_session + Test 7 断言 | **已应用** | L313「两条退出路径都执行：capture.stop + chain.end_session + self.manager.stop_session()」；L306 Test 7 断言 manager.stop_session 被调用 |
| 6 | W5 行内子串负断言 | **未应用** | L330 仍断言跨行整句；实测 `grep -c "the jitter-buffered one the session runs on, with the real device attached"` 在现行（未修）文件上 = **0**（句子跨 L8/L9）→ 空门原样保留；`grep -q "02-06"` 未收紧 |
| 7 | W6 预算口径预声明 + 硬断言保留 | **已应用** | 02-07 L35（整句+ eos 2s 结构性超支属预期；超线是阶段发现：降 eos/提前 End/重定义跨度，不得放松断言）；L125 assert_within_budget 硬断言 |
| 8a | RESEARCH Q6 RESOLVED 标记 | **已应用** | 02-RESEARCH.md L1054：`6. **(RESOLVED → 02-03 T3.8：tools/vendor-experiments/failure-cases/ + run.mjs + CI 第四车道)**` |
| 8b | 02-07 pipefail 双保险 | **部分应用（info）** | L132 加的是 `[ "${PIPESTATUS[1]}" = "0" ]`（grep 腿，且与 `;` 之后整块的退出码等价）；建议的 `PIPESTATUS[0] -ne 0`（钉住 cargo 退出 101）仍缺 |

---

## Blocker（必须先改）

### B1-reopened. [task_correctness / key_link] B1 修复不完整且内部矛盾：代次拆分停在 run/state 层，`drive_live` 仍混用，`generation` 无入径——Test 8 在字面实现下必然失败

- Plan: `02-06` Task 1（L250、L254）、Task 3（L307 Test 8、L313 run、L316 start_live_session、L319 访问器、L324 lib.rs）；`02-07`（L71 接口摘录、L124 rig）。
- 对码事实链（本轮逐条复验，全部成立）：
  - `PlayoutQueue.epoch` 从 0 起（playout.rs:315）；`begin_session()/end_session()` = `fetch_add(1)+1`（playout.rs:595-624）→ 每次 `assemble()` 新建的链首次 `begin_session()` 后代次恒为 **1**。
  - `PlayoutChain::push` 拒绝 `epoch != queue.epoch()` 并只静默计数（playout.rs:922-950）；`impl PlayoutSink for PlayoutChain` 是 `let _ = self.push(...)`（playout.rs:1183-1187）——无音频、无上抛。
  - `SessionState.session_epoch` 从 0 起，start 与 stop **各 +1**（state.rs:383、406）；对外只有 `session_epoch() -> u64`（state.rs:369），**没有**返回 `Arc<AtomicU64>` 的访问器。
  - 入队戳的唯一来源链（代码核实）：`drive_live` 的 `epoch` 参数 → `feed.next_script(epoch,…)` 写进 `script.epoch`（cascade.rs:400-430）→ `self.speak_segment(segment, script.epoch)`（cascade.rs:670）→ `self.playout.play(epoch, …)`（cascade.rs:855）。**该参数就是打点值**，没有第二条入队路径。
- 计划的两种读法，全都不自洽：
  - **读法 A**（`epoch` = 链代次，按 02-06 L25/L313 与 02-07 L77 的 B1 声明）：循环顶部检查 `current_epoch.load() != epoch`（L254）拿**会话 epoch** 与**链代次**比。Test 8 拓扑：模拟会话 start+stop 后 `session_epoch = 3`，新链代次 = 1 → `3 != 1` → 驱动循环**第一轮即 return**，`accepted_chunks == 0` → Test 8 失败（且换成真实场景：第二次真实会话完全无音频驱动）。
  - **读法 B**（`epoch` = 会话 epoch，按 L250「反复迭代直到 epoch 被超越」与 L254 的取消逻辑）：检查正确，但入队戳 = 会话 epoch(3) ≠ 链代次(1) → **原 B1 症状原样保留**：全部 TTS 块被拒，`stale_chunks > 0`、`accepted_chunks == 0` → Test 8 失败。
  - 组合读法也不成立：若 run 让 drive_live 的内部检查永不触发（两值恒等）而靠 run 自己的循环检查 `state.session_epoch() != epoch`（L313）退出，则 `drive_live` 的「反复迭代直到 epoch 被超越」永不返回，Test 7 的「停止撕裂」也失败。
- **generation 入径缺失（三处独立缺口，互相印证）**：
  1. `start_live_session` 把 `chain.begin_session()` 的返回值存进 `live_generation`（L316），但 L315 的新字段清单只列 `session_mode`/`live_playout`，**未声明 `live_generation` 的落点**；L319 访问器清单（live_interrupt / session_mode / live_playout）**也没有它的读取器**。
  2. lib.rs 调用点 `spawn(kit.run(state.clone(), epoch, Some(app_root)))`（L324）只有 **3 个实参**，而 `run` 的签名是 4 参 `(state, epoch, generation, app_root)`（L313）——`generation` 既没被传，`epoch` 的来源（start_live_session 返回值）也没绑定。
  3. `drive_live` 需要 `&Arc<AtomicU64>`（会话句柄），但 state.rs 不暴露该 Arc，L319 也未新增访问器——生产调用点无法构造该实参。
  4. `02-07` L71 的接口摘录仍是旧签名 `run(self, state, epoch, app_root)`——与本轮 02-06 的新 4 参签名跨计划不一致；L124 的 rig 也需要额外把 `current_epoch` 初始值恰与链代次相同，否则同样第一轮即退（假失败「未检测到语音」），但这层数值未写明。
- 结论：B1 的**意图与测试都对**（Test 8 的断言拓扑正是唯一能抓 B1 的拓扑，务必保留），但计划文本**没有给出能通过 Test 8 的实现**——执行者若照抄 L250/L254 必失败，被迫中途自行改设计，这正是本门要拦截的。
- Fix（把拆分补完，四处一起改）：
  a) `drive_live` 签名显式三分：`drive_live(&mut self, feed, generation: u64, session_epoch: &Arc<AtomicU64>)`，入口捕获 `let baseline = session_epoch.load();`，循环检查 `session_epoch.load() != baseline`；`generation` 只走 `next_script/start/speak_segment→playout.play` 入队戳；`stt.start` 用哪个都行但注明。
  b) `live_generation` 落到具体结构（SessionStateInner）并加访问器（或由 `start_live_session` 一并返回 `(epoch, generation)`），lib.rs 调用点补齐 `run(state, epoch, generation, app_root)`。
  c) 为 `drive_live` 提供会话 epoch Arc 的获取途径（state 访问器或改传 `&SessionState`），并在 L319 清单登记。
  d) 同步 02-07 L71 的 `run` 接口摘录为 4 参；L124 写明 rig 的 `current_epoch` 初值（= 链代次，或其基线）以规避假阴性。

---

## Warnings（建议修；其中 W2/W5 为上一轮遗留未修，W3 为本轮补丁新引入）

### W2'-unapplied. [task_completeness] `SegmentScript.pcm16` 的编译波及面仍未进清单（上一轮 W2 未修）

- Plan: `02-06` Task 1（L237 `<files>`）、frontmatter（L7-16）。
- 事实（对码复验）：`SegmentScript` 全仓字面构造点 = `tests/stability.rs:463`（`SegmentScript { epoch, frames }`）+ `tests/voice_resolution.rs` **8 处**（L166/201/227/251/269/300/311/343）+ `tests/cascade_integration.rs` 8 处 + cascade.rs 内部 2 处。给结构体加 `pub pcm16` 字段后，未更新 `stability.rs`/`voice_resolution.rs` 则 `cargo test`（Task 3 verify 的全量）**编译失败**。
- Fix：`tests/stability.rs`、`tests/voice_resolution.rs` 加入 Task 1 `<files>` 与 frontmatter `files_modified`；frontmatter 同时与任务 files 并集对齐（补 `tests/cascade_integration.rs`、`tests/audio_devices.rs`）。

### W3'-incomplete. [task_correctness] W3 新签名在现有工厂架构下无法编译；引用了 cpal 0.18.2 不存在的类型（本轮补丁引入）

- Plan: `02-06` Task 2（L280-L281）。
- 事实：
  - `StreamFactory` 在 crate 内以 `Arc<dyn StreamFactory>` 使用（device.rs:476/486/495；lib.rs 经 `DeviceManager`）；而 L280 的新方法带 `on_error: impl Fn(...)`（泛型参数）→ trait 失去 dyn 兼容。最小复现 `rustc` 实测：`error[E0038]: the trait StreamFactory is not dyn compatible ... because method open_output_with_render has generic type parameters`。默认实现不改变 dyn 兼容性结论。
  - cpal 0.18.2（依赖已锁 0.18.2）**没有 `StreamError` 类型**：其流回调参数是 `E: FnMut(cpal::Error) + Send + 'static`（cpal-0.18.2/src/traits.rs `build_output_stream`），device.rs 现用 `|_error| {}` 无显式类型；`FaultSender::callback()`（device.rs:308）本就返回 `impl FnMut(cpal::Error) + Send + 'static`——计划却手搓 `move |e| sender.report(DeviceFault::from_error(e))`，而 `from_error` 收 `&cpal::Error`（device.rs:200），按文写也不类型合。
- 方向正确（输出故障必须有入径、Test 4 是对的测试）；Fix：参数改 `on_error: Box<dyn FnMut(cpal::Error) + Send + 'static>`（或直接取 `FaultSender`/其 `callback()` 的输出装箱），删除 `StreamError` 提法，闭包写 `&e`。

### W5'-unapplied. [verification_derivation] 文档负断言仍是空门（上一轮 W5 未修）

- Plan: `02-06` Task 3 verify（L330）。
- 实测（现行未修文件）：`grep -c "the jitter-buffered one the session runs on, with the real device attached" apps/desktop/src-tauri/src/audio/mod.rs` = **0**——该句跨 L8/L9（L8 以 "…is the" 结尾，L9 以 "//!   jitter-buffered one…" 开头），grep 按行匹配，**改与不改都 mismatch**，`! grep` 两端恒真。存在且完整的行内子句是 **"with the real device attached"**（实测 count = 1，落在 L9）。`grep -q "02-06"` 亦未收紧（任意一处 "02-06" 即过，旧的失实句若遗留、别处补 "02-06" 门照过）。
- Fix：改为 `! grep -q "with the real device attached"`，并把 `grep -q "02-06"` 换成对新句的断言（如 `grep -q "the jitter-buffered one the live session runs on (02-06"`），或对 L1/L8-9/L121-126 逐句断言。

---

## Info（可选）

1. **[task_completeness]** behavior 计数未随补丁更新：Task 1 `<done>` 说「六条 behavior」但列了 7 条（Test 2/2b 同名"每片段恰一个供应商会话"）；Task 3 `<done>` 说「七条」但列了 **8** 条（Test 8 为新增）。纯文案漂移，恰是补丁痕迹。
2. **[task_completeness]** 02-07 objective 出现重复开头：「**Gap 2 闭合（SC2 BLOCKER…）**。**W6 预算口径预声明**：…」换行后再次「**Gap 2 闭合（SC2 BLOCKER…）**：让 live 延迟测量可达。」——W6 插入留下的残句。
3. **[verification_derivation]** pipefail 半修：新增 `[ "${PIPESTATUS[1]}" = "0" ]` 断言的是 grep 腿（在 `;` 之后的整块语义里与改造前等价），并未断言 cargo 非零退出；done 里的「无凭据实跑 exit 101」仍无独立断言。建议 `[ "${PIPESTATUS[0]}" -ne 0 ] && [ "${PIPESTATUS[1]}" = "0" ]`。
4. **[task_correctness]** 02-06 L251 的类型文字未修（上一轮 Info 1）：`SttUpstream::Audio(Vec<i16>)`（traits.rs:210），LE 字节化与 40ms/1280 分帧已在 xfyun 客户端内部完成（xfyun.rs:669-697）；驱动应直接 `send(SttUpstream::Audio(script.pcm16.clone()))`。编译会强制收敛。
5. **[task_completeness]** 后端决策函数三处名称/位置/可见性不一致：frontmatter artifacts 把 `session_backend` 列为 live.rs 导出（L30）；Test 3 以 `session_backend(lookup, NEXTALK_SIM=1, has_device=false)` 调用（L302）；action 却定义为 lib.rs 私有 `resolve_backend(lookup, force_sim, has_default_input)`（L324）；返回类型 `SessionBackend` 与 state 的 `SessionMode` 命名不同。执行者需统一为单一定义（建议 live.rs `pub fn session_backend(...) -> SessionMode`，Test 3 可落 live_session.rs）。
6. **[verification_derivation]** Test 8 的 `accepted_chunks` 不在 chain 的 `JitterStats`（playout.rs:738-758 无此字段）——它是 `chain.queue().stats().accepted_chunks`（PlayoutStats，playout.rs:117）。断言口径需写明来源，材料本身齐备。

---

## 新增问题核查（补丁是否引入新问题）

- **无新增任务结构破损**：7 个任务全部保留 files/action/verify/done + `<automated>`（无 MISSING、无 watch 模式）；内容质量层面的新增矛盾集中在 B1/W3 两处（见上）。
- **无重复测试**：Task 1 的 Test 2/2b 同名但测点不同（顺序容忍 vs 单会话不重建）；02-06 Task 3 Test 8 与 02-07 Task 1 Test 5 拓扑不同；02-08 Task 2 扩展同一 `tests/live_session.rs` 属波次内增量，非复制。
- **依赖/波次未动且仍正确**：02-06(w1, dep[02-05]) → 02-07/02-08(w2, dep[02-06])；无环、无前向引用；02-07（仅 latency_rig.rs）与 02-08（state/live/jsonl/live_session）交集 ∅，可并行。
- **跨计划契约漂移 1 处**：02-07 L71 的 `run` 摘录与 02-06 L313 的新签名不一致（已计入 blocker 证据 d）。
- **上下文合规未回退**：2026-09-30 修订项（无置信度标记 / 弃权限无音频 / 火山 ICL 2.0 / D-13 计量轴 / D-18 本地 JSONL）在修订后文本中保持原样。
- **范围**：02-06 = 3 任务 / frontmatter 9 文件（现实 ~11；补 W2 后 ~13 文件、仍 3 任务）——按「5+ 任务」线判为不越界；若执行上下文吃紧，Task 2 可独立拆出（沿用上一轮判断）。

## 维度速览（未变项均复检通过）

| 维度 | 结论 |
|---|---|
| 需求覆盖 | PASS：AUDI-04/05→02-06，AUDI-06→02-07，GOV-09（D-13）→02-08（ROADMAP Phase 2 需求行 L69 含 AUDI-03..06，AUDI-03 由既有 02-01..05 承接，非本缺口集职责） |
| 任务完整性 | PASS（结构）；W2 文件清单与 W3 签名两处需修 |
| 依赖/波次 | PASS |
| 关键接线 | **B1 未闭合**（generation→入队戳无入径）；其余装配链（assemble→start_live_session→run→drive_live→speak_segment、publish_waterfall、publish_with_usage）任务齐备 |
| must_haves 推导 | 02-06 truth 3 的目标正确但当前文本无法兑现（B1）；余 PASS |
| 上下文合规 | PASS |
| 范围缩减检测 | PASS（无 v1/简化/暂不接线话术） |
| 架构层级 | PASS（Desktop Rust core 持有全部能力） |
| Nyquist | PASS（02-VALIDATION.md 存在；门禁即断言；无 watch/全量 E2E 依赖） |
| 跨计划数据契约 | PASS（02-08 只读 02-06 账本/事件） |
| CLAUDE.md 合规 | PASS |

---

**建议路径**：修 B1-reopened（a-d 四处）→ 修 W2'、W3'、W5' 三个 warning → info 可随同清理 → 重过 Revision Gate。B1-reopened 是唯一阻挡执行的；Test 8 保留不动（它就是判据）。
