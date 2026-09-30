---
phase: 02-real-cloud-pipeline-audio-core
status: concerns
checked_at: 2026-09-29
plans_checked: 5
checked_by: gsd-plan-checker (goal-backward pre-execution review)
concerns:
  - "C1 [blocker] 失败案例库 0003 编号/所有权跨计划冲突（02-03 ↔ 02-04），且 02-03 的 regression_test 存在性验收在该案例上自相矛盾"
  - "C2 [blocker] 02-VALIDATION.md 缺失（config nyquist_validation=true 且 RESEARCH 含 Validation Architecture；plan-phase §5.5 门禁未执行）"
  - "C3 [blocker] 02-RESEARCH.md ## Open Questions 无 (RESOLVED) 标注（维度 11 判定 FAIL；6 问中的 5 问为测量待定型，均有任务承接，属标注缺口）"
  - "C4 [warning] 阶段门禁的『真实供应商 live 延迟测量工件』无任务所有者（RESEARCH:1146 要求 /gsd:verify-work 前产出）"
  - "C5 [warning] 02-04 CaptureGuard min_secs=15 与 Test 1 期望（15s 拒绝）、中文文案（至少 1 分钟）、ROADMAP 成功标准 3（1–3 分钟）四处口径不一致"
  - "C6 [warning] 02-04 T4.0b 阻塞门使用 checkpoint:human-verify gate=blocking；end-of-phase 模式下建议改 blocking-human 或 checkpoint:decision（本项目 auto 标志为 false，当前无实际自动批准通路）"
  - "C7 [warning] AUDI 编号口径与 REQUIREMENTS.md 标签错位（能力 100% 覆盖，标签偏移；02-04/02-05 未声明口径）"
  - "C8 [warning] 02-03 规模 7 任务/31 文件（框架阈值 5+/15+）；用户已批准 5 波骨架且合并已减少任务数，判为警告并建议执行时隔离上下文"
---

# Phase 2 计划预执行检查（goal-backward）

**结论：计划可以进入执行，但 C1 必须在执行 02-03 之前修复**（跨计划文件/ID 冲突 + 一条无法同时满足的验收断言）；C2/C3 是工件合规缺口（补文件/加标注即可，不改计划本体）；C4–C6 为低成本补漏，建议随同一轮修订处理。

## 验证依据

- 5 份计划：`02-01-PLAN.md` … `02-05-PLAN.md`（逐任务读 action/verify/done/files）
- 已批准骨架：从 planner 任务提示复原（`<locked_task_skeleton>`，标题写“24 tasks”，实际枚举 4+7+9+4+5=29 项；本仓库无 02-SKELETON.md 文件）
- `02-CONTEXT.md`（D-01..D-20 锁定决策 / 自由裁量 / 延后项）、`02-RESEARCH.md`（修正项、Open Questions:1027、责任映射:91、阶段门禁:1146、包合法性审计:174）
- `ROADMAP.md`（Phase 2 目标 + 5 条成功标准）、`REQUIREMENTS.md`（AUDI-01..07 + 追溯表）、`.planning/ref/ai-governance-requirements.md`（GOV-01..23）
- framework：`checkpoints.md`、`planner-human-verify-mode.md`、`gsd-planner.md:628`、`gsd-executor.md:189/298`、`plan-phase.md`（§5.5 VALIDATION.md 门禁、§7.5 pattern mapper）、`config.json`

---

## Q1. 五条成功标准 → 任务覆盖（逐条追踪）

