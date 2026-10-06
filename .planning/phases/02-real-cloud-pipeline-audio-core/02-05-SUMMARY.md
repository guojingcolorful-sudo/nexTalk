---
phase: 02-real-cloud-pipeline-audio-core
plan: 05
subsystem: audio core (AEC3 preprocessing → live capture → jitter-buffered playout → device rebuild → role routing)
tags: [webrtc-audio-processing, aec3, cpal-0.18, rubato-5, jitter-buffer, device-hotswap, stream-roles, loopback, blackhole, meson, ninja, tdd]

# Dependency graph
requires:
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 03
    provides: epoch-guarded PlayoutQueue（barge-in 与硬上限的既有实现，T5.3 在其上扩展而非替换）+ pipeline/vad.rs 本地能量 VAD（AEC 块刻意不含 VAD）+ 失败案例库与 CI 五车道
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 04
    provides: rubato 5 的既有用法与 48k→16k 助手、cpal 0.18 真实采集经验（description() 而非 to_string()）、有界队列 + 溢出计数的回调纪律
provides:
  - "AEC3 + NS + AGC 采集前处理（audio/aec.rs）：10ms/480 帧纪律、渲染先于采集、无 VAD、SharedProcessor 一处理器两链路"
  - "实时采集链（audio/capture.rs）：cpal 回调 → 有界队列 → FrameAssembler → AEC → 流式重采样 → 16k PCM16 STT；回调只入队"
  - "抖动缓冲播放链（audio/playout.rs）：预滚 120ms / 硬上限 200ms / 低水位 60ms / 欠载淡出计数 + AEC 参考取自「正在播放的」缓冲（T-02-25）"
  - "错误驱动设备重建（audio/device.rs）：ErrorKind→DeviceFault 分类 + 退避 250ms/上限 3 次/冷却 5s + 每次开流带 5s 超时；重建期间静音且句子不丢"
  - "双流路由与 Phase 3 接缝（audio/routing.rs）：StreamRole 三分 + RoutingProfile（默认三分皆空）+ RoutingPlan::resolve/status/open；回采默认不启用"
  - "lib.rs audio_device_status / audio_routing_status（只读可见性命令，供 Phase 3 设置页消费）"
  - "CI：三条编译车道装 meson+ninja 并把 pip --user 的 bin 追加进 $GITHUB_PATH；本地 test:full / test:rig 自解析同一路径"
affects: [Phase 3（BlackHole 引导安装 + 设备选择器 + 回采 STT 副线；路由结构无需改动）, Phase 5（策略卡读取设备/回采状态）, 任何后续真机延迟测量（rig 与真机探针已就位）]

# Tech tracking
tech-stack:
  added: [webrtc-audio-processing 2.0.4 (~2.0), 构建期 meson 1.11.2 + ninja 1.13.2（PyPI，用户级）]
  patterns:
    - "实时回调纪律单一实现（audio/bounded.rs）：回调只 try_send + 溢出计数；采集与注册两条链共用一份，禁止复制"
    - "一处理器两链路（SharedProcessor）：渲染帧来自播放链、采集帧来自采集链，跨线程；两个处理器=两个延迟估计=不消回声"
    - "帧长校验做在入口：479 或 960 样本得到 AecError::BadFrameLength（含期望/实际），不让 crate 的 assert_eq! 在 cpal 回调里终止进程（T-02-23）"
    - "错误驱动胜过回调（cpal 0.18 无热插拔通知）：ErrorKind→DeviceFault 的处置为白名单，NotAvailable/Invalidated/Busy 才重建，Changed 明确「无需重建」"
    - "设备身份用 Arc::ptr_eq（强断言）而非 PartialEq（只是 audio_device_id）：macOS 拒绝二次打开同一设备，一次枚举同时解析两个方向"
    - "只读边界显式化：路由层只 enumerate + 按名匹配 + default_device；组合设备与系统音频设置属 Phase 3，代码检查固化（routing.rs 源码扫描）"
    - "隐私默认值即安全默认值：RoutingProfile 默认三字段皆空 → 回采不存在；坏配置文件回落默认（也是回采关闭）"

