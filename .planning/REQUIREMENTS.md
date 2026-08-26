# Requirements: 极言 NexTalk

**Defined:** 2026-08-26
**Core Value:** 让用户以母语思考、以本人音色讲出地道英文——面试官听到无缝衔接的英文回答，用户看到实时字幕与回答策略，端到端延迟 ≤ 2 秒。

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Audio Core (AUDI)

- [ ] **AUDI-01**: 官方 BlackHole 虚拟声卡安装向导（引导安装为独立系统组件，不捆绑 GPL 代码）+ 设备检测与路由
- [ ] **AUDI-02**: 「耳机 + BlackHole」聚合多输出设备：面试官原声零延迟透传耳机 + 回采捕获供 AI 分析
- [ ] **AUDI-03**: 音色注册（1-3 分钟录音 → 克隆音色）；未注册前可用库存音色试用翻译链路
- [ ] **AUDI-04**: 中→英级联流式管线（STT partial → 翻译 → TTS partial），端到端 ≤2s，遵守「partial 渲染、final 发声」稳定性门
- [ ] **AUDI-05**: AEC 回声消除 + 音频设备热变更处理
- [ ] **AUDI-06**: 延迟测量装置（mic→输出逐级瀑布计时，门禁一切下游阶段）
- [ ] **AUDI-07**: 术语/热词表（STT/MT 术语保护，如 K8s、backpressure、幂等性）

### Sync (SYNC)

- [ ] **SYNC-01**: 局域网 WebSocket 服务 + 二维码配对（token 认证，手机无需安装 App）
- [ ] **SYNC-02**: 手机 H5 提词器：上半屏双语流式字幕、下半屏 AI 策略卡片
- [ ] **SYNC-03**: 语言切换（全中/全英/双语），面试官与用户气泡独立切换
- [ ] **SYNC-04**: 手机屏幕常亮（Wake Lock 安全上下文失败时回退方案）
- [ ] **SYNC-05**: 打字机流式渲染（字逐个出现，掩盖 LLM 生成延迟）

### Desktop (DSK)

- [ ] **DSK-01**: 微型控制台（340×680）：隐形模式、跨端同步、知识库状态
- [ ] **DSK-02**: 双栏扩展视图（860×680）：左双语字幕、右 AI 辅助流
- [ ] **DSK-03**: Cmd+Shift+H 一键真隐藏/唤出（窗口 orderOut 脱离层级，音频引擎与 UI 进程分离持续运行）
- [ ] **DSK-04**: 桌面双语字幕 + 单语/双语切换

### Copilot (COPT)

- [ ] **COPT-01**: 简历（PDF/Word）+ 高频面试题库导入与预索引（面试开始前完成）
- [ ] **COPT-02**: 提问结束自动检测（双预算静默端点 550ms/1300ms + LLM 问句完整性判断；真实语料误触发 <5%、截断 <3%）
- [ ] **COPT-03**: 策略卡片 ~1.5s（简历真实经历 + 题库预设 + 实时网络搜索）
- [ ] **COPT-04**: 流式要点提纲 + 参考话术（用户以自己的话回答，非全文代写）

### Recording & Review (REC)

- [ ] **REC-01**: 本地双轨录制（用户轨/面试官轨，Opus 编码）
- [ ] **REC-02**: 双语逐字稿导出（SRT/Markdown/Word，带时间戳）
- [ ] **REC-03**: 面试复盘报告（Action Items、情绪、关键关注点、逐题回放）
- [ ] **REC-04**: 录音前置同意门（合规：加州 §632 全方同意州属刑事风险，不可只做设置项）

### UI Design System (UI)

- [ ] **UI-01**: 新粗野主义设计系统落地（设计规范 V1.0：4px 粗黑边框、硬偏移阴影、三功能色、Space Grotesk、圆点矩阵背景）
- [ ] **UI-02**: 参考 HTML 4 屏实现 + 缺页补齐（引导向导、录音资产、复盘报告、音色注册、术语表、简历导入）
- [ ] **UI-03**: 零 CDN 本地打包（Tailwind v3.4，兼容 macOS 12.7 Safari 15.6 / WKWebView）

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Practice & Platform

- **PRAC-01**: 模拟面试练习模式（AI 面试官，复用同传引擎）
- **PLAT-01**: Windows 客户端（WASAPI/VB-Audio 驱动）
- **PLAT-02**: 更多语言对（日/韩/德等）

### Ecosystem

- **ECO-01**: 云同步与账号体系（产品验证后重新评估）
- **ECO-02**: 表达质量反馈（Yoodli 式 filler/pacing 分析）
- **ECO-03**: 外贸谈判、多语种例会场景（共用引擎，不同提示层）

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| 全文代写模式（AI 写答案用户照读） | Anti-feature：2026 年检测率 2%→10%+ 的作弊军备竞赛；违背「语言桥梁」定位；监管灰区 |
| 编程平台提示（HackerRank/LeetCode 悬浮提示） | 作弊感知最强、雇主最敏感；FinalRound/Sensei 已占位；与定位冲突 |
| 面试官音色克隆反向输出（英文→克隆中文语音） | 对方生物特征数据的同意/合规风险；原声透传已定案 |
| 视频/数字人形象 | 不同产品赛道；对面试无价值 |
| 机考/笔试防作弊功能 | 纯作弊垂直；品牌风险 |
| SaaS 后端/账号/云存储 | v1 纯本地决定；产品验证后重新评估 |
| Windows 平台 | v1 macOS 优先；音频核心验证后进入 v2 |
| 模拟面试练习模式 | 「第二款产品」性质；Yoodli/Huru 已占领；v2 复用引擎低成本实现 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| AUDI-01 | Phase 3 | Pending |
| AUDI-02 | Phase 3 | Pending |
| AUDI-03 | Phase 2 | Pending |
| AUDI-04 | Phase 2 | Pending |
| AUDI-05 | Phase 2 | Pending |
| AUDI-06 | Phase 2 | Pending |
| AUDI-07 | Phase 4 | Pending |
| SYNC-01 | Phase 1 | Pending |
| SYNC-02 | Phase 1 | Pending |
| SYNC-03 | Phase 1 | Pending |
| SYNC-04 | Phase 1 | Pending |
| SYNC-05 | Phase 1 | Pending |
| DSK-01 | Phase 1 | Pending |
| DSK-02 | Phase 1 | Pending |
| DSK-03 | Phase 4 | Pending |
| DSK-04 | Phase 1 | Pending |
| COPT-01 | Phase 5 | Pending |
| COPT-02 | Phase 5 | Pending |
| COPT-03 | Phase 5 | Pending |
| COPT-04 | Phase 5 | Pending |
| REC-01 | Phase 6 | Pending |
| REC-02 | Phase 6 | Pending |
| REC-03 | Phase 6 | Pending |
| REC-04 | Phase 6 | Pending |
| UI-01 | Phase 1 | Pending |
| UI-02 | Phase 1 | Pending |
| UI-03 | Phase 1 | Pending |

**Coverage:**
- v1 requirements: 27 total
- Mapped to phases: 27 (Phase 7 Productization is a distribution-hardening phase per research; carries no direct requirement mappings)
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-26*
*Last updated: 2026-08-27 (roadmap traceability)*
