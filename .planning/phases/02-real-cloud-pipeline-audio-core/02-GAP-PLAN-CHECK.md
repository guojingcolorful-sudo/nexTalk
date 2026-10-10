---
phase: 02-real-cloud-pipeline-audio-core
check: gap-plan-check 第三轮终检（revision 2 · da215b8）
status: concerns
date: 2026-10-10
plans_checked: [02-06, 02-07, 02-08]
blockers: 0
warnings: 4
info: 4
---

# Phase 2 Gap-Plan Check — 第三轮终检（02-06 / 02-07 / 02-08）

**结论：concerns（0 blocker / 4 warning / 4 info）。** revision 2（da215b8）把 B1 主体修正确：02-06 的 drive_live 四参分离、live_generation 全链、lib.rs 四参调用、Test 8 绊线均落地且自洽；W5 生效；W6 重复开头已删；PIPESTATUS[0] 已加。四项残留仍在，全部为编译期或测试断言可强制暴露的显性问题（无静默交付风险）：**W2 只改了任务列、未改 frontmatter**；**W3 只改了构造行、未改 L280 签名（dyn 不兼容依旧）**；**02-07 的 drive_live 摘录/调用未同步四参**；**run→drive_live 的 current_epoch Arc 获取途径仍未登记**。本文件由终检写入：未修改任何 PLAN.md、未提交。

## 逐项裁决

| # | 项目 | 裁决 | 关键证据 |
|---|------|------|----------|
| 1a | drive_live 四参（generation, epoch, current_epoch）+ generation 走打点链 | ✅ | 02-06 L250 签名 + 语义注（"generation=链代次（TTS 块盖章用），epoch=进入时的 session_epoch 基线（仅作取消比较）"）；L251 `next_script(generation,…)`、`stt.start(generation)`；L254 仅 `current_epoch.load() != epoch` 作取消。既链 next_script→script.epoch→speak_segment→playout.play 未动，注释值即打点值——无混用 |
| 1b | live_generation 字段 + 访问器 + 存值 | ✅ | L315 SessionStateInner 增 `live_generation: u64`（默认 0）；L316 `chain.begin_session()` 返回值存入；L319 `pub fn live_generation(&self) -> u64` |
| 1c | lib.rs 四参调用 | ✅ | L324：`let epoch = state.start_live_session(kit.chain.clone())?; let generation = state.live_generation(); spawn(kit.run(state.clone(), epoch, generation, Some(app_root)))` |
| 1d | 02-07 同步 | ⚠ | run 摘录已四参（L70 ✓）、rig 文案已写"链代次传入 drive_live 的 generation 参数；current_epoch 种子=进入时 session_epoch"（L123 ✓）；**但 L76 drive_live 摘录仍三参 `(feed, epoch, current_epoch)`、L123 调用串仍 `Cascade::drive_live(feed, epoch, current_epoch)`——与 02-06 L250 四参定义不一致**。读法 A（照 02-06）：自洽；读法 B（照 02-07 字面）：三参调用不编译；若把链代次塞入 epoch 槽，取消对照变 会话epoch(种子)≠链代次 → 首轮即退/假阴性（B1 原始失败形态镜像）。跨计划两读法未能同时成立 |
| 1e | run→drive_live 的 current_epoch Arc 获取途径 | ⚠ | L313 run 只用 `state.session_epoch()`（值比较）；L319 访问器清单无会话 epoch Arc 读取器（round-2 修复项 c 未落实）。最小修法：登记 `session_epoch_handle() -> Arc<AtomicU64>`（或等效） |
| 1f | Test 8 绊线 | ✅ | L307 断言拓扑未变：`chain.queue().stats().accepted_chunks`（PlayoutStats）≥ 1、PlaybackFirstSample 存在、stale_chunks == 0、"不得改动"（da215b8 仅补字段出处/绊线注记） |
| 2 | W2 | ⚠ | Task 1 `<files>` 已含 tests/stability.rs + tests/voice_resolution.rs（L237 ✓）；**frontmatter files_modified（L7-16）da215b8 未动**——仍 9 项，缺 tests/cascade_integration.rs、tests/stability.rs、tests/voice_resolution.rs、tests/audio_devices.rs（与任务并集不符） |
| 3 | W3 | ⚠ | L281 构造行已修：`let on_error: Box<dyn FnMut(cpal::Error) + Send + 'static> = Box::new(move |e| sender.report(DeviceFault::from_error(&e)));` ✓（`&e` 匹配 device.rs:200 `from_error(&cpal::Error)`）；**L280 签名仍未修**：`on_error: impl Fn(StreamError) + Send + 'static`——impl Trait 参数致 `StreamFactory` 失 dyn 兼容（E0038；现状 `Arc<dyn StreamFactory>` device.rs:476/486/495、`&dyn` routing.rs:249/316/338）；`StreamError` 全仓零出现（src/ grep 空）；与 L281 的 Box<dyn FnMut(cpal::Error)> 不自洽（FnMut≠Fn）。默认委托 ✓（L280 后文）、Test 4/4b ✓（L276/277） |
| 4 | W5 | ✅ | L330 负门 `! grep -q "with the real device attached"`（现行 mod.rs L9 行内实存、count=1——非空门）；正门 `grep -q "02-06 (gap closure) attached"`（L326 指令写出的字面串） |
| 5 | W6 | ✅* | 重复开头已删：全文仅一处 "Gap 2 闭合（SC2 BLOCKER"（L35）；⚠ 残留 "。：让 live 延迟测量可达。" 残句（cosmetic） |
| 6a | PIPESTATUS[0] | ✅* | L131 `[ "${PIPESTATUS[0]}" -ne "0" ]` 已在；⚠ grep 腿仍未被断言（点名缺失 key 不进退出码；且不可在 `&&` 后直接追加 `[ ${PIPESTATUS[1]} ]`——读到的是被 `[` 刷新的值，需先缓存） |
| 6b | 行为计数 | ⚠ | Task 3 "八条"=8 条 ✓（L300-307）；Task 1 `<done>` 写"八条"但列表 **7** 条（L239-245：Test 1/2/2b/3/4/5/6） |
| 6c | 命名统一 | ✅* | L30/L302/L324 已统一 `session_backend`；残留：02-06 L366 仍写 `resolve_backend`（全仓唯一一处）；L30 列 live.rs 导出 vs L324 定义 lib.rs 私有（"命名与 live.rs 导出一致"）——位置口径需执行者统一 |