key-files:
  created:
    - apps/desktop/src-tauri/src/audio/aec.rs
    - apps/desktop/src-tauri/src/audio/bounded.rs
    - apps/desktop/src-tauri/src/audio/capture.rs
    - apps/desktop/src-tauri/src/audio/device.rs
    - apps/desktop/src-tauri/src/audio/routing.rs
    - apps/desktop/src-tauri/tests/audio_chain.rs
    - apps/desktop/src-tauri/tests/audio_devices.rs
  modified:
    - apps/desktop/src-tauri/src/audio/playout.rs
    - apps/desktop/src-tauri/src/audio/resample.rs
    - apps/desktop/src-tauri/src/audio/mod.rs
    - apps/desktop/src-tauri/src/lib.rs
    - apps/desktop/src-tauri/src/enroll/capture.rs
    - apps/desktop/src-tauri/src/pipeline/cascade.rs
    - apps/desktop/src-tauri/Cargo.toml (+Cargo.lock)
    - .github/workflows/ci.yml
    - package.json
    - README.md

key-decisions:
  - "AEC 不降级：bundled 构建真的成功（9m23s 冷编译，缓存后秒级），pkg-config 不需要——Task 0 的 descope-aec 分支未被触发，自听回环风险按设计消除"
  - "帧长校验前置到封装层：把 panic 变成带期望/实际的结构化错误，因为 webrtc 的 assert_eq! 在实时线程里等于会话终止"
  - "渲染帧必须先于采集帧（AEC3 的延迟估计需要「麦克风即将听到的东西」的远端参考）；违规返回 RenderOrderViolated，采集侧在无播放链时喂静音帧而不是停摆"
  - "T5.3 扩展 02-03 的 PlayoutQueue 而非替换：硬上限靠队列自身的背压实现，「丢最旧并计数」只有一个实现"
  - "AEC 参考 = 设备真正消费的那个缓冲（转换一次进图，不复制、不用「收到的」数据）；中断时镜像同步停止（T-02-25）"
  - "设备重建的处置白名单：Only NotAvailable/Invalidated/Busy 触发；Changed 按 cpal 语义「流仍活、无需重建」，Unexpected 不做未分类拆除"
  - "连续失败上限的跨越点仍返回 Failed（本次尝试确实发生了），Exhausted 只留给停摆期间的拒绝——否则一千次故障看起来像一千次重建，上限不可观测（T-02-26）"
  - "DeviceManager 无内部锁：所有方法 &mut self，回调侧只有 FaultSender（一次 channel send），锁/中毒/临时_guard 死锁整类问题消失"
  - "回采是 opt-in 且必须按名指定：无设备名 → 该角色不存在（role_disabled），绝不隐式选默认输入设备——把麦克风当回采用会把用户自己的声音送进面试官线（T-02-24）"
  - "按名匹配（先精确、再 trim+忽略大小写）而非能力标志：macOS 虚拟驱动两个方向都报 true 且自称麦克风，能力标志是错的问题（T-02-22）"
  - "路由档案可持久化到应用数据目录 routing.json；load 永不失败（缺失/损坏都回落默认，而默认是回采关闭）"

requirements-completed: [AUDI-06]

# Metrics
duration: 107min
completed: 2026-10-06
---

# Phase 2 Plan 05: 真实音频内核 Summary

**AEC3 实测消掉 11.55 dB 合成回声、采集链 48k→16k 逐样本可验、播放链 200ms 硬上限不越界、拔设备不杀会话、回采默认关闭——壳里的音频内核从「接口 + 脚本假装」换成真实链路，全量 354 passed / 0 failed**

## Performance

- **Duration:** ~1h47m（107 min，`5627b2c` T5.1 RED → `facf45e` T5.5 GREEN 的提交跨度；Task 0 工具链门在其之前）
- **Started:** 2026-10-06T12:48:33Z（T5.1 RED 提交）
- **Completed:** 2026-10-06T14:35:32Z（T5.5 GREEN），收尾 15:0x
- **Tasks:** 6/6（Task 0 为 `checkpoint:human-verify` 门 + Task 1–5 全 TDD）
- **Files modified:** 17 tracked files, +6654/-44

