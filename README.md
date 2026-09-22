# 极言 NexTalk

AI 跨语言实时面试辅助系统。让你**以母语思考、以本人音色讲出地道英文**——在跨国技术面试中，你讲中文，系统实时翻译并以克隆的你本人音色输出流利英文；手机作为提词器，实时滚动双语字幕并给出基于简历与题库的 AI 回答策略。

**Core Value:** 面试官听到的是无缝衔接的英文回答，你看到的是实时字幕与回答策略——端到端延迟 ≤ 2 秒。

## 功能

- **桌面双窗口**：340×680 微型控制台（中枢：隐形模式、手机配对、知识库、资产入口）+ 860×680 扩展视图（左实时字幕、右 AI 时间线）
- **手机 H5 提词器**：扫码即用（无需安装 App），上半屏双语字幕、下半屏 AI 辅助；支持熄屏保持、断线自动重连续传
- **双向会话控制**：手机「开始提词」与桌面「开始模拟会话」是同一功能，任一端发起，另一端同步
- **AI 面试辅助**：面试官提问实时记录 → 「AI 思考中」过程文字逐步呈现 → 策略要点逐条浮现 → **中英文完整智能回答**逐字打出
- **语言切换**：中 / EN / EN+中 过滤式切换，桌面与手机实时同步
- **模拟会话演示**：四轮确定性模拟面试（全部内容标注「模拟数据」），完整的听题 → 思考 → 回答节奏
- **厂商实验框架**（`tools/vendor-experiments/`）：STT A/B 协议、克隆盲听测试集、RTT 测量工具——零密钥政策，实验结论决定真实管线选型

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面壳 | Tauri 2.11 + Rust（axum LAN 服务器、tokio 广播、确定性模拟引擎） |
| 桌面 UI / 手机 H5 | React 19 + TypeScript + Vite 7（同一套前端，safari15 目标，macOS 12.7 可用） |
| 跨端同步 | 局域网 WebSocket（128 位配对 token 门禁），单事件模型双传输 |
| 契约 | pnpm workspace：`packages/protocol`（闭合联合类型 + 运行时守卫）、`packages/design-tokens`（新粗野主义设计令牌） |
| 测试 | Vitest · Playwright（32 项 e2e）· cargo test（34 单测 + 4 集成） |

## 快速开始

前置：macOS 12.7+（Monterey 兼容）、Node 20+、pnpm、Rust 工具链（rustup）。

```bash
pnpm install
pnpm --filter @nextalk/desktop dev:tauri   # 启动桌面应用（双窗口）
```

桌面控制台会显示配对二维码，手机同 Wi-Fi 扫码即进入提词器。

测试：

```bash
pnpm -r test                                  # 全部单元测试
pnpm exec playwright test                     # e2e（含模拟会话全流程）
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 仓库结构

```
apps/desktop/         Tauri 桌面壳（Rust src-tauri/ + React UI）
apps/teleprompter/    手机 H5 提词器
packages/protocol/    线协议：ServerEvent/ClientMessage 联合类型 + isServerEvent 守卫
packages/design-tokens/ 新粗野主义设计令牌 + Tailwind 预设
e2e/                  Playwright 全流程测试
tools/vendor-experiments/ 供应商实验框架（零密钥）
.planning/            开发规划档案（ROADMAP / 阶段计划 / 验证报告）
```

## 当前状态

- **Phase 1 完成**：全 UI + 模拟会话演示（5/5 计划，自动化测试全绿，人工 UAT 通过）
- **Phase 2 进行中**：真实云管线（STT→翻译→克隆 TTS）+ 延迟测量装置；供应商选型由 `tools/vendor-experiments/` 的 A/B 实测决定

## 隐私

- 纯本地工具：录音、逐字稿、复盘报告均不传任何自有云端；v1 无后端
- AI 能力全部走云端 API 调用时最小化数据暴露；实验框架不提交任何密钥（`.env` 不入库）
