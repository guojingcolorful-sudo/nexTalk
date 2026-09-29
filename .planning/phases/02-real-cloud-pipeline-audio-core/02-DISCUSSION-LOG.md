# Phase 2: Real Cloud Pipeline + Audio Core - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-29
**Phase:** 02-real-cloud-pipeline-audio-core
**Areas discussed:** 置信与弃权落地, 溯源数据结构与线协议扩展, 容错链路 v1 参数, 成本治理本地化（引出定价讨论）

---

## 置信与弃权落地（实时流语义）

| Option | Description | Selected |
|--------|-------------|----------|
| 三因子加权 | STT×翻译×术语命中组合 | ✓ |
| 仅 STT 置信度 | 只信音频侧 | |
| 仅翻译置信度 | 只信模型侧 | |

| Option | Description | Selected |
|--------|-------------|----------|
| 仅低置信标记 | 红色微章，中置信只在复盘 | ✓ |
| 三级全标 | 实时全标（噪音大） | |
| 实时不标 | 全部移到复盘 | |

| Option | Description | Selected |
|--------|-------------|----------|
| 低置信即弃权 | 宁缺毋滥（文档原则） | |
| 仅无声弃权 | 无声才弃权，低置信硬译+标红 | ✓ |
| 组合条件弃权 | 低置信+术语未命中 | |

| Option | Description | Selected |
|--------|-------------|----------|
| 实验标定 | 阈值由实验数据定 | ✓ |
| 先拍初值 | 预设 0.6 红线 | |

**Notes:** 用户选择「仅无声弃权」覆盖了设计文档中更严格的 abstention——实时面试必须连续输出；低置信通过红色标记 + 复盘追溯兜底。

## 溯源数据结构与线协议扩展

全部采纳推荐项：JSONL 事件流（SQLite 到 Phase 6）、语句级 segment 偏移、协议加 confidence+trace+abstained、模型版本逐句记录。

## 容错链路 v1 参数

| Option | Description | Selected |
|--------|-------------|----------|
| 片段级×2 | 100→200ms 退避、500ms 预算 | ✓ |
| 整句重试 | 最坏破预算 | |
| 不重试 | 丢内容 | |

熔断：用户选择 **2 次失败 / 120s 恢复**（比推荐的 3 次/30s 更保守）。
备用供应商：实验后定。降级文案：锁定「翻译服务暂时不可用」+「正在重试」+ 原文。

## 成本治理本地化（→ 引出定价讨论）

用户升级了议题：
- 计量 = **开发者面板**（内部观测，非用户计费）
- 预算机制与**收费模式**联动，需先讨论定价
- 定价模式：**分钟包月制**（月费含分钟数，超出买量）
- 公测：**免费公测 + 内测码 + 邮箱注册**，观察每用户使用漏斗；**后端可逐步建立**（这是对「v1 纯本地」决策的动态演进信号）
- 定价决策记入 ROADMAP 待议清单，不阻塞 Phase 2 工程
- 成本路由：实验期固定路由

## Claude's Discretion

置信权重公式与红线标定、JSONL schema 细节、延迟测量实现、重试/熔断参数实验期微调。

## Deferred Ideas

分钟包月制定价、免费公测机制（内测码+邮箱+漏斗）、用户侧预算闸门细化、多级配额/RBAC/审计（SaaS 概念）、低置信人工确认（复盘实现）、反馈闭环 UI（Phase 4/6）。
