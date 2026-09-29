# Phase 2: Real Cloud Pipeline + Audio Core - Context

**Gathered:** 2026-09-29
**Status:** Ready for planning (供应商决定性实验先行)

<domain>
## Phase Boundary

交付真实 mic→耳机级联流式翻译链：中文 STT → 增量翻译 → 克隆音色 TTS，端到端 ≤2s（含冷启动），延迟测量装置（分阶段瀑布计时）作为下游一切的门。供应商选型由 `tools/vendor-experiments/` 的 A/B 实测决定（实验在规划前执行，用户正在申请 key）。

本阶段不做：虚拟声卡（Phase 3）、真隐藏（Phase 4）、AI 策略引擎与术语库 UI（Phase 5/4）、录制复盘（Phase 6）——但 Phase 2 必须**埋好**溯源数据（JSONL 事件流 + 协议字段），否则复盘与反馈闭环无数据可用。
</domain>

<decisions>
## Implementation Decisions

### 输出可控：置信与弃权（实时流语义）
- **D-01:** 置信度 = **三因子加权**：STT 置信度 × 翻译模型输出置信 × 术语命中率。阈值与权重由研究+规划代理用实验数据标定（不拍初值）。
- **D-02:** 实时字幕**仅低置信标记**（气泡角红色「低置信」微章）；中置信黄色提醒只在复盘报告呈现，直播流零打扰。
- **D-03:** 弃权**仅限无声场景**：音频无有效文本（空转/无法识别）才弃权并显示「待翻译」标记；**低置信不弃权、照常输出但红色标记**——实时面试必须连续输出，用户选择优先于文档里更严格的 abstention 原则（该原则在复盘环节生效）。
- **D-04:** 确定性输出前校验：数字/金额/日期格式一致性规则校验（如 STT「800毫秒」↔ 译文「800ms」），全部译文过此层。

### 可溯源：数据结构与线协议扩展
- **D-05:** 溯源落地 = **JSONL 事件流文件**（每句一行：原文/译文/置信/时间戳/术语命中/provider/modelVersion），与现有 timeline 内存模型同构、追加写、零依赖。SQLite 索引与复盘查询 UI 留到 Phase 6。
- **D-06:** 时间戳粒度 = **语句级 segment 偏移**（每句相对会话起始的 ms）。
- **D-07:** 线协议（`packages/protocol` ServerEvent 闭合联合）向后兼容扩展：
  - `subtitle` 变体新增 `confidence: 'high'|'medium'|'low'`
  - 新增 `trace` 对象：`segmentStartMs`、`termHits`、`provider`、`modelVersion`
  - 新增 `abstained` 事件（无声弃权）
- **D-08:** 模型版本**逐句记录**（实验期频繁换模型，会话级快照会归因失真）。

### 可兜底：容错链路 v1 参数
- **D-09:** **片段级重试**：瞬时错误（超时/429/5xx）重试 2 次、指数退避 100→200ms、总预算 500ms 内放弃该片段并流下一片段。禁止整句串行重试（会破 2s 预算）。
- **D-10:** 熔断：**连续 2 次失败熔断该供应商 120s**，半开探测恢复。
- **D-11:** 备用供应商：**实验后定**——A/B 实测双路径均达标且预算允许才配备用 key；否则 v1 单供应商 + 降级。
- **D-12:** 降级展示（中文锁定）：红色微章 + 「翻译服务暂时不可用」+ 显示原文 + 副标注「正在重试」。

### 成本可算：本地化计量（开发者面板）
- **D-13:** 分阶段计量：STT 按音频分钟、翻译按 token、TTS 按字符；每句记录各阶段用量，桌面设置页展示累计成本面板（**开发者/观测用途**，归因到阶段）。
- **D-14:** 成本路由：**实验期固定路由**（每阶段单一供应商），自动升降级留到数据积累后。
- **D-15:** 用户侧预算闸门：**提示式，待细化**——达到月度用量阈值给予提示，不硬停（与未来收费模式联动，见 Deferred）。

### 架构拓扑：本地网关与公测基建（2026-09-29 二次讨论）
- **D-16:** 三层容错架构映射：**前端层** = 手机 H5 / 桌面 Webview（UX 兜底：状态展示、低置信标记、重试按钮、本地缓存）；**网关层** = **桌面 Rust 内核**（重试/熔断/成本闸门，D-09..D-15 在此落地）——Phase 2-7 无远程网关、保持纯本地；**管理后台** = 公测前新建（成本看板/错误日志/卡点分析/模型切换/配额管理/人工干预）。
- **D-17:** 公测前最小基建优先级：**账号（内测码+邮箱注册）→ 埋点上报 → 观测看板**。观测是控制的前提；远程网关（服务器持 key）随商业化（分钟包月制）引入，本地直连为 v1 兜底。
- **D-18:** 埋点日志**复用本地 JSONL 溯源流**（D-05），云端聚合上报供管理后台查询——不另造第二套埋点体系。
- **D-19:** ROADMAP 新增 **Phase 8: Beta Infrastructure + Commercialization**（Phase 7 之后、公测之前）承接：账号体系、埋点上报、观测看板、远程网关最小版、分钟包月制定价落地。Phase 2 只需保证 JSONL 结构与埋点字段（用户/任务/模型/耗时/状态/错误码）可聚合。