\* ✅* = 主目标达成，附一条 info 级残留。

## 剩余项（建议一次微修；均为显性问题）

1. [W2'] 02-06 frontmatter files_modified 补齐并集：+ tests/cascade_integration.rs、tests/stability.rs、tests/voice_resolution.rs、tests/audio_devices.rs。
2. [W3'] 02-06 L280 参数改 `on_error: Box<dyn FnMut(cpal::Error) + Send + 'static>`（删 `StreamError`），与 L281 一致——否则 StreamFactory 失去 dyn 兼容（E0038）。
3. [02-07 同步] L76 摘录改四参 `(…, generation: u64, epoch: u64, current_epoch: &Arc<AtomicU64>)`；L123 调用串改 `drive_live(feed, generation, epoch, current_epoch)`。
4. [Arc 途径] 02-06 L319 登记 current_epoch Arc 的生产获取途径（如 `session_epoch_handle() -> Arc<AtomicU64>`），或改传 `&SessionState`。
5. [info] 02-06 Task 1 `<done>`：八条→七条（或补一条缺失 behavior）。
6. [info] 02-06 L366：`resolve_backend` → `session_backend`。
7. [info] 02-07 L35：清理 "。：让 live 延迟测量可达。" 残句。
8. [info] 02-07 verify：如需双保险，先缓存 PIPESTATUS 再同时断言 `[0] -ne 0` 与 `[1] = 0`。

## 复核面（无新增问题）

- 任务结构：7 个任务保持 files/action/verify/done + `<automated>`（无 MISSING、无 watch、无延迟回归）；02-08 本轮未改动。
- 依赖/波次未动：02-06(w1) → 02-07/02-08(w2)；无环、无前向引用。
- 跨计划数据契约（02-08 只读账本/事件）、上下文合规（D-13/火山主供应商/无置信度标记）保持；无范围缩减话术；B1 修复未引入新症状。

**建议：** 按上述 4 项微修后执行；如需复核，仅需看 02-06 L7-16 / L280 / L319 与 02-07 L76 / L123 五处 diff。
