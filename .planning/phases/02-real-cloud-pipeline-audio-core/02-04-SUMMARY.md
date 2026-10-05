---
phase: 02-real-cloud-pipeline-audio-core
plan: 04
subsystem: audio (voice clone enrollment: capture → training → profile → fallback → preview)
tags: [cpal, rubato, hound, resample, voice-clone, volc-icl-2.0, cross-lingual-probe, enrollment-wizard, preset-fallback, vitest, playwright]

# Dependency graph
requires:
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 02
    provides: 火山 TTS 客户端（VoiceRef 分派、seed-icl-2.0 资源头、explicit_language+tone_fidelity 参数已在 volc_tts.rs 里待裁决）+ StageError 分类与凭据纪律
  - phase: 02-real-cloud-pipeline-audio-core
    plan: 03
    provides: cascade 装配（每片段音色解析的挂载点）、pipeline/vad.rs 本地能量 VAD（静音占比复用）、失败案例库 schema 与 CI 车道
provides:
  - "跨语种克隆裁决：克隆音色说英文成立（三份对照音频工件 + 用户人耳 approved）→ D-11 维持，GOV-16 备用供应商不触发"
  - "真实麦克风采集（enroll/capture.rs）：cpal + 有界队列（溢出计数）+ drain 线程电平句柄；前 500ms 丢弃；时长/体积/静音校验带锁定中文文案"
  - "48k→16k 单声道 PCM16 WAV 导出（audio/resample.rs，rubato 5 新 API：Fft + FixedSync::Both）写应用数据目录，权限 0600"
  - "voice_clone 训练客户端（enroll/register.rs）：请求逐字段对齐探针脚本、10MB 前置拦截、WER 门 45001109 与网络/凭据错误可区分、凭据只从环境读"
  - "本地音色档案（enroll/voice_store.rs，voice/profile.json，0600，无凭据无样本）+ resolve_voice()：ready→Clone / 无档或损坏→Preset（带警告）"
  - "级联每片段音色解析（cascade.rs VoiceSource 闭包）：训练完成后即时生效，进程级永不缓存"
  - "四步注册向导接真实后端（VoiceEnrollmentPage）+ preview_voice(zh|en) 真实合成与本地回放 + 重训（复用已存样本、失败保留旧音色）+ 删除档案与样本（两击确认）"
  - "e2e/enrollment.spec.ts：mock 后端走完 采集→训练→试听→重训 并断言命令参数"
affects: [02-05 (播放链替换 play_pcm_blocking；resample.rs 被流式链复用), Phase 3 (设备路由), Phase 5 (策略卡读取音色状态)]

# Tech tracking
tech-stack:
  added: [cpal 0.18.2, hound 3.5.1, rubato 5.0.1]
  patterns:
    - "实时回调纪律扩展（T-02-19）：回调只 try_send + 溢出计数；drain 线程是唯一消费者并独占写 LevelHandle（AtomicU32 f32 bits），UI 10Hz 轮询"
    - "语言参数化构建器：Option<&str> 语言 + same_language() 省略 explicit_language/tone_fidelity —— 试听两语种各镜像 T4.0 探针验证过的参数形状"
    - "音色解析每片段一次（VoiceSource = Arc<dyn Fn() -> VoiceRef>），训练后即时生效；徽章与解析同源（T-02-20）"
    - "生物数据边界（T-02-16）：样本与 speaker id 仅落应用数据目录（0600），显式断言不进 JSONL、进错即编译/测试失败"

key-files:
  created:
    - tools/vendor-experiments/cross-lingual-clone-probe.mjs
    - apps/desktop/src-tauri/src/enroll/capture.rs
    - apps/desktop/src-tauri/src/enroll/register.rs
    - apps/desktop/src-tauri/src/enroll/voice_store.rs
    - apps/desktop/src-tauri/src/audio/resample.rs
    - apps/desktop/src-tauri/tests/enrollment_capture.rs
    - apps/desktop/src-tauri/tests/enrollment_train.rs
    - apps/desktop/src-tauri/tests/voice_resolution.rs
    - e2e/enrollment.spec.ts
  modified:
    - apps/desktop/src-tauri/src/lib.rs
    - apps/desktop/src-tauri/src/pipeline/cascade.rs
    - apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs
    - apps/desktop/src-tauri/src/audio/mod.rs
    - apps/desktop/src-tauri/src/enroll/mod.rs
    - apps/desktop/src-tauri/Cargo.toml (+Cargo.lock)
    - apps/desktop/src/pages/VoiceEnrollmentPage.tsx (+test)
    - playwright.config.ts
    - e2e/desktop.spec.ts