### Claude's Discretion
- 置信三因子的具体权重公式与低置信红线（实验标定）
- JSONL 事件流的字段 schema 细节与文件滚动策略
- 延迟测量装置的具体实现（分阶段时间戳注入点）
- 重试/熔断参数的实验期微调

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 设计锁定（最高优先级）
- `.planning/phases/01-foundation-simulation-mode/01-UI-SPEC.md` — UI 设计契约：三级状态视觉（红色微章等）须符合既有设计语言；文案中文锁定（降级文案在此约束下）。
- `.planning/phases/01-foundation-simulation-mode/01-CONTEXT.md` — Phase 1 决策（D-04 供应商实验框架、partial/final 语义预告）。

### 需求与范围
- `.planning/ROADMAP.md` — Phase 2 目标、5 条成功标准、5 个计划粗纲（02-01 至 02-05）、研究备注（供应商实验先于规划）。
- `.planning/REQUIREMENTS.md` — 本阶段需求：AUDI-03/04/05/06。
- `.planning/PROJECT.md` — 项目关键决策：2s 预算、纯本地 v1、隐私最小暴露、2026 供应商再评估结论（CLAUDE.md 内）。

### 技术研究
- `CLAUDE.md` — 2026 供应商再评估表（各阶段 Primary/Alternative、成本、置信度标注）；webrtc-audio-processing AEC3、rubato 重采样、silero-vad 等管线组件选型。
- `.planning/research/STACK.md` — 技术栈定案与供应商候选推理。
- `.planning/research/PITFALLS.md` — partial/final 语义（本阶段落地）、Safari 15.6 兼容。

### 供应商实验
- `tools/vendor-experiments/README.md` — 实验框架总览与零 key 政策（D-04 延续）。
- `tools/vendor-experiments/stt-ab-protocol.md` — STT A/B 协议（实验按此执行，结果决定 02-02 选型）。
- `tools/vendor-experiments/.env.example` — 实验凭据字段约定（讯飞/DeepSeek/火山/Deepgram/MiniMax）。
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `packages/protocol/src/index.ts` — 闭合联合类型 + `isServerEvent` 守卫：D-07 的 confidence/trace/abstained 扩展在此完成（TS 侧），Rust 侧镜像在 `apps/desktop/src-tauri/src/lan/server.rs`。两端同步扩展 + 测试同步更新是 Phase 1 已验证的协作模式。
- `apps/desktop/src-tauri/src/state.rs` — `SessionState` 状态机 + timeline + 广播：JSONL 落地写入点与事件广播共用 `append_event`/`publish` 路径；溯源字段随事件入流。
- `apps/desktop/src-tauri/src/sim/` — SimSource trait 抽象：真实管线以 `AudioSource` trait 替换 SimSource（Phase 1 架构论证已预留解耦）。
- `tools/vendor-experiments/rtt/measure.mjs` — RTT 测量工具（实验直接复用）。

### Established Patterns
- **单事件模型双传输**：所有新事件（abstained、带 trace 的 subtitle）自动流向桌面 + 手机，无需双份实现。
- **威胁注册表**（T-01-xx）：新管线引入的威胁（供应商密钥处理、提示注入、输出校验）在规划时登记。
- **TDD RED→GREEN** + 确定性测试（注入时钟）是 Phase 1 的执行惯例，Phase 2 延续。

### Integration Points
- Rust 音频图（cpal 采集 → 重采样 → VAD 分段）→ STT WebSocket 客户端 → 翻译流 → TTS 流 → 播放，全部在 `apps/desktop/src-tauri/src/` 内新增模块；手机/桌面 UI 只消费既有协议事件。
- 延迟测量注入点：每阶段边界打时间戳，汇总到 e2e 瀑布（02-01）。
</code_context>

<specifics>
## Specific Ideas

- 用户提供的系统设计原则文档（可控/可溯源/可兜底/成本可算四原则 + 系统论要素-连接-功能框架）是本阶段决策的上位依据，其 SaaS 泛化表述已按 v1 本地工具现实裁剪（见各 D 条目）。

</specifics>

<deferred>
## Deferred Ideas

- **商业化定价 + 公测基建**：已晋升为 ROADMAP Phase 8（D-19），分钟包月制 + 内测码 + 邮箱注册 + 漏斗观察 + 远程网关最小版；Phase 2 仅埋可聚合数据。
- **用户侧预算闸门细化**：与收费模式联动后细化（提示式 → 硬闸门 → 恢复流程）。
- 多级配额（用户级/部门级）、RBAC（查看者/编辑者/管理员）、审计日志——SaaS 概念，v1 不适用，商业化阶段再评估。
- 低置信「人工确认」环节——实时流不可行，复盘报告（Phase 6）中实现。
- 反馈闭环 UI（用户修正→术语库/评测集）——术语库 Phase 4、复盘 Phase 6；Phase 2 仅埋错误记录数据。

</deferred>

---

*Phase: 02-real-cloud-pipeline-audio-core*
*Context gathered: 2026-09-29*