| # | 成功标准 | 承接任务 | 判定 |
|---|---------|---------|------|
| 1 | 中文入麦 → ≤2s（含冷启动）→ 耳机出克隆音色英文 + 双端双语字幕 | 02-01（预算/停表）、02-02（四供应商客户端全部落地）、02-03（提交门 + 渲染 + D-07 协议双端）、02-04（克隆音色）、02-05（真实设备采集/播放链）；**live 冷启动实测无任务所有者 → C4** | 覆盖（验证工件有缺口） |
| 2 | 瀑布报告 e2e 数字 + 超支可归因到阶段 | 02-01 Task1（恰好 5 个流式边界 + `assert_within_budget` + Test2 重叠证明 + Test3 2223ms 朴素和 ≤2000ms e2e）、02-02 各客户端首标记埋点（Deepgram 例外已显式记录）、02-03 Task5（逐段用量/成本进 JSONL + 成本面板） | 覆盖 |
| 3 | 1–3 分钟录音注册克隆；未注册可用库存音色 | 02-04 Task4（真实采集+校验）、Task5（voice_clone + 存储）、Task6（预置回退 per-segment）、Task7（试听/重训 + enrollment e2e）；**校验阈值口径冲突 → C5** | 覆盖（阈值口径需修复） |
| 4 | 中途重说 ~100ms 停止、无重叠；拔插不杀会话 | 02-03 Task2（epoch 先自增后清队列 + 淡出 + 最小语音门限）、02-05 Task4（错误驱动重建 + 同 Device 复用 + 超时） | 覆盖 |
| 5 | “spoken English ⊆ committed finals”（测试语料） | 02-03 Task1（提交门 + poison-partial 签名测试 = GOV-15 零违例 + eos/静默/`vad_events` 三种分段触发） | 覆盖 |

补充：ROADMAP `Requirements: AUDI-03, AUDI-04, AUDI-05, AUDI-06` 的四个 ID 全部出现在计划 `requirements` 字段并各有承接任务（见 C7 的标签错位说明——能力无遗漏，仅标签偏移）。

## Q2. 波次与依赖

| 计划 | wave | depends_on | 依赖合理性 | 判定 |
|---|---|---|---|---|
| 02-01 | 1 | [] | 无前置 | ✓ |
| 02-02 | 2 | [02-01] | 需要 budget.rs 的阶段埋点契约 | ✓ |
| 02-03 | 3 | [02-02] | 装配四链路；需要阶段契约与错误分类 | ✓ |
| 02-04 | 4 | [02-02, 02-03] | 需要 TTS 客户端 + 02-03 的 `pipeline/vad.rs`（静默检测） | ✓ |
| 02-05 | 5 | [02-03, 02-04] | 修改 02-03 的 `audio/playout.rs` 与 02-04 的 `audio/resample.rs`（保留离线助手） | ✓ |

- 无环、无引用不存在计划、无真实前向依赖（02-03 文中提到 02-05“AEC 归属”是描述而非依赖）；`wave = max(depends_on)+1` 全部成立。
- 与骨架“Wave 保持顺序”一致（用户确认项）。

## Q3. 阻塞门位置（父问题 3）

| 门 | 类型 / gate | 位置 | 评估 |
|---|---|---|---|
| 02-02 Task0 依赖合法性（hmac/sha2 不在审计表 → [ASSUMED]；tokio-tungstenite 版本确认） | `checkpoint:human-verify` `gate="blocking-human"` | 安装前（Cargo.toml 变更前） | ✓ 符合 gsd-planner.md:628 + gsd-executor.md:298（合法性别名门禁不得自动批准） |
| 02-04 Task2（T4.0b）跨语种克隆人耳判定（approved / 接受但需改进 / failed） | `checkpoint:human-verify` `gate="blocking"` | 探针（Task1）之后、任何注册 UI 之前 | 位置 ✓（研究要求“返回前不得设计注册 UI”）；**gate 取值建议改 blocking-human/decision → C6** |
| 02-04 Task3 cpal/hound/rubato 5.0 依赖合法性（cpal [SUS]） | `checkpoint:human-verify` `gate="blocking-human"` | 安装前 | ✓ |
| 02-05 Task0 工具链门禁（meson/ninja/pkg-config 缺失 + PyPI 合法性 + 显式 descope-AEC 决策） | `checkpoint:human-verify` `gate="blocking-human"` | `webrtc-audio-processing` 构建前 | ✓（降级为“显式用户决策”，不是构建失败的默认结果） |

- 四道门都在不可逆/外部成本步骤之前；全部 Cargo.toml/外部安装点（02-02、02-04、02-05）均有前置于安装的门禁，无遗漏。
- 降级路径（AEC descope、跨语种失败）均绑定用户 checkpoint，**不存在静默降级**（维度 7b 通过）。

## Q4. must_haves ↔ 生产任务 / 无用任务