## Accomplishments

- **AEC 真的在消回声（T5.1）**：`webrtc-audio-processing 2.0.4` bundled 构建在本机完成（9m23s 冷、缓存后秒级），合成 100ms/0.5 增益回声路径上**回声下降 11.55 dB**、本地话者距其纯语音电平仅 2.80 dB——阈值断言进 CI，不是「应该有回声消除」
- **帧纪律不 panic（T5.1）**：479 或 960 样本帧得到 `AecError::BadFrameLength`（带期望/实际），crate 的 `assert_eq!` 永远不会在 cpal 回调里看到错尺寸帧；公开 API 无 VAD 语义（代码扫描固化）
- **采集链逐样本可验（T5.2）**：48k→16k 在 480 样本块与 137/251/480 混合切分下**逐位相同**，1 秒输入恰好 16 000 样本输出；`FrameAssembler` 跨设备块携带余数，512 帧 CoreAudio 缓冲的接缝不丢样本；回调只入队 + 溢出计数（T-02-23）
- **播放链不爆音（T5.3）**：预滚 120ms、硬上限 200ms（复用 02-03 队列背压）、低水位 60ms 在缺口发生**之前**上报、欠载把尾部斜坡到精确静音并停止重复报警、抢话保留 5ms 淡出；AEC 参考镜像的就是设备消费的那个缓冲
- **拔设备不杀会话（T5.4）**：`ErrorKind→DeviceFault` 白名单处置、退避 250ms、连续失败上限 3 次、冷却 5s、每次开流带 5s 超时；重建期间 `PlayoutChain::suspend` 发静音**且不消费缓冲**，句子在设备回来之后样本连续地继续（T-02-26）
- **双流路由就位（T5.5）**：`StreamRole::{UserMic, Loopback, Output}` 各自独立路径（`Arc::ptr_eq` 断言麦克风与回采不是同一个流）；回采默认不启用（T-02-24），按名解析 BlackHole（容忍手工输入的空格/大小写），设备不在时返回 `device_not_found` 并点名设备与角色，**绝不静默降级**；路由层只读（源码扫描禁止 capability 标志与系统写入）
- **全量验证绿**：cargo **354 passed / 0 failed / 5 ignored**；`pnpm test:rig` 冷 p50 1180ms / 热 p50 1225ms / p95 1300ms（预算 2000ms，不回退）；`grep -rn "aggregate\|CoreAudio.*set\|defaults write" src/audio/` 零命中；五车道 CI 齐备

## Task Commits

每个任务都是 RED→GREEN 原子对：

1. **Task 0（T5.0）: 工具链与合法性门（checkpoint:human-verify，用户裁决）** — 无提交（环境动作）；结论：`pip3 install --user "meson==1.11.2" "ninja==1.13.2"`，暂不装 pkg-config，不用 Homebrew。落地记录随 `6f0dcff`（README + Cargo.toml 注释）
2. **Task 1（T5.1）: AEC3/NS/AGC 处理块** — `5627b2c` (test RED), `6f0dcff` (feat GREEN)
3. **Task 2（T5.2）: 实时采集链与流式重采样** — `091e62a` (test RED), `263e348` (feat GREEN)
4. **Task 3（T5.3）: 抖动缓冲播放链与 AEC 镜像** — `b760539` (test RED), `05a7ba9` (feat GREEN)
5. **Task 4（T5.4）: 错误驱动设备热切换** — `b6bb12e` (test RED), `116252d` (feat GREEN)
6. **Task 5（T5.5）: 双流路由与只读边界** — `db42a6e` (test RED), `facf45e` (feat GREEN)
7. **收尾** — `76719c5` (docs: 修正 audio/mod.rs 中「02-05 会替换」的过时注释)

**Plan metadata:** 本 SUMMARY + STATE.md + ROADMAP.md（最终 docs 提交）

## Files Created/Modified