key-decisions:
  - "跨语种克隆成立（T4.0 人耳裁决 approved，2026-10-05T13:38:11Z）：D-11 维持，GOV-16 备用供应商不触发——这是本阶段唯一未验证高风险假设的关闭方式"
  - "英文用 explicit_language=en + tone_fidelity=false；中文（同语种）省略两键（same_language()）——试听中文句与探针 clone-zh 参考完全同参"
  - "预置音色英文路径无需补参数：探针 C 份（seed-tts-2.0 预置×英文）成立，02-02 客户端内联参数即为正确形状"
  - "采集期 drain 线程常驻：回调只入队（T-02-19），电平由 drain 线程写 LevelHandle，enrollment_level 命令 10Hz 轮询"
  - "重训复用已存样本（不重录）；失败保留旧音色（previous 锚点），绝不把用户置于「既无旧也无新」状态"
  - "删除 = 档案 + 全部样本一次性清除（UI 两击确认），录音进行中拒绝（busy）——避免与仍在写入的采集竞态"
  - "试听不缓存：每次点击重新合成，重训后立即可听新音色（e2e/vitest 双层断言）"

requirements-completed: [AUDI-05, GOV-16]

# Metrics
duration: 128min
completed: 2026-10-05
---

# Phase 2 Plan 04: 克隆注册 Summary

**跨语种克隆经人耳裁决成立（三份对照音频工件，verdict approved）：cpal 真实采集 48k→16k WAV → voice_clone 训练 → 本地音色档案 → 预置回退 → 试听/重训 全链落地，主供应商 TTS 的最后一个高风险假设被钉死**

## Performance

- **Duration:** ~2h08m (128 min)
- **Started:** 2026-10-05T13:33:42Z（T4.0a 探针提交；研究探针本身更早）
- **Completed:** 2026-10-05T15:41Z（收尾验证）
- **Tasks:** 7/7（T4.0a 探针、T4.0b 人耳裁决、T3 依赖合法性门、T4.1–T4.4）
- **Files modified:** 24 tracked files（+ 两轮共 6 份 gitignored 探针音频工件）

## Accomplishments

- **主供应商风险关闭**：`cross-lingual-clone-probe.mjs`（441 行，零依赖）产出三份对照（clone-en / clone-zh / preset-en，两轮），`blind-clone-results.json` 记录 `cross_lingual.verdict: "approved"`（用户人耳判定）——「ICL 2.0 仅支持同语种」的 .env.example 矛盾被证伪，D-11 维持；失败才触发的 GOV-16 备用供应商路径保持未激活
- **真实采集链**：cpal 采集（回调只入队 + 溢出计数，drain 线程电平句柄）→ 前 500ms 丢弃 → 时长/体积（≤10MB）/静音占比校验（锁定中文文案）→ rubato 5 重采样 48k→16k 单声道 → hound 写 0600 WAV 到应用数据目录
- **训练与档案**：voice_clone 请求逐字段对齐探针脚本；10MB 前置拦截；WER 门 45001109 → `TranscriptMismatch`（「录音与文本不匹配，请重录」）与网络/凭据错误可区分；音色档案只存 speakerId 与元数据（无凭据无样本）
- **开箱可用**（ROADMAP 成功标准 3 后半）：`resolve_voice()` 无档/坏档一律回退预置（带可读警告，不 panic 不阻塞），级联每片段解析一次，未注册用户可完整跑一轮对话（mock 阶段集成测试钉住）
- **四步注册向导**：采集（真实电平表 + 计时 + 重录）→ 训练（进度/失败分支）→ 试听（中文/英文各一句，真实合成 + 本地回放）→ 重训（复用样本）/删除（两击确认）；徽章「当前音色：预置/我的克隆」与 `resolve_voice()` 同源（T-02-20）
- **全量验证绿**：cargo 290 passed / 0 failed / 2 ignored（真机麦克风与 live 延迟测量为手动 `#[ignore]` 项）；vitest 63/63（10 文件）；Playwright 36 passed / 4 skipped；隐私 grep（`enroll|profile.json` 在 `src/trace/`）零命中

