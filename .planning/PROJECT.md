# 极言 NexTalk — AI 跨语言实时面试辅助系统

## What This Is

极言（NexTalk）是一款面向中文母语用户的"超级沟通外挂"：在跨国技术面试中，用户以中文自然表达，系统实时识别、翻译并克隆用户本人音色，向面试官输出无缝衔接的流利英文；同时手机网页端作为"提词器"，实时滚动双语字幕并给出基于用户简历与题库的 AI 回答策略。v1 聚焦技术面试场景（中↔英），形态为本地单机桌面应用（Tauri + Rust，macOS 优先）+ 手机 H5 跨端协同 + 云端 AI API 调用。

## Core Value

让用户以母语思考、以本人音色讲出地道英文——面试官听到的是无缝衔接的英文回答，用户看到的是实时字幕与回答策略，端到端延迟 ≤ 2 秒。如果其他都失败，这条链路必须成立。

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] 音频核心：虚拟声卡接管会议软件音频（macOS CoreAudio/BlackHole），支持中→英实时转换 + 用户音色克隆输出，对方英文原声透传耳机（零延迟），集成 AEC 回声消除
- [ ] 级联流式管线：STT 识别 3-5 词即触发翻译，翻译前半句即触发 TTS 合成，端到端延迟 ≤ 2s
- [ ] 桌面端静默运行：Tauri + Rust 客户端，全局快捷键 Cmd+Shift+H 一键真隐藏/唤出（防抓屏），双栏扩展视图（左字幕右 AI 辅助）
- [ ] 跨端提词器：桌面端生成局域网二维码，手机扫码打开 H5 提词器（无需安装 App），上半屏双语滚动字幕、下半屏 AI 策略卡片，支持全中/全英/双语切换
- [ ] AI 面试辅助引擎：导入简历（PDF/Word）与高频面试题库；自动检测提问结束（静默检测 + LLM 问句完整性判断）触发 AI 思考；综合【简历真实经历 + 题库预设答案 + 实时网络搜索】生成流式要点提纲
- [ ] 会后本地资产：本地分轨录制（我的声音 vs 对方声音）、双语逐字稿导出、面试复盘报告生成（含 Action Items、情绪与关键关注点提取）
- [ ] UI 设计系统落地：新粗野主义暗黑风格（4px 粗黑边框 + 硬偏移阴影 + 传送门绿/瑞克蓝/莫蒂黄功能色），参考 HTML 4 屏为视觉基准，补齐虚拟声卡引导向导、录音资产、复盘报告等缺页

### Out of Scope

- Windows 平台（WASAPI/VB-Audio 驱动路线）— v1 macOS 优先，音频核心验证后再议
- 自有 SaaS 后端、账号体系、云存储、多租户计费 — 用户决定 v1 为纯本地工具（需求文档标注"按需建设"），产品验证后重新评估
- 外贸商务谈判、多语种团队例会场景 — v1 聚焦技术面试，共用引擎但 AI 提示逻辑不同
- 中↔英以外的语言对 — 需求仅明确中→英语音 + 英→中字幕

## Context

- 输入文档：《AI 同传.docx》（需求文档，含完整系统架构：音频驱动层/桌面客户端/AI 管线/手机端/后端按需建设）、《设计规范.docx》（Copilot C-137 UI 设计规范 V1.0 已确认）、《AI同传译.txt》（参考 HTML，Rick & Morty 风格 4 屏 mockup：桌面连接/桌面双栏/手机字幕/手机 AI 辅助）
- 技术环境：macOS 优先（需适配 12.7 Monterey 老版本）；目标会议软件 Zoom/Teams/腾讯会议；驱动安装需企业级开发者账号签名公证
- 需求文档指定的 AI 供应商（Deepgram、GPT-4o-mini/4o、ElevenLabs、Tavily）为 2024-25 选型，2026 年市场已有更优替代——研究阶段重新评估，以延迟与成本择优，不锁死
- 参考 HTML 使用 Tailwind/FontAwesome/Google Fonts CDN——生产环境（Tauri 离线）必须本地打包
- 无现有代码，绿地项目
- 合规提醒：分轨录制涉及对方声音，产品化前需处理录音告知/同意（原型阶段不阻塞）

## Constraints

- **Tech stack**: 桌面端 Tauri + Rust；手机端 H5（React/Vue + WebSocket/SSE）；AI 能力全部走云端 API（本地不跑模型）
- **Performance**: 端到端语音延迟 ≤ 2s；AI 策略提示 ~1.5s；必须级联流式（禁止整句串行）
- **Compatibility**: macOS 12.7 Monterey 可用；低延迟音频缓冲区
- **Design**: 设计规范 V1.0 已确认——新粗野主义、Space Grotesk、三功能色语义（绿=用户/发音、黄=AI 策略、蓝=翻译/系统提示）；参考 HTML 为视觉基准
- **Privacy**: 纯本地存储（录音、逐字稿、复盘报告均不传自有云端）；AI API 调用时注意最小化数据暴露

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| v1 聚焦技术面试场景 | 参考 HTML 只覆盖面试场景，提示逻辑最明确；外贸/例会共用引擎但场景感知复杂 | — Pending |
| 语言对仅中↔英 | 需求文档唯一明确的方向，TTS 音色克隆成本随语言对线性增长 | — Pending |
| v1 纯本地工具，无自有后端 | 用户从"全部进 v1"改为"纯本地工具"（以后者为准）；需求文档本身标注后端"按需建设" | — Pending |
| macOS 优先，Windows 后续里程碑 | 两套音频驱动（CoreAudio vs WASAPI）是双倍核心难度 | — Pending |
| Tauri + Rust | 需求文档强烈推荐：Rust 低拷贝延迟 + 打包体积小 + 老 Mac 友好 | — Pending |
| 品牌 NexTalk/极言，弃用 Copilot C-137 | C-137 是 Rick & Morty 工作名，正式品牌以需求文档为准 | — Pending |
| 仅原声透传，无中文合成语音选项 | 与需求文档"英听英"一致，零延迟；中文理解靠手机字幕 | — Pending |
| AI 提示仅自动触发（静默检测 + LLM 问句完整性判断） | 用户明确拒绝手动按钮；⚠️ 断句误判是核心体验风险，需在管线中重点验证 | ⚠️ Revisit |
| AI 供应商可替换，研究阶段择优 | 文档选型已过时 1-2 年，以 2026 年延迟/成本/音色质量重新评估 | — Pending |
| 参考 HTML 为视觉基准，缺页（引导向导/录音资产/复盘报告）按设计规范补齐 | 设计规范 V1.0 已确认，HTML 覆盖 4 屏核心界面 | — Pending |
| 防抓屏 = 窗口真隐藏（orderOut），透明度动画仅作过渡反馈 | 截屏引擎读窗口缓冲区，透明度降 0 防不住捕获（需求文档此处有技术误述） | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-26 after initialization*