- `audio/aec.rs`（371 行）— `AecConfig` 四开关、`AudioProcessor`（帧长前置校验 + 渲染先于采集）、`SharedProcessor`（一处理器两链路，同时实现 `RenderReference`）、`AecError`
- `audio/bounded.rs`（99 行）— 实时回调纪律的唯一实现：`CaptureSink::bounded` + `try_send`/溢出计数 + `DEFAULT_QUEUE_BLOCKS`
- `audio/capture.rs`（809 行）— `FrameAssembler`、`BlockSource`（`CpalSource`/`ScriptedSource`）、`CaptureChain::{start,poll,drain_errors,stop}`、`CaptureOutput`（同一趟同时给出 48k 帧与 16k PCM16）、T5.5 追加 `CaptureLine` 与 `CpalSource::for_role`
- `audio/playout.rs`（+543 行）— `PlayoutChain`（预滚/硬上限/低水位/欠载淡出/AEC 镜像/suspend-resume）+ `JitterPolicy`；`RebuildGate` 实现
- `audio/device.rs`（1069 行）— `DeviceRef`/`DeviceHandle`、`DeviceFault`（`from_kind`/`needs_rebuild`/`code`/`message`）、`FaultSender`、`StreamFactory`/`OpenStream`、`DeviceManager`（`start_session`/`poll_faults`/`report_fault`/`status`）、`CpalStreamFactory`
- `audio/routing.rs`（684 行）— `StreamRole`、`RoutingProfile`（可持久化）、`ResolvedStream`/`RoleStatus`、`RoutingPlan::{resolve,device,open,status,capture_roles}`、`RoutingError`、`LOOPBACK_DEVICE_NAME`
- `audio/resample.rs`（+278 行）— `StreamingResampler`（rubato 5 `Fft` + `FixedSync::Both`，跨调用携带余数）、`to_stt()`/`from_tts()`
- `pipeline/cascade.rs`（+123 行）— `UserAudioFeed`：真实采集帧重定时进既有分段器，脚本路径降级为测试/无设备回退
- `enroll/capture.rs`（±43 行）— 改用 `audio::bounded` 的共享队列纪律（删掉本地副本）
- `lib.rs`（+120 行）— `audio_device_status`（T5.4）、`audio_routing_status`（T5.5），均为只读可见性
- `tests/audio_chain.rs`（1370 行，28 条）— AEC/重采样/采集链/播放链全部脚本后端，零真实设备
- `tests/audio_devices.rs`（967 行，16 条）— 设备故障与重建、双流路由、只读源码检查、CI 车道检查；唯一真设备用例 `#[ignore]`
- `.github/workflows/ci.yml` — 三条编译车道加 meson+ninja 安装与 `$GITHUB_PATH` 追加（含顺序原因注释）
- `package.json` / `README.md` — `test:full`/`test:rig` 自解析 pip --user 路径；README 写明 PATH 导出理由（`source ~/.cargo/env` 不管这个）

## Decisions Made