- 5 份计划均有 `must_haves`（truths/artifacts/key_links）；抽查 artifacts 与文件清单、key_links 与装配任务一一对应（如：02-03 `cascade.rs → trace/jsonl.rs` 由 Task5 接线；02-04 `voice_store → cascade.resolve_voice` 由 Task6 接线；02-05 `playout → aec.process_render_frame` 由 Task3 接线；02-02 协议/服务端镜像由 Task6 双端同改 + `wire_shapes` 测试）。
- **无 must_have 完全缺生产者**，但两处压力点：
  1. 02-01 must_have“冷启动/热路径数字都进报告”目前只有脚本化生产者；RESEARCH:1146 阶段门禁要求的 **live（真实 key）冷启动工件无任务承接 → C4**。
  2. 失败案例库“跨语种克隆失败”案例的生产者归属冲突（02-03 占位 vs 02-04 写入）→ C1。
- 反向检查：30 个执行单元（含 4 道门）均产出目标所需物（能力代码/测试/决策）；无空转任务。checkpoint 门产出的“依赖合法性确认 / 跨语种判定 / 工具链或降级决策”都是目标依赖项，不是装饰。

## Q5. 已批准骨架保留（父问题 5）

骨架枚举 29 项（标题写 24，实为 4+7+9+4+5，用户计数偏差不影响判定，枚举为准）。**逐项核对：29/29 全部在计划中有承接，无静默丢弃。**

- 合并 4 处，均保留双方子步骤（文件与行为测试俱在）：
  - T1.1+T1.2 → 02-01 Task1（Stage/Waterfall + 停表/预算硬断言）
  - T2.1+T2.6 → 02-02 Task1（traits/error/config + 四个 mock 服务器）
  - T3.1+T3.2 → 02-03 Task1（提交门/状态机 + 句子聚合 + 本地能量 VAD）
  - T3.4+T3.5 → 02-03 Task3（重试 2×/100→200ms/500ms + 熔断 2/120s/half-open + 降级展示）
- 新增 5 个执行单元，全部来自研究修正并显式标注：02-02 Task0（依赖门禁）、02-04 Task1（T4.0a 探针）、Task2（T4.0b 门）、Task3（依赖门禁）、02-05 Task0（T5.0 工具链门）。均未改变波次结构。
- 骨架的“User-confirmed decisions”逐项落实：设置页先显示本地累计分钟数（02-03 Task7）、Wave 顺序不变、英文 STT 文件回放验证（02-02 Task3 测试）。

## Q6. 与 CONTEXT / RESEARCH 的一致性

- 研究修正 8 条全部折入（逐条核对）：跨语种探针 T4.0a/b（02-04:120/137/141-142 + MiniMax/Cartesia 回退分支写明）、流式 TTFB 重叠证明（02-01:107-108/117）、讯飞无置信分→代理 + `confidenceSource`（02-02:246、02-03:136/248）、AEC crate 无 VAD→三种 VAD 来源（02-03:137/151-155）、构建工具链前置（02-05:101-107 + T5.0）、Deepgram `language=en` + KeepAlive（02-02 Task3）、rubato 5.0 新 API / cpal 0.18 错误驱动（02-05:187-188）、禁用 `--workspace`（02-01:93/129/173）。
- CONTEXT 锁定决策：D-01..D-20 由各计划头部“本计划落地的决策”清单逐条声明（02-03:133 等），抽查实现与决策一致（三因子置信、仅低置信标记、仅无声弃权、片段级重试、熔断、降级文案、分阶段计量双面板、JSONL 溯源、失败案例库）。延后项（多级配额、人工确认、Phase 8 项）未混入。自由裁量区（阈值标定、术语权重公式）以“实验标定 + 预留字段”处理，合理。
- 两处**非计划缺陷**的文档漂移（INFO，不阻塞）：
  1. CLAUDE.md 供应商表与 ROADMAP Phase 2 计划纲要仍写 Gemini Live/Gemini Flash-Lite/MiniMax；skeleton/研究/计划采用 讯飞/Deepgram/DeepSeek/火山（ROADMAP 自身研究注记“实验结果决定 Phase 2 接线”，且骨架 Wave2 即写明四家）→ 建议后续更新 CLAUDE.md/ROADMAP 纲要。
  2. 骨架行“置信双因子（STT×翻译）” vs CONTEXT D-01“三因子（含术语命中率）”；计划按 CONTEXT（更晚的锁定决策）实现三因子——优先级正确，提请知悉。

