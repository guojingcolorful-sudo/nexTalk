# Phase 1: Foundation + Simulation Mode - Context

**Gathered:** 2026-08-27
**Status:** Ready for planning

<domain>
## Phase Boundary

交付 Phase 1 的完整产品 UI 骨架：Tauri 2 工作区（React 19 + Vite + Tailwind v3.4 本地打包）+ 新粗野主义设计系统落地（参考 HTML 4 屏重实现 + 6 个缺页设计）+ 桌面双独立窗口（340×680 微型控制台 + 860×680 双栏视图）+ 局域网 WebSocket 服务与二维码配对 + 手机 H5 提词器 + SimSource 仿真音频源与 mock 管线——在模拟音频上端到端可演示，无需真实音频驱动与 AI API。

本阶段不做：真实音频管线（Phase 2）、虚拟声卡集成（Phase 3）、真隐藏（Phase 4）、AI 策略引擎（Phase 5）、录制与复盘（Phase 6）。
</domain>

<decisions>
## Implementation Decisions

### 前端技术栈
- **D-01:** 前端框架锁定 **React 19 + TypeScript**（+ Vite 7 构建）。需求文档曾提 "React/Vue" 二选一，经确认选 React（与 STACK.md 研究一致，桌面端与手机 H5 共享组件与类型）。不用 Vue。

### 桌面窗口形态
- **D-02:** **双独立 Tauri 窗口**：340×680 微型控制台与 860×680 双栏视图是两个独立窗口，各自管理。控制台为中枢（导航入口），双栏承载会话直播。与参考 HTML 双屏视觉一致；Phase 4 隐形模式将隐藏所有窗口（orderOut），音频引擎独立进程持续运行。

### 仿真演示脚本
- **D-03:** 仿真会话使用**多轮英文面试脚本**（3-4 个技术问题），完整演示：面试官英文提问 → 双语字幕 → AI 策略卡片联动 → 用户中文回答 → 打字机渲染 → 生成中指示 → 打断/重说等状态。参考 HTML 的「数据库优化」场景作为其中第一轮。脚本为 Phase 1 内置 mock 数据（标注「模拟数据」），不做外部可配置（配置 UI 属后续阶段）。

### 供应商决定性实验
- **D-04:** Phase 1 **只搭建实验框架**（评估脚本、盲测集设计、RTT 测量工具骨架），真实 API 实验（中文 STT A/B、克隆音色盲测、网络 RTT）留到 Phase 2 规划前执行——届时需要用户提供 API keys。本阶段不阻塞 UI 开发。

### Claude's Discretion
用户将以下实现细节交由 Claude 决定（研究 + 规划代理自行决策，无需再问）：
- 工作区结构（Tauri app + 共享协议包 + 手机 H5 的组织方式，monorepo 方案）
- TypeScript 严格模式配置、测试框架选择（倾向 Vitest）、ESLint/格式化配置
- 组件文件组织与命名（按 UI-SPEC 组件清单：NexTalkBrand、StealthCard、ChatBubble 等）
- WebSocket 协议消息格式的具体设计（在 UI-SPEC 约束的语义下）
- 仿真事件流的节奏控制参数（打字机速度、间隔时长等微调值）

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 设计锁定（最高优先级）
- `.planning/phases/01-foundation-simulation-mode/01-UI-SPEC.md` — **已批准的 UI 设计契约**（6/6 维度通过）。含设计系统、25 组件清单、完整状态循环、动效契约、文案契约（UI 功能命名全中文锁定）、6 缺页契约、无障碍契约。规划与执行必须遵守，不得偏离。
- `.planning/ref/design-spec.md` — 设计规范 V1.0（锁定）：色彩/字体/容器尺寸/组件/间距。
- `.planning/ref/reference-mockup.html` — 参考 HTML 4 屏视觉基准（锁定）：重实现为 React 组件 + 本地打包，不得保留 CDN 引用。

### 需求与范围
- `.planning/ROADMAP.md` — Phase 1 目标、5 条成功标准、5 个计划的粗纲（01-01 至 01-05）、研究备注（供应商实验框架）。
- `.planning/REQUIREMENTS.md` — 本阶段需求：UI-01/02/03、SYNC-01..05、DSK-01/02/04。
- `.planning/PROJECT.md` — 项目关键决策表（品牌、本地单机、macOS 优先、防抓屏 orderOut、语言桥定位等）。

### 技术研究
- `.planning/research/STACK.md` — 技术栈定案：Tauri 2.11 + Rust、React 19 + Vite 7、Tailwind **v3.4**（Safari 15.6 硬底线，禁 v4）、FontAwesome 6 本地打包、Space Grotesk 自托管、零 CDN。
- `.planning/research/ARCHITECTURE.md` — 构建顺序论证、SimSource/AudioSource trait 解耦模式、局域网同步（tokio-tungstenite 单一 WebSocket 而非 SSE）、wake lock 安全上下文回退（隐藏循环视频法）。
- `.planning/research/PITFALLS.md` — 坑点：wake lock 需用户手势、Safari 15.6 兼容、partial/final 语义（Phase 2 适用）、GPL 陷阱（Phase 3 适用）。

### 项目文档
- `CLAUDE.md` — 项目指南（GSD 工作流约定 + 技术栈上下文）。

</canonical_refs>

<code_context>
## Existing Code Insights

绿地项目——无现有代码。唯一的既有资产是 `.planning/ref/reference-mockup.html`（Tailwind CDN 单文件 mockup）。**重实现方式**：拆解为 React 组件 + 设计 token（CSS 变量），Tailwind 配置迁移为本地构建，删除所有 CDN（Tailwind/FontAwesome/Google Fonts），字体与图标本地打包。参考 HTML 只作为视觉与交互基准，不直接照抄其类名结构。

### Integration Points
- 新代码连接点：Tauri 2 窗口（340×680 控制台 + 860×680 双栏）↔ Rust 后端（LAN WebSocket 服务 + 模拟事件源）↔ 手机 H5（独立可部署静态页面，经 QR 配对连接）。
- 共享协议包：桌面端与手机端共用的 WebSocket 消息类型（字幕事件、策略事件、状态事件）需在同构 TypeScript 包中定义。

</code_context>

<specifics>
## Specific Ideas

- 仿真多轮脚本第一轮 = 参考 HTML 的数据库优化场景（慢查询日志 → 拆连表查询 → Redis 缓存层），保证与已确认视觉基准的对齐。
- Rick & Morty 文案彩蛋策略按 UI-SPEC：主品牌 NexTalk/极言，仅保留 1-2 处非功能性彩蛋。
- 所有 mock 数据（模拟会话内容、模拟简历、模拟策略）标注「模拟数据」，与真实数据在视觉上可区分（UI-SPEC 已定义）。
- 手机端常亮：Wake Lock API 需安全上下文，`http://192.168.x.x` 下不可用 → 隐藏循环视频回退方案 + 用户手势触发（「开始提词」按钮作为手势）。
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-foundation-simulation-mode*
*Context gathered: 2026-08-27*