- **AEC 走通，不降级**：Task 0 的门禁结论是「装上」而非 `descope-aec`；实时回环（自听）风险按设计消除，不需要「最小语音门限是唯一防线」的 MVP 取舍
- **帧长校验在封装层前置**：错误信息含期望与实际帧长，且必须是 `Result` 而非 panic——这是 T-02-23 在音频路径上的具体形态
- **一处理器两链路**：`SharedProcessor` 由采集链与播放链共享（两个处理器 = 两个延迟估计 = 不消回声）；无播放链时喂静音帧，静音就是「没有东西在播」的正确参考
- **队列背压即硬上限**：T5.3 没有另写一套「丢最旧」，而是复用 02-03 队列的背压——「丢最旧并计数」只有一个实现
- **重建处置是白名单**：只有 NotAvailable/Invalidated/Busy 触发；`Changed` 明确不重建（cpal 说有它自己的语义），未分类的 `Unexpected` 也不拆一条明显还开着的流
- **上限跨越点仍算一次尝试**（返回 `Failed`），`Exhausted` 只描述停摆；否则「退避 + 上限」在测试里不可观测——这正是 T-02-26 要防的无限重建风暴
- **`DeviceManager` 无锁**：`&mut self` + 回调侧单次 channel send，把锁中毒/临时 guard 死锁这一整类问题从设计里去掉
- **回采必须按名显式启用**：默认 profile 三分皆空 → 回采不存在；`role_disabled` 是可读答案而不是「悄悄用默认输入设备」；坏配置文件的回落方向也是回采关闭（T-02-24）
- **按名不按能力**：macOS 虚拟驱动两向皆报 true，能力标志是错的问题；匹配容忍 trim + 忽略大小写，因为名字是人在设置框里敲的
- **路由只读边界写进代码**：组合设备、系统音频设置属 Phase 3 的自有同意流程；本计划的源码扫描测试禁止 `aggregate`/`AudioObjectSet`/`defaults write` 等字样进入 `routing.rs`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking，用户已裁决] 构建工具链缺失（T5.0 门禁）**
- **Found during:** Task 0
- **Issue:** meson/ninja/pkg-config 三者皆缺且无 Homebrew；bundled AEC3 的 C++ 需要 meson+ninja
- **Fix:** 用户批准 Option A——`pip3 install --user "meson==1.11.2" "ninja==1.13.2"`；**实测 pkg-config 不需要**（bundled 路径不查系统 abseil），故计划中的 pkgconf 源码回退未触发；安装命令 + PATH 导出写进 README、`.github/workflows/ci.yml`（三条编译车道）与 `package.json`
- **Files modified:** `README.md`, `.github/workflows/ci.yml`, `package.json`
- **Verification:** bundled 构建成功（9m23s 冷）；`cargo build --manifest-path … ` 在 `MACOSX_DEPLOYMENT_TARGET=12.0` 下通过
- **Committed in:** `6f0dcff`（文档部分），CI/脚本部分 `facf45e`

**2. [Rule 1 - Bug] 回调纪律有两份实现（T5.2 抽取 `audio/bounded.rs`）**
- **Found during:** Task 2
- **Issue:** 02-04 的 `enroll/capture.rs` 自带一份 `CaptureSink`/`CAPTURE_QUEUE_BLOCKS`，02-05 的采集链需要同一纪律——计划明写「不要复制粘贴两份」
- **Fix:** 抽出 `audio/bounded.rs` 作为唯一实现，`enroll/capture.rs` 改为复用（删除本地副本）
- **Files modified:** `audio/bounded.rs`（新）, `enroll/capture.rs`
- **Verification:** enrollment_capture 16 条与 audio_chain 全部绿，无行为回归
- **Committed in:** `263e348`

**3. [Rule 1 - Bug] 失败上限的跨越点曾返回 `Exhausted`，使上限不可观测（T5.4）**
- **Found during:** Task 4（测试 `left: 2, right: 3`）
- **Issue:** 第 3 次连续失败（即跨越上限的那次）返回 `Exhausted`，而 `Exhausted` 语义上是「停摆期间被拒绝」——按「实际发生过的尝试」计数时它被漏掉，上限在测试里永远差一次
- **Fix:** 跨越点仍返回 `Failed`（本次尝试确实发生了），同时置 `stalled` + `retry_after_ms`（冷却是真的）；`Exhausted` 只留给停摆期间的拒绝
- **Files modified:** `audio/device.rs`
- **Verification:** `every_build_carries_a_timeout_and_a_storm_of_faults_cannot_become_a_storm_of_rebuilds` 与 `a_device_that_comes_back_ends_the_stall_and_the_session_recovers` 绿
- **Committed in:** `116252d`

**4. [Rule 1 - Bug] 把「投递」误当「已处理」计数（T5.4 单测）**
- **Found during:** Task 4（`left: 1, right: 2`）
- **Issue:** `faults_seen` 只在 `report_fault` 里自增，躺在 channel 里还没被 `poll_faults` 取走的故障不该计数；初版测试把两者当同一个数
- **Fix:** 测试先手工 drain 一次（顺带断言 `stream_invalidated` 映射）再 `poll_faults()`，并写明「投递与处置是两个计数，故意不同」
- **Files modified:** `audio/device.rs`（测试）
- **Committed in:** `116252d`