## Task Commits

Each task was committed atomically (RED→GREEN pairs; TDD tasks):

1. **Task 1（T4.0a）: 跨语种克隆探针** - `2b05da4` (feat: 探针 + 三份工件 + blind-clone-results.cross_lingual)
2. **Task 2（T4.0b）: 人耳裁决（checkpoint:human-verify，用户判定）** - `58949d0` (docs: verdict approved), `043cba5` (docs: 回填失败案例 0017 措辞)
3. **Task 3: 依赖合法性门禁（checkpoint:human-verify，用户批准）** - `3207c41` (chore: cpal 0.18.2 / hound 3.5.1 / rubato 5.0.1)
4. **Task 4（T4.1）: 真实录音采集与校验** - `041efd0` (test RED), `3526237` (feat GREEN)
5. **Task 5（T4.2）: voice_clone 训练与本地音色档案** - `7944b6e` (test RED), `366feee` (feat GREEN)
6. **Task 6（T4.3）: 预置音色回退与级联音色选择** - `14a271d` (test RED, Rust), `1f0189c` (test RED, UI), `5aff2b9` (feat GREEN)
7. **Task 7（T4.4）: 克隆试听与重训** - `adf3f19` (test RED), `6b8ded2` (feat GREEN), `500a99c` (test: 重训 mock 序列修正), `a7ab648` (test: 淘汰旧注册 e2e 规格)

**Plan metadata:** final docs commit（本 SUMMARY + STATE.md + ROADMAP.md）

_Note: T4.3/T4.4 均为多条提交的 TDD 对（Rust 与前端拆开 RED；RED 后测试自身修正单独提交）_

## Files Created/Modified

- `tools/vendor-experiments/cross-lingual-clone-probe.mjs` - 跨语种可行性探针（三份对照 + 结论 JSON；退出码 0/2/3 区分产物/能力/凭据）
- `tools/vendor-experiments/blind-clone-results.json` - 新增 `cross_lingual` 段（artifacts + verdict approved + verdict_at）
- `tools/vendor-experiments/failure-cases/0017-*.json` - 占位措辞回填为真实结论
- `apps/desktop/src-tauri/src/enroll/capture.rs` - CaptureBackend/CpalCapture/ScriptedCapture + 有界队列 + drain 线程 + CaptureGuard 四项校验 + 0600 WAV 落地
- `apps/desktop/src-tauri/src/enroll/register.rs` - voice_clone 训练调用 + CloneTrainError 分类（WER 门 / 可重试 / 凭据）
- `apps/desktop/src-tauri/src/enroll/voice_store.rs` - VoiceProfile/VoiceStore/resolve_voice/voice_resolver（每片段解析）
- `apps/desktop/src-tauri/src/audio/resample.rs` - `downsample_to_16k_mono`（rubato 5，整数比路径单测）
- `apps/desktop/src-tauri/src/audio/mod.rs` - `play_pcm_blocking` 最小回放助手（注释：02-05 完整播放链替换）
- `apps/desktop/src-tauri/src/pipeline/cascade.rs` - `VoiceSource`：每片段开始时解析音色
- `apps/desktop/src-tauri/src/pipeline/stages/volc_tts.rs` - 语言参数化 + `same_language()` 构建器（计划外文件，见偏差 2）
- `apps/desktop/src-tauri/src/lib.rs` - 7 个新命令：start/stop_enrollment_recording、enrollment_level、train_voice_clone、get_voice_profile、delete_voice_profile、preview_voice
- `apps/desktop/src-tauri/tests/{enrollment_capture,enrollment_train,voice_resolution}.rs` - 16/14/6 条集成测试
- `apps/desktop/src/pages/VoiceEnrollmentPage.tsx` (+test) - 四步向导接真实后端 + 试听/重训/删除
- `e2e/enrollment.spec.ts` - 自包含 Tauri mock 的四步 e2e（断言命令与参数）
- `playwright.config.ts` - teleprompter 项目 testIgnore 增加 enrollment.spec.ts
- `e2e/desktop.spec.ts` - 删除已淘汰的旧三步注册块
- `apps/desktop/src-tauri/Cargo.toml` (+lock) - 三个门禁批准的音频依赖

