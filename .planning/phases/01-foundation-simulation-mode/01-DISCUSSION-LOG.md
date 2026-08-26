# Phase 1: Foundation + Simulation Mode - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-27
**Phase:** 01-foundation-simulation-mode
**Areas discussed:** 前端框架确认, 桌面窗口形态, 仿真演示脚本, 供应商实验范围

---

## 前端框架确认

| Option | Description | Selected |
|--------|-------------|----------|
| React 19 | 研究推荐的 2026 主流：生态最大、Tauri 官方支持最佳、桌面端与手机 H5 可共享组件与类型。STACK.md 已按 React 19 + Vite 7 规划 | ✓ |
| Vue 3 | 需求文档提到"React/Vue"二选一。模板语法简洁但生态支持弱于 React，需推翻 STACK.md 部分选型 | |

**User's choice:** React 19（推荐）
**Notes:** 与 STACK.md 研究一致，TypeScript + Vite 7。

---

## 桌面窗口形态

| Option | Description | Selected |
|--------|-------------|----------|
| 双独立窗口 | 340×680 控制台与 860×680 双栏是两个独立窗口，各司其职；与参考 HTML 双屏一致，隐形模式隐藏所有窗口 | ✓ |
| 单窗口切换 | 同一窗口内切换尺寸/视图，窗口管理简单但切换时尺寸跳动，与参考 HTML 视觉不一致 | |

**User's choice:** 双独立窗口（推荐）
**Notes:** 控制台为中枢导航，双栏承载会话直播；Phase 4 隐形模式隐藏所有窗口。

---

## 仿真演示脚本

| Option | Description | Selected |
|--------|-------------|----------|
| 多轮面试脚本 | 3-4 个技术问题 + 策略卡片联动 + 打字机/生成中/打断重说等状态演示，完整体现产品能力；保留参考 HTML 数据库优化场景作为其中一轮 | ✓ |
| 仅复刻参考场景 | 只复刻参考 HTML 的一轮对话，实现最简但演示效果单薄 | |
| 可配置脚本 | 脚本文件可外部配置（JSON），额外增加配置 UI/格式设计复杂度 | |

**User's choice:** 多轮面试脚本（推荐）
**Notes:** Phase 1 内置 mock 数据（标注「模拟数据」），不做外部可配置。

---

## 供应商实验范围

| Option | Description | Selected |
|--------|-------------|----------|
| 只搭框架 | 本阶段搭建实验框架（评估脚本、盲测集、RTT 测量工具），真实跑留到 Phase 2 前；不需要现在提供 API keys，不阻塞 UI 开发 | ✓ |
| 真实跑实验 | 本阶段真实接入 API 跑 STT A/B 与克隆音色盲测；需要用户提供 Deepgram/Gemini/MiniMax 等 API keys | |
| 暂不涉及 | 不专门搭建框架，实验细节留给 Phase 2 规划时再定 | |

**User's choice:** 只搭框架（推荐）
**Notes:** 真实 API 实验在 Phase 2 规划前执行，届时需要用户提供 API keys。

---

## Claude's Discretion

- 工作区结构（Tauri app + 共享协议包 + 手机 H5 组织方式，monorepo 方案）
- TypeScript 严格模式、测试框架（倾向 Vitest）、Lint/格式化配置
- 组件文件组织与命名（按 UI-SPEC 组件清单）
- WebSocket 协议消息格式具体设计
- 仿真事件流节奏控制参数

## Deferred Ideas

None — discussion stayed within phase scope.