**5. [Rule 1 - Bug] CI 自检源码检查把断言自己算了进去（T5.5）**
- **Found during:** Task 5（`left: 2, right: 1`）
- **Issue:** `suite.matches("CpalStreamFactory::shared()")` 的字面量同时出现在被检查文件里的断言行上，任何字面量都会匹配自身，检查永远不可能满足
- **Fix:** needle 运行期拼接（`["CpalStreamFactory", "::shared()"].concat()`），不匹配自身源码
- **Files modified:** `tests/audio_devices.rs`
- **Committed in:** `facf45e`

**6. [Rule 1 - Bug] `expect_err` 需要 `Debug`，而 `Box<dyn OpenStream>` 没有（T5.5 单测）**
- **Found during:** Task 5（编译错误 E0277）
- **Fix:** 改为 `let Err(error) = … else { panic!(…) }`——不为测试给生产类型补 `Debug` 约束
- **Files modified:** `audio/routing.rs`（测试）
- **Committed in:** `facf45e`

**7. [Rule 2 - Missing critical functionality] 路由档案的持久化（T5.5）**
- **Found during:** Task 5
- **Issue:** 计划要求 `RoutingProfile`「可持久化到本地配置」，而 `audio_routing_status` 需要读一个真实档案才有意义
- **Fix:** `serde` derive + `load_from`（**永不失败**：缺失或损坏都回落默认，而默认是回采关闭）+ `save_to` + `ROUTING_CONFIG_FILE`，并有一条往返/损坏/部分字段的单测
- **Files modified:** `audio/routing.rs`
- **Committed in:** `facf45e`

**8. [Rule 1 - Bug] RED 文件自身的编译缺陷（T5.4/T5.5，GREEN 内一并修复）**
- **Found during:** Task 4/5 的 GREEN 编译
- **Issue/Fix:** `ChainGate` 元组结构体初始化写成命名字段；`request.device.clone()`（`Option<&DeviceHandle>`）应为 `.cloned()`；两处 `let _ = manager.start_session()` 在「第一次开流就是坏的」场景下会让 `in_session == false`，使后续每个故障都返回 `Ignored`（测试再也测不到重建）→ 改为先 `.expect()` 成功开流、再 `break_device`；`tests/audio_devices.rs` 的 `RoutingError` 未使用导入；`error.role()` 断言与 `Option<StreamRole>` 签名对齐
- **Verification:** 全量套件绿
- **Committed in:** `116252d`, `facf45e`

### 计划文本与现实的其他偏差（非 auto-fix，记录）

1. **`DeviceManager` 去掉了内部 `Mutex`（设计决定）**：所有变更方法 `&mut self`，回调侧只有 `FaultSender`（一次 channel send）。计划未指定锁策略；这个选择把锁中毒、临时 guard 生命周期死锁（`if let` 临时值）整类问题从设计里去掉。`status()`/`current()` 仍是 `&self`
2. **一次枚举同时解析两个方向**：`resolve_devices()` 只 enumerate 一次，输入/输出都从同一份列表导出——这样同一台设备（如耳机）在两侧解析为**同一个 `Arc`**（`Arc::ptr_eq` 为真），而不是两个相等的副本。这是计划「同一设备必须复用同一个 Device 对象」要求的实现方式
3. **`DeviceFault::Unexpected` 不触发重建**：计划只列了「观察四种 kind 后重建」；未分类的 kind 保持流不动（结构上把 `other =>` 落在这条路上），并在代码注释里写明理由
4. **`play_pcm_blocking` 保留**：02-04 的注释写「02-05 替换此助手」，但会话路径改走 `PlayoutChain` 之后，注册试听仍只有一段 3 秒块、没有流式生产者——给它套抖动缓冲是无米之炊。已在 `audio/mod.rs` 里把注释改成真实分工（`76719c5`，纯注释）
5. **`audio_routing_status` 暂无前端消费者**：与 T5.4 的 `audio_device_status` 同规格（计划明写「可见性风格同 Task 4」）；设置页是 Phase 3 的，命令已注册待接
6. **`cargo fmt --check` 的范围**：本仓库并非全 crate 干净（见 `deferred-items.md`），所以只对本计划触碰的文件跑 `cargo fmt --check`，避免把无关文件的重排混进任务提交

## Verification Run