---

## 框架维度判定

| 维度 | 判定 | 说明 |
|---|---|---|
| D1 需求覆盖 | PASS | 四个 AUDI 全承接；GOV Phase 2 范围内（01–10、12–15、17–20、16）全覆盖 |
| D2 任务完整性 | PASS | 逐任务核对 files/action/verify/done；TDD 任务含 behavior+测试命令；action 具体（含文件与断言） |
| D3 依赖正确 | PASS | 见表；无环 |
| D4 key_links | PASS | 关键接线有装配任务（见 Q4） |
| D5 规模 | WARNING | 02-01: 3/12、02-02: 7/15、02-03: 7/31、02-04: 7/15、02-05: 6/13（任务/文件，含门）。框架阈值 5+/15+ 应判 blocker；因 5 波组成系用户批准且合并已减任务数，降为警告 → C8 |
| D6 must_haves 派生 | PASS | truths 可观测（“超支指出阶段与 ms”“重说 ~100ms 停止无重叠”等），artifacts/links 与任务对应 |
| D7 上下文合规 | PASS | 见 Q6；无延后项混入 |
| D7b 范围缩减 | PASS | 无“v1/静态/以后接”式缩减；AEC descope 与跨语种失败均为显式用户决策门 |
| D7c 架构层级 | PASS | 全部能力落在责任映射表指定层（Rust 核心/前端展示）；线协议只发 confidence 枚举（02-02:372），分数留在本地 trace |
| D8 Nyquist | **FAIL** | 检查 8e：`02-VALIDATION.md` 不存在（config `nyquist_validation=true`、RESEARCH 有 `## Validation Architecture`、Phase 1 有 `01-VALIDATION.md`）。按规则 8a–8d 不再评估；抽查所见任务均带 `<automated>`（含 CI 车道命令）→ C2 修复成本低 |
| D9 跨计划数据契约 | **FAIL** | 0003 失败案例跨计划冲突 + 验收自相矛盾 → C1 |
| D10 CLAUDE.md 合规 | PASS | Safari 15.6 底线、Tailwind v3.4、无 CDN、webrtc `~2.0` 钉版、凭据只走环境变量（.env 已 gitignore、验证命令不回显敏感值）；供应商表漂移见 Q6 INFO |
| D11 研究问题解析 | **FAIL** | `## Open Questions`（:1027）无 `(RESOLVED)` 标注 → C3（6 问均有任务承接：Q1→T4.0a/b、Q2→02-01、Q3→冷/热用例、Q4→confidenceSource、Q5→用量计数、Q6→T3.8） |
| D12 模式合规 | SKIPPED | 无 PATTERNS.md（Phase 1 亦无；config `pattern_mapper=true` 与 plan-phase §7.5 期望该工件——可选补生成） |

---

## 结构化问题清单