## Decisions Made

- 跨语种克隆成立 → D-11 维持；GOV-16（备用供应商）不被触发。裁决依据：三份对照音频 + 用户人耳判定；工件两轮时间戳（`2026-10-05T13-33-04` / `13-33-14` UTC）
- 试听双语种参数形状：英文 `explicit_language=en + tone_fidelity=false`；中文 `same_language()`（省略两键）——与探针 clone-zh 的 `params: {}` 一致
- 预置音色英文路径以探针 C 份为准：C 份成功 → 02-02 客户端内联参数即是正确实现，无需为预置路径补参数
- 采集电平与 UI 轮询解耦：drain 线程独占写 LevelHandle，命令层只读快照（T-02-19 纪律的延续）
- 重训从档案的 `samplePath` 读样本（不要求用户重录）；重训失败保留旧 speaker（`previous` 锚点只增）
- 删除是「档案 + 全部样本」的清除动作（含样本文件名列表返回）；录音进行中拒绝，避免竞态
- 试听不做任何缓存；每次点击触发完整合成（重训后立即反映新音色的可测保证）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] 采集队列在真实时长下必然溢出（T4.1 缺陷，T4.3 发现并修复）**
- **Found during:** Task 6（T4.3 前端接线时需要真实电平）
- **Issue:** T4.1 的 `CaptureSession` 只在 stop 时消费队列；真实 60–180 秒录音远超市面上 ~2.6 秒的队列容量（`CAPTURE_QUEUE_BLOCKS = 256`），录到中途即溢出丢音频，电平表也只在停止后更新一次
- **Fix:** 采集期常驻 drain 线程连续消费并把最新峰值写进 `LevelHandle`；新增 `enrollment_level` 命令（前端 10Hz 轮询）
- **Files modified:** `enroll/capture.rs`, `src/lib.rs`, `tests/enrollment_capture.rs`
- **Verification:** enrollment_capture 16 条（含溢出计数、电平、drain 纪律断言）+ 前端电平表轮询测试
- **Committed in:** `5aff2b9`（T4.3 GREEN）

**2. [Rule 2 - Missing critical functionality] 试听中文需要「同语种」参数路径（计划外文件 volc_tts.rs）**
- **Found during:** Task 7（T4.4）
- **Issue:** 计划要求用当前（克隆）音色合成中文试听句，但 `volc_tts.rs` 恒发 `explicit_language=en + tone_fidelity=false`——中文句会走跨语种参数，与 T4.0 探针验证过的 clone-zh 参考（默认参数）不一致
- **Fix:** 请求体构造参数化（`Option<&str>` 语言）+ `same_language()` 构建器省略两键；新增单测钉住两种 body 形状
- **Files modified:** `pipeline/stages/volc_tts.rs`
- **Verification:** volc_tts 15 条单测绿（含新增 `the_same_language_body_leaves_the_cross_lingual_keys_out`）
- **Committed in:** `6b8ded2`（T4.4 GREEN）

**3. [Rule 1 - Bug] e2e mock 非自包含 → 页面内 invoke 全部 reject（徽章永不出现在测试里）**
- **Found during:** Task 7（T4.4 e2e）
- **Issue:** `installTauriMock` 引用了外层作用域常量；`page.addInitScript(fn)` 会把函数序列化进页面，外层引用在页面里是 undefined，`invoke` 全部 reject 且被页面 `.catch` 吞掉——症状是徽章一直停在空态（用一次性调试 spec 定位，调试文件已删）
- **Fix:** 数据与 helper 全部内联进 `installTauriMock`（对齐 `demo.spec.ts` 已记录的「必须自包含」约定）
- **Files modified:** `e2e/enrollment.spec.ts`
- **Committed in:** `6b8ded2`