| 项 | 命令 | 结果 |
|---|---|---|
| 全量 cargo | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | **354 passed / 0 failed / 5 ignored**（lib 211；audio_chain 26+2i；audio_devices 15+1i；cascade_integration 8；enrollment_capture 15+1i；enrollment_train 14；latency_rig 3+1i；mock_vendors 31；session_integration 5；stability 19；voice_resolution 6；doctest 1） |
| 构建（Monterey 目标） | `MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --manifest-path …` | 成功 |
| 延迟预算 | `pnpm test:rig` | 冷 p50 **1180ms** / 热 p50 **1225ms** / p95 1300ms（预算 2000ms）——加 AEC 与抖动缓冲后不回退 |
| 任务 5 计划验证 | `cargo test routing:: && cargo test --test audio_devices routing && node -e "…五车道…" && grep -v '^#' ci.yml \| grep -q -- '--manifest-path'` | PLAN-VERIFY-OK |
| 只读核对 | `grep -rn "aggregate\|CoreAudio.*set\|defaults write" apps/desktop/src-tauri/src/audio/` | 零命中 |
| CI 车道 | 五车道齐备；三条编译车道装 meson+ninja 且把 `$(python3 -m site --user-base)/bin` 追加到 `$GITHUB_PATH`（顺序在 cargo 之前） | 测试固化（`the_ci_lanes_cover_the_new_tests_and_install_the_build_toolchain`） |

### 待人工项（本 executor 无法执行，进 STATE Blockers）

1. **真机拔插**：`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test audio_devices a_real_device_unplug_is_survived -- --ignored`——需要有人真的拔掉耳机；`#[ignore]` 原因已写明步骤（拔 → 看控制台故障码 → 插回 → 下一句从新设备播出）
2. **真机采集 + 播放 + 回声实测**：本机 GUI/麦克风回路未跑（executor 不能驱动 GUI）；自动化等价物已绿（脚本后端覆盖全链）
3. **live 延迟测量**：`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test latency_rig -- --ignored` 需要真实供应商凭据；瀑布报告属阶段门禁的人工记录工件

## Threat Model Compliance

| 威胁 | 落点 | 证据 |
|---|---|---|
| T-02-SC（构建期供应链） | PyPI 的 meson/ninja 经用户裁决安装；包版本固定为 `1.11.2`/`1.13.2` | Task 0 门禁（用户 approved）；CI 与脚本同一组固定版本 |
| T-02-22（设备名不可信） | 设备名只用于展示与按名匹配，**从不**参与能力判定、不进 shell/路径 | `routing_never_asks_a_device_what_it_can_do`（禁止 `supports_input(`/`output_devices()` 等字样，要求 `default_device` 与 `.name()`）；`device_name()` 用 `description()` 不用 `to_string()` |
| T-02-23（实时线程 DoS） | 回调只入队 + 溢出计数；帧长错误是结构化错误不是 panic；重建在 `poll_faults` 而非回调 | `audio/bounded.rs` 单一实现；`AecError::BadFrameLength`；`FaultSender` 只做一次 channel send |
| T-02-24（回采隐私） | 默认不启用；启用需显式设备名；回采只走 STT 副线 | `routing_defaults_fall_back_to_the_default_devices_and_leave_the_loopback_off`；`CaptureLine::Interviewer` 标注「不落盘、不进 JSONL、不留存」 |
| T-02-25（AEC 参考完整性） | 参考 = 设备真正消费的那个缓冲；中断时镜像同步停止 | `the_playout_mirror_is_the_buffer_the_device_consumed`（audio_chain T5.3 组） |
| T-02-26（重建风暴） | 退避 250ms + 连续失败上限 3 + 冷却 5s；重建期间静音不爆音 | `every_build_carries_a_timeout_and_a_storm_of_faults_cannot_become_a_storm_of_rebuilds`、`the_rebuild_is_silent_and_the_sentence_survives_it`、`a_device_that_comes_back_ends_the_stall_and_the_session_recovers` |

**Threat flags（计划外新增面）：无。** 本计划没有引入新的网络端点、认证路径或 schema 变更；`routing.json` 只写本机应用数据目录，内容是用户自己的设备名。

## Authentication Gates