```yaml
issues:
  - id: C1
    dimension: cross_plan_data_contract
    severity: blocker
    plans: ["02-03", "02-04"]
    description: >
      失败案例库 0003 存在两个写入方：02-03 Task6 <files> 声明 `tools/vendor-experiments/failure-cases/0003-*.json`
      并要求 18 个新案例中“跨语种克隆失败（占位待 02-04 T4.0 结论回填）”，且 --check 强制 id 唯一 + 每条
      regression_test 指向真实存在的测试；02-04 Task1 失败路径固定写入 `0003-cross-lingual-clone.json`，
      regression_test 暂指探针脚本并标 pending。执行 02-03 时该案例的回归测试尚不存在（探针在 02-04 才跑），
      而 02-04 写入后 pending 指针会使 --check/CI failure-cases 车道变红（02-05 与 02-03 的验收均要求该车道绿）。
      两条计划的验收不能同时满足。
    tasks: ["02-03 Task6", "02-04 Task1"]
    fix_hint: >
      三选一：(a) 02-03 显式预留该案例 ID（不与自建 18 例冲突），并把 --check 扩展为允许 ≤1 条
      `regression_test: "pending:<reason>"`（schema 受控值），02-04 T4.0b 判定后回填真实测试名；
      (b) 02-04 改用下一个空闲 ID（如 0021-…）并在 02-03 的 --check 中支持 pending 状态；
      (c) 把该案例整体移到 02-04（由 T4.0b 产出真实回归测试），02-03 只建其余 17 例。
  - id: C2
    dimension: nyquist_validation
    severity: blocker
    file: "02-real-cloud-pipeline-audio-core/"
    description: >
      02-VALIDATION.md 缺失。plan-phase §5.5 规定：config nyquist_validation=true 且 RESEARCH 含
      “## Validation Architecture” 时必须先生成 {phase}-VALIDATION.md，否则 STOP（不得进入 planner）。
      Phase 1 有 01-VALIDATION.md，Phase 2 没有 → 维度 8 门禁 FAIL。
    fix_hint: >
      按 plan-phase §5.5 用模板（$HOME/.claude/get-shit-done/templates/VALIDATION.md）+
      02-RESEARCH.md 的 Validation Architecture 段生成 02-VALIDATION.md（无需重跑研究）。计划侧内容已齐
      （各任务均有 <automated>），补文件即可。
  - id: C3
    dimension: research_resolution
    severity: blocker
    file: "02-RESEARCH.md:1027"
    description: >
      `## Open Questions` 标题与 6 条问题均无 RESOLVED 标注（维度 11 规则：缺标注即 FAIL）。实质均有承接：
      Q1→02-04 T4.0a/b、Q2→02-01 五边界、Q3→冷/热分别判定、Q4→confidenceSource、Q5→用量计数、Q6→T3.8。
    fix_hint: >
      将标题改为 `## Open Questions (RESOLVED)` 并为每条附一行 “RESOLVED: 由 <任务> 承接（测量型：以 T 门禁产出为准）”。
      属文档标注修复，不动 PLAN。
  - id: C4
    dimension: requirement_coverage
    severity: warning
    plans: ["02-01", "02-05"]
    description: >
      RESEARCH:1146 阶段门禁要求“AUDI-04 live latency run recorded as an artifact”方可 /gsd:verify-work；
      02-01:179 声明 live 测量“属阶段门禁的手动记录工件”但无任务拥有：02-01 的 live 变体恒 #[ignore]，
      02-04:344 与 02-05:315 的人工项清单都未列入该测量。
    fix_hint: >
      把“真实 key + 真实麦克风/耳机跑一次 latency_e2e_cold，记录冷/热 e2e ms 入 SUMMARY/HUMAN-UAT”加入
      02-05 人工项（或设为 02-05 的收尾 checkpoint）。
  - id: C5
    dimension: context_compliance
    severity: warning
    plan: "02-04"
    task: "Task4"
    description: >
      CaptureGuard 具名常量 min_secs: 15（02-04:215）与三处口径不一致：Test1 把 15s 当“下限之下”拒绝
      （02-04:199）、拒绝文案写“请至少朗读 1 分钟”、must_have/ROADMAP 成功标准 3 写“1–3 分钟”。
      若 min=15，则 15–59s 会被接受但文案逻辑成立不了；若文案/测试为准，则常量应为 60。
    fix_hint: >
      统一为 60s 下限（与 ROADMAP/文案一致，且研究 A2 的 10–30s 技术下限仍满足），Test1 改用 <60s 用例；
      或保留 15s 则改测试用例值（如 10s）并同步文案为“至少 15 秒（≥1 分钟更佳）”。
  - id: C6
    dimension: task_completeness
    severity: warning
    plan: "02-04"
    task: "Task2 (T4.0b)"
    description: >
      该门“阻塞后续全部实现”，但用 checkpoint:human-verify gate=blocking。config human_verify_mode=end-of-phase
      的规范是 human-verify 不进 mid-flight（planner-human-verify-mode.md）；且 gate=blocking 的 human-verify 在
      auto 标志（_auto_chain_active/auto_advance）开启时会被自动批准（gsd-executor.md:298）——当前两项标志均 false，
      故仅为潜在风险；gate=blocking-human 的三道门则天然免疫。
    fix_hint: >
      改为 `checkpoint:decision`（end-of-phase 模式不抑制、不可自动批准），或至少 gate="blocking-human"。
      同时在计划 objective 注明“本门要求 mid-flight 停止”。
  - id: C7
    dimension: requirement_coverage
    severity: warning
    plans: ["02-01", "02-02", "02-04", "02-05"]
    description: >
      AUDI 标签与 REQUIREMENTS.md 定义错位（以 ROADMAP 位置口径重排）：02-01[AUDI-04]=延迟装置（REQ: AUDI-06）、
      02-02[AUDI-03]=管线（REQ: AUDI-04）、02-04[AUDI-05]=克隆注册（REQ: AUDI-03）、
      02-05[AUDI-06]=AEC/热变更（REQ: AUDI-05）。四个 ID 均被认领、能力无遗漏，但 02-04/02-05 未附口径声明，
      验证器可能按 REQUIREMENTS.md 把 AUDI-05 误判给克隆任务。
    fix_hint: >
      在 02-04/02-05 各加一行与 02-01:97 / 02-02:158 相同的“需求编号口径”声明；或统一改用 REQUIREMENTS.md 编号。
  - id: C8
    dimension: scope_sanity
    severity: warning
    plans: ["02-02", "02-03", "02-04", "02-05"]
    description: >
      任务/文件：02-02 7/15、02-03 7/31、02-04 7/15、02-05 6/13。框架阈值（5+ 任务 / 15+ 文件）判 blocker；
      但 5 波组成系用户批准、任务合并（4 处）已较骨架 29 项缩减，且每任务可独立验证。
    fix_hint: >
      执行时按任务粒度刷新上下文（每任务独立验证后再进下一个）；若 02-03 出现质量滑坡，按
      Slice A/B（稳定性门 / 溯源与案例库）拆分为两段执行。
  - id: I1
    dimension: dependency_correctness
    severity: info
    plan: "02-05"
    description: "任务编号标注错位：Task1（AEC）无编号；Task2 标 T5.1/T5.2，而骨架 T5.1=AEC3/NS/AGC（即 Task1）。能力无缺失，仅追溯标签错。"
    fix_hint: "把 Task1 标注为 T5.1、Task2 改标 T5.2（或 T5.2+采集链合并说明）。"
  - id: I2
    dimension: cross_plan_data_contract
    severity: info
    plan: "02-05"
    task: "Task2"
    description: "要求与 02-04 的 enroll/capture.rs 共用有界入队类型（“不要复制粘贴两份”），但 02-05 files_modified/task files 未含 enroll/capture.rs 或共享模块路径。"
    fix_hint: "把共享类型落在已列出的 audio/mod.rs（或新增 audio/queue.rs 并加入 files），并在 02-05 文件清单补 enroll/capture.rs 的引用改动。"
  - id: I3
    dimension: claude_md_compliance
    severity: info
    plans: ["docs"]
    description: "CLAUDE.md 供应商表与 ROADMAP Phase 2 计划纲要仍写 Gemini/MiniMax，与骨架/研究/计划采用的 讯飞/Deepgram/DeepSeek/火山 不一致（ROADMAP 研究注记已授权实验决定接线）；AI-SPEC 的 `cargo test --workspace` 行已被计划显式纠正。"
    fix_hint: "在后续 docs 更新中同步 CLAUDE.md 供应商表与 ROADMAP 纲要（不阻塞执行）。"
  - id: I4
    dimension: context_compliance
    severity: info
    plans: ["02-03"]
    description: "骨架行写“置信双因子（STT×翻译）”，CONTEXT D-01 为三因子（含术语命中率）；计划按 CONTEXT 实现（更晚的锁定决策，优先级正确）。"
    fix_hint: "知悉即可；如用户本意是双因子，需在 CONTEXT 层面确认后再改计划。"
```

## 建议

1. **修订轮（planner）**：修 C1（唯一会破坏执行/CI 的实质冲突），顺带处理 C4–C6（均为小改）。
2. **编排层（不经 planner）**：补 02-VALIDATION.md（C2）、为 RESEARCH 的 Open Questions 加 RESOLVED 标注（C3）。
3. C7/C8/I1–I4 为追溯性与执行策略建议，不阻塞执行。

修订后按 Revision Gate 重新提交本检查（上限 3 轮）。