**4. [Rule 1 - Bug] 淘汰的旧注册 e2e 规格仍在跑并失败（T4.3/T4.4 的直接余波）**
- **Found during:** Task 7 的全量 Playwright 运行
- **Issue:** `desktop.spec.ts` 的 voice-enrollment 块（2 条用例 + `denyMicrophone`/`grantMicrophone` 助手）针对已被重写的三步 getUserMedia 页面，必然失败
- **Fix:** 删除旧块与助手，留注释指向 `e2e/enrollment.spec.ts`
- **Committed in:** `a7ab648`

**5. [Rule 3 - Blocking] teleprompter Playwright 项目会把 enrollment.spec.ts 跑到 H5 源上**
- **Found during:** Task 7（T4.4）
- **Issue:** teleprompter project 只 testIgnore 了 desktop/demo；新 spec 需要 1420 源 + Tauri IPC mock，跑到 8791 源会测错应用
- **Fix:** `testIgnore` 增加 `'**/enrollment.spec.ts'`
- **Committed in:** `6b8ded2`

---

**Total deviations:** 5 auto-fixed（3 Rule 1 bug + 1 Rule 2 正确性 + 1 Rule 3 blocking；无 scope creep，无架构变更）
**Impact on plan:** 全部必要——#1 不修则 1–3 分钟真实录音在架构上不可能成功；#2 不修则中文试听走错参数路径；#3–#5 是 e2e 证据链的一次性修复

### 计划文本与现实的其他偏差（非 auto-fix，记录）

1. **测试命名重写（T4.3）**：计划的 verify `cargo test resolve_voice` 在最初命名下命中 0 条测试（表面绿、实际什么都没跑）。`tests/voice_resolution.rs` 六条测试重命名为 `resolve_voice_*`，过滤器现在真实命中断言（6 passed）。同类：T4.2 的 `enroll::register` / `voice_store` 过滤器只命中 lib 单测（各 6 条），集成层副本住在 `tests/enrollment_train.rs`（14 条）需全量套件覆盖——全量已绿
2. **失败案例编号**：计划原定新案例 `0003-cross-lingual-clone.json`，但 0003 已被 02-03 占用（numeric-drift-wrong-digits）。探针退出码 0（能力成立）故**未**新增案例文件——改为回填既有 0017 占位措辞（`043cba5`）。若未来失败需新增，下一个可用编号是 0021
3. **测试自身缺陷修正**：T4.4 RED 阶段发现重训 mock 在第一次训练就翻转 speaker（是测试错、不是实现错）——用 `attempts` 计数器修正测试（`500a99c`）
4. **计划外文件（T4.4）**：`volc_tts.rs`（见 auto-fix #2）、`playwright.config.ts`（见 auto-fix #5）、`e2e/desktop.spec.ts`（见 auto-fix #4）
5. **user_setup 修正**：计划列了 `VOLC_CLONE_APP_ID`，但实现（与探针脚本一致）只读 `VOLC_CLONE_ACCESS_TOKEN`（回退 `VOLC_TTS_ACCESS_TOKEN`）与 `VOLC_CLONE_SPEAKER_ID`（新 id 前缀走 `VOLC_CLONE_NEW_SPEAKER_ID`）——火山 voice_clone 走 token，不需要 app id
6. **`--no-gpg-sign` 噪声**：`7944b6e` 上用过一次该标志；仓库本就不签名（`%G?` 全为 `N`），属 no-op，无行为影响
7. **真实外呼范围（诚实记录）**：本计划唯一真 key 实跑是 **T4.0 探针**（两轮共 6 份音频工件，用户人耳判定 approved）。注册/训练/试听链路的**真人 GUI 跑**（含 fallback 人工检查「删除 voice/profile.json 后启动并跑一轮对话」）**未执行**——本机没有生成应用数据目录，executor 也无法驱动 GUI/麦克风；自动化等价物已绿（`resolve_voice_without_a_profile_returns_the_preset_and_the_fragment_still_runs`、`resolve_voice_runs_a_whole_unregistered_conversation_on_mock_stages`）。此外 `tests/enrollment_capture.rs` 的真机 cpal 测试是 `#[ignore]` 手动项，未跑