None——本计划全部测试零 key、零网络、零真实设备依赖（唯一真设备用例是 `#[ignore]`）。Task 0 是**包合法性门禁**（`gate="blocking-human"`）而非认证门，用户已裁决。

## Known Stubs

- **无阻塞性 stub。** 逐文件扫描：无 `TODO`/`FIXME`/「coming soon」占位，无「恒空值流向渲染」。
- 两处**刻意接缝**（计划声明，不是缺失）：`play_pcm_blocking` 留给注册试听（一段 3 秒块，无流式生产者，已在代码注释写明分工）；`audio_routing_status` / `audio_device_status` 是只读可见性命令，前端设置页属 Phase 3。
- `RoleStatus.enabled = false`（回采未启用）是**有意的显式表达**，UI 文案为「回采」未启用，不是「数据缺失」。

## Issues Encountered

- **T5.1 的 AEC 断言依赖合成场景**：11.55 dB / 2.80 dB 是在 100ms 延迟、0.5 增益的合成回路上实测的，不是真机回声路径的数字；真机测量在待人工项里
- **linker 噪声（既有，非本计划造成）**：每次链接都打印三条 `ld: directory not found for option '-L…/lib/x86_64-linux-gnu'`（`webrtc-audio-processing-sys` 的 build script 在 macOS 上仍发出 Linux 库路径）。无害、不致命，已记入 `deferred-items.md`
- **既有范围外告警（未修，SCOPE BOUNDARY）**：`src/pipeline/validate.rs:461` 未使用的 `mut`、`src/sim/source_test.rs:256` 未使用的 `zh`、`clone_on_copy` 两处、`drain_collect` 一处——全部记入 `deferred-items.md`「From 02-05」
- **TDD 门全部合规**：T5.1–T5.5 每个任务都有 `test(02-05)` RED 提交在前、`feat(02-05)` GREEN 在后；RED 均因预期原因失败（T5.5 RED 的三个错误：两个 `include_str!` 读不到 `routing.rs` + `unresolved import`）；无 fail-fast 触发；RED 文件里的编译缺陷（偏差 8）在 GREEN 中修复并记录

## Next Phase Readiness

- **Phase 3 的接缝已经就位**：`RoutingProfile` 指定 `loopback_device = "BlackHole 2ch"` 即可让回采成为一条独立采集路径（`CpalSource::for_role` + `CaptureLine::Interviewer`），路由结构无需改动；`LOOPBACK_DEVICE_NAME` 常量保证设置页、引导安装与测试不会各写各的名字
- **Phase 3 要补的三件事**：BlackHole 的引导安装与缺失横幅、设备选择器（消费 `audio_device_status`/`audio_routing_status`）、回采 STT 副线的真实消费端
- **设备重建对级联透明**：`RebuildGate` 让播放链在设备消失期间静音且不消费，句子在恢复后连续播出——上层（会话/字幕）看不到这段空窗
- **延迟预算未回退**：rig 冷 1180ms / 热 1225ms，AEC + 抖动缓冲的 200ms 上限没有吃掉 <=2s 预算
- **CI 记得住工具链**：三条编译车道自带 meson+ninja 与 PATH 追加，新机/新 runner 不会重踩 T5.0 的坑

---
*Phase: 02-real-cloud-pipeline-audio-core*
*Completed: 2026-10-06*

## Self-Check: PASSED

- Summary file: FOUND (`.planning/phases/02-real-cloud-pipeline-audio-core/02-05-SUMMARY.md`)
- Task commits: FOUND all 11 (`5627b2c`, `6f0dcff`, `091e62a`, `263e348`, `b760539`, `05a7ba9`, `b6bb12e`, `116252d`, `db42a6e`, `facf45e`, `76719c5`)
- Key files: FOUND all 7 created (`audio/{aec,bounded,capture,device,routing}.rs`, `tests/{audio_chain,audio_devices}.rs`) + all 10 modified
- Verify commands: `cargo test routing::` 7 passed；`cargo test --test audio_devices` 15 passed / 1 ignored；全量 cargo 354 passed / 0 failed / 5 ignored；`pnpm test:rig` 3 passed / 1 ignored（冷 1180ms / 热 1225ms）；只读 grep 零命中