## Issues Encountered

- **T4.3 前端 RED 失败（文案逻辑）**：初始实现在任何 captureError 下都显示「重新录制」，但「录音器打不开」用例要求停留在「开始录音」。修法是给错误加 `afterStop` 维度（启动失败 afterStop:false / 停止与守卫失败 afterStop:true）——`5aff2b9` 内解决
- **E2E strict-mode**：`getByText('录音 1-3 分钟')` 同时命中 sr-only 步骤条与 h2 → 改用 `getByRole('heading', {name})` 定位四个步骤标题
- **全量 Playwright 2 条失败**：旧注册规格（见偏差 #4），删除后全量 36 passed / 4 skipped
- **既有范围外告警（未修，SCOPE BOUNDARY）**：`src/sim/source_test.rs` 的 `unused variable: zh` —— 已在 `deferred-items.md`「From 02-01」记录，非本计划造成
- **TDD 门全部合规**：每个 TDD 任务都有 `test(02-04)` RED 提交在前、`feat(02-04)` GREEN 在后；无 fail-fast 触发（RED 均因预期原因失败）

## Authentication Gates

None——供应商密钥已在 `tools/vendor-experiments/.env`（gitignored）就位，探针直接可用；两个 plan 内 checkpoint（T4.0b 人耳裁决、Task 3 依赖门禁）均为用户已裁决项（approved），非认证门

## User Setup Required

None——所有自动化套件零 key 运行；真 key 路径（探针实跑、桌面应用的注册/训练/试听）需要 `.env` 中已有的火山凭据（`VOLC_CLONE_ACCESS_TOKEN` 优先，或复用 `VOLC_TTS_ACCESS_TOKEN`）

## Known Stubs

- **无阻塞性 stub。** `audio::play_pcm_blocking` 是刻意的最小回放助手（代码注释标注「02-05 的完整播放链（抖动缓冲 + AEC 参考）会替换此助手，不要在此扩展功能」）——这是计划的显式接缝，不是缺失实现：plan 已声明 02-05 替换它
- T4.3 曾把试听步骤留为占位，T4.4 已接真实合成/回放，占位不复存在

## Next Phase Readiness

- **02-05（AEC + 设备路由）可直接开工**：`audio/resample.rs` 的 16k 重采样助手就是流式链要复用的导出接口；`play_pcm_blocking` 是其要替换的接缝；播放队列（02-03 playout.rs）是 jitter buffer 的基座
- **音色链路对级联透明**：每个片段经 `VoiceSource` 解析一次音色——02-05 的流式装配无需知道克隆/预置的差异
- **待人工检查项**（进 STATE Blockers）：桌面应用的真实注册 → 训练 → 试听全流程 + fallback 人工检查（删 `voice/profile.json` → 启动 → 一轮对话）需要有 GUI 与麦克风的一轮手动验证；自动化等价物已到位
- **03 阶段设备面**：`enrollment_level` 的 LevelHandle 模式（drain 线程独占写、命令读快照）是 BlackHole 设备热切换可参照的采集纪律

---
*Phase: 02-real-cloud-pipeline-audio-core*
*Completed: 2026-10-05*

## Self-Check: PASSED

- Summary file: FOUND (`.planning/phases/02-real-cloud-pipeline-audio-core/02-04-SUMMARY.md`)
- Task commits: FOUND all 15 (`2b05da4`, `58949d0`, `043cba5`, `3207c41`, `041efd0`, `3526237`, `7944b6e`, `366feee`, `14a271d`, `1f0189c`, `5aff2b9`, `adf3f19`, `6b8ded2`, `500a99c`, `a7ab648`)
- Key files: FOUND all 10 checked artifacts (probe script, capture/register/voice_store/resample, three integration suites, enrollment.spec.ts, VoiceEnrollmentPage.tsx)
- Verify commands: `cargo test enroll::` 18 passed / `resample::` 4 passed / `enroll::register` 6 passed / `voice_store` 6 passed / `resolve_voice` 6 passed；全量 cargo 290 passed / 0 failed / 2 ignored；vitest 63/63；Playwright 36 passed / 4 skipped
