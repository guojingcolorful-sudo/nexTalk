---
status: resolved
trigger: "手机端未同步实时内容：桌面双栏在跑模拟会话（字幕+AI 策略）时，手机提词器收不到任何实时内容。手机端不能启动内容：手机点「开始提词」无法启动会话（SYNC-01 手机→桌面 start_session 往返失效）。实时复现中。"
created: 2026-09-30T12:33:11+08:00
updated: 2026-10-03T21:05:00+08:00
---

## Current Focus
<!-- OVERWRITE on each update - reflects NOW -->

reasoning_checkpoint:
  hypothesis: "R1 = aiTabItems 无按 id 去重、渲染 key=item.id（TeleprompterPage.tsx:128-146/:300）：同一 interviewer id 的多条事件 → 重复 React key → 列表异常。判决：**缺陷为真、但不是本轮实机冻结的原因**。partial 序列下生产 bundle 堆出 3 张同 key 卡（dev build 直接告警并丢卡）；但真实 sim 每问只发一条 final、id 每轮唯一 ⇒ 现场不可能出现同 id 多事件。R2（视口/锚点）在全部 4 个模型下被证伪：新问题卡始终在视口内。"
  confirming_evidence:
    - "partial 实验（生产 bundle，390×664）：同 id 3 条 partial → qCards=3，全为同一 key；列表本身继续更新、最新项可见（无冻结）。dev build 红测：React 打印 `two children with the same key, r1-q` 并少渲染一张 → 未定义行为已实证。"
    - "真实 sim 形态（4 轮、单 final、唯一 id）10/10；忠实旅程（掉线重连尾回放、stop→start 换 epoch 复用 id、会话中再掉线）12/12 —— 每一问的最新卡都可见。"
    - "source.rs:80-118 每轮仅一条 interviewer final（seq=1，SimSource 再统一重编号）；打断/重听（唯一的重发来源）未注册为 Tauri 命令，UI 无入口 ⇒ 现场无 partial。"
  falsification_test: "若真实 sim 序列在真实浏览器上产生同 id 多卡或任何一步最新卡不可见，则 R1/R2 成立为现场原因。两者均未发生。"
  fix_rationale: "R1 作为**已证实的潜在缺陷**按 TDD 修复：toAiTabItems 按 id 首次占位、后续载荷原位覆盖（记录语义、key 稳定唯一），并补 streaming 单测；Phase 2 真实 STT 必然发 partial，届时这是常态。此修复**不声称**解决本轮实机残留（见 checkpoint）。"
  blind_spots: "（1）真机是 iOS Safari/WKWebView，chromium 无法覆盖 iOS 的 scrollIntoView/ResizeObserver 差异；（2）未取到真机 DOM/网络证据——无法排除手机页面仍是 10:21 之前的旧 bundle（页面 JS 只在导航时加载，长期驻留的页面不会自动更新）；（3）运行中的桌面进程镜像（inode …294）早于磁盘上的最新二进制（inode …751）⇒ C 是否在运行镜像中未经证实，且全日志无 `lagged` 行 ⇒ C 从未被现场触发。"
checkpoint: "2026-10-03 21:05 — 残留缺陷按 TDD 修复并自验完成（commit 9fa65fb，仅 teleprompter 两文件）。**实机残留（AI Tab 不实时更新面试官提问）在本轮 4 个模型下均无法复现**：可复现的只有 partial 重复卡（已修）。另发现环境再次变化：Mac Wi-Fi IP 变为 192.168.9.107（旧 IP 全部失效）、手机当前无连接 → 实机复验前必须重新配对。"
next_action: "返回 CHECKPOINT REACHED（human-verify）：① 手机重新配对到新 IP 192.168.9.107（二维码可能仍是旧 IP 快照 → 视需要让控制台重挂载或重启桌面应用；重启会把 C 带进运行镜像，代价 = 新 token + 重新扫码）；② 用新 URL 打开手机页面（自动获得 index-Vc4QBp_y.js = R1 修复后的 bundle）；③ 桌面跑一轮模拟会话，核对 AI Tab 实时性 / 字幕同步 / 策略到达居中；④ 若 AI Tab 仍冻结而字幕正常，回报并考虑先重启桌面再复测。"

fix_plan: "已完成的历史计划（A/B 落 useWs.ts、C 落 lan/server.rs；均零协议变更）—— 实现细节见 Resolution/Evidence。遗留：desktop 的 toTimelineItems 同型去重缺口与 iOS 真机覆盖见 Hardening Backlog。"

## Symptoms
<!-- Written during gathering, then IMMUTABLE -->

expected: 桌面双栏跑模拟会话（字幕 + AI 策略）时，手机提词器应实时收到字幕与 AI 策略流；手机点「开始提词」应经 SYNC-01 往返启动会话（手机 start_session → 桌面响应）。
actual: 手机提词器收不到任何实时内容；手机点「开始提词」无法启动会话。手机已配对成功、WS 已连（日志 `[lan] phone connected (1 total)`），但连接后无内容流动。
errors: 无显式错误输出；/tmp/nextalk-dev.log 中 phone connected 之后无相关错误行。
reproduction: 1) `pnpm dev:tauri` 开发模式运行中（后台任务）；2) 手机经局域网 IP 打开 H5 并配对（已达成，phone connected）；3) 桌面跑模拟会话（字幕+AI 策略）→ 手机无内容；4) 手机点「开始提词」→ 会话不启动。
started: 当前开发会话实时复现中；Phase 1 UAT 曾验证通过（UAT-5 手机↔桌面 start_session 双向、SYNC-03 手机语言模式 push → 桌面应用）→ 疑似回归，优先差分定位 Phase 1 之后的代码变更。

### 补充症状 S2（2026-09-30 用户补充，原 Symptoms 块保持不变）

expected: 手机端「AI 辅助」Tab 应实时展示最新内容（如面试官新问题）——桌面 AI 时间轴的约束为：按到达顺序交织「上下文问题节点 + 策略卡」，最新项居中。
actual: 手机 AI Tab 停留在旧回答卡上，未随新事件更新/刷新（也未见按最新项重新锚定）。
reproduction: 桌面跑模拟会话产生新的 interviewer 问题/策略卡 时，观察手机 AI Tab。
linkage: 可能与 S1（事件不同步）同一根因——事件根本没到手机（无事件 ⇒ 无更新）；也可能是独立的行为缺陷（事件到了但 AI Tab 未按新问题刷新/锚定）。
related_code: apps/teleprompter/src/pages/TeleprompterPage.tsx（aiTabItems = interviewer subtitle + strategy 交织；lastAiItemId / useCenterAnchor 锚定；isAiThinking）；桌面对照 apps/desktop/src/components/AiTimeline.tsx。

## Environment Notes

- 【2026-09-30 ~13:09 环境变化】Wi-Fi 切换至新网络「603」：桌面 LAN IP 192.168.110.198 → 192.168.1.10（手机旧地址一度打不开）；桌面应用已重启：新进程 PID 33695（旧 PID 28412 已终止）、新 pairing token/凭证、二维码重新出现；用户已重新扫码（日志第 54 行 = 重启后第三次 `[lan] phone connected (1 total)`；14:28 时 PID 33695 运行正常）。后续验证一律用新 IP / 新进程；12:44 前的 netstat/curl 等观测保留为历史。
- 应用当前以 dev 模式运行：后台任务日志 /tmp/nextalk-dev.log；进程 `target/debug/nextalk-desktop`（PID 28412，12:44 时已运行 26:48）；`tauri dev` 含 BeforeDevCommand：desktop vite dev（1420）+ teleprompter `vite build --watch`（PID 28394，产物 dist/）。
- 端口占用（勿误判）：用户其他项目（career-ops-china）的孤儿进程 `node tools/jd-inbox-server.mjs`（PID 82377）占用 127.0.0.1:8787；桌面 LAN 服务器绑 *:8787（0.0.0.0）。手机走局域网 IP 不受影响（H5 可加载、配对成功）。**禁止未经用户同意 kill PID 82377。桌面 localhost 自测会命中错误服务——验证一律用局域网 IP。**
- H5 经局域网 IP 可正常加载并渲染未配对态；/ws 鉴权行为正确（无 token → 400，错 token → 401）。
- 手机加载的是 `vite build --watch` 产物（需确认是否最新构建——H4）。→ 已确认：与当前 dist 逐字节一致（见 Evidence）。
- 验证手段：可用 Playwright（playwright-core 位于 /Users/guojing/nexTalk/node_modules/.pnpm/playwright-core@1.53.2/node_modules/playwright-core）驱动 H5，用真实 pairing token 配对；token 可从 pairing_token() 生成逻辑获取。注意：本机 ms-playwright 缓存只装了 chromium-1179（无 webkit），且桌面控制台截图被 macOS 隐私脱敏（无法读二维码）。
- 约束：修改产品代码或临时加日志前，必须返回 CHECKPOINT REACHED（decision）由会话管理器向用户确认。
- 本轮新增可复用工具（已停服，需要时重启）：`node /tmp/fake-phone/server.mjs`（真实 dist 静态服务 + mock WS，绑 127.0.0.1:8899，**从不触碰 8787**）与 `node /tmp/fake-phone/run.mjs`（playwright chromium 驱动，10 项断言覆盖健康路径 / M2 冻结 / 标记恢复）。H5 支持 `?token=..&ws=..&tab=ai` 覆盖 WS 地址（App.tsx:15-19），因此该实验**不需要真实 pairing token**。
- token 无法离线获取：pairing token 仅存于进程内存（state.rs:60-68），前端无 localStorage/持久化，桌面 webview 数据目录 ~/Library/WebKit/nextalk-desktop 无相关条目，日志也从不打印配对 URL。真机实验需用户提供 token，或批准临时诊断日志。
- 【2026-10-03 21:05 环境再变化】Mac en0 IP 从 192.168.1.10 变为 **192.168.9.107**（又一次 Wi-Fi 换网）：旧 IP 全部不可达；`http://192.168.9.107:8787/` 返回 200，index.html 与磁盘 dist 逐字节一致（SHA1 cf9902d2…，引用 index-Vc4QBp_y.js = R1 修复后的 bundle）；/ws 无 token → 400（鉴权正常）。桌面仍是 **PID 41473（10-01 10:14:22 启动至今未重启）**、token 未轮换；netstat 无任何到 8787 的 ESTABLISHED → 手机当前未连接。`pairing_url()` 在 invoke 时用 `local_ip_address::local_ip()` 现算，但 QrCodeCard 只在挂载时 invoke 一次（QrCodeCard.tsx:35-49，仅 `attempt` 重试）→ 控制台二维码很可能仍是 10-01 挂载时的旧 IP 快照。
- 【2026-10-03 21:05 运行镜像判定】C（服务端 Lagged 自愈）**不在运行镜像中**：进程启动 10:14:22 早于 C 的实现（~10:20）与提交（10:33:29）；且运行镜像 inode 13037343294 ≠ 磁盘二进制 inode 13037345751（磁盘二进制含 `resending timeline` 字符串）。A+B 由磁盘 dist 提供 → 任何新加载的页面都会带上。要让 C 生效必须重启桌面应用（新 token → 重新扫码）。
- 【2026-10-03 21:05 残留服务】本轮自建 mock 服务器仍在运行且与产品无关：`node /tmp/fake-phone/server.mjs`（PID 43712，127.0.0.1:8899）与 `node /tmp/fake-phone/server2.mjs`（PID 45981，127.0.0.1:8901）；**从不绑定 8787**，需要时可直接复用或 kill。dev 环境（vite watcher 33652/33661、桌面 41473、node 82377）一律未动。

## Eliminated
<!-- APPEND only - prevents re-investigating -->

- hypothesis: "H4：手机加载的是旧构建产物（页面/缓存里的旧 bundle 导致手机端根本没有发送逻辑）"
  evidence: "12:44 经局域网 IP `curl http://192.168.110.198:8787/` 取回 index.html 指向 `assets/index-Ch2A0d-y.js`；该 JS 的 SHA1 与 `apps/teleprompter/dist/assets/index-Ch2A0d-y.js` 完全一致（f8ded3e296896a8e933aa18e7557cbeeb84a9c10, 295119 字节），而 dist 由当前干净工作树构建（git diff 无代码改动）。且 token 每进程轮换，手机能通过 401 校验就证明其 URL 来自当前进程运行期。"
  timestamp: 2026-09-30T12:45+08:00
- hypothesis: "服务端协议/读循环/广播扇出断裂（resume 或 control 帧解析失败导致读循环 break、或广播任务死亡）"
  evidence: "cargo test 全绿：34 unit + 4 integration，含 `full_demo_session_reaches_the_phone_and_applies_the_language_control`（真实 router + 真实 WS 客户端收全量事件）、`phone_control_action_starts_the_session_over_the_wire`（手机 control 帧真的启动会话）。另核对了 Rust serde 镜像：`Resume { since_seq, #[serde(default)] since_epoch: Option<u64> }`、`Control { #[serde(default)] language, #[serde(default)] action }`（variant 级 camelCase 重命名），手机实际发送的 `{t:'resume',sinceSeq:0,sinceEpoch:0}` 与 `{t:'control',action:'start_session'}` 均可解析（server.rs:628 单测覆盖）。且日志中只有 2 行 connected、无重连风暴 → 不存在反复解析失败导致的静默断连循环。"
  timestamp: 2026-09-30T12:41+08:00
- hypothesis: "客户端点击处理器抛异常 / useWakeLock 先抛错导致 sendSessionAction 根本没执行"
  evidence: "通读 useWakeLock.ts：activate() → wakeLockApi() 在非安全上下文返回 null → startVideoFallback()；整条路径（createElement/appendChild/play().catch/两个 setState/onFallbackEngaged?.()）无任何 throw 分支。TeleprompterPage.test.tsx:115「starts the session on tap and flips the gate to 暂停提词」证明 OPEN 状态下点击确实发出 `{t:'control',action:'start_session'}`（唯一 action 帧）。"
  timestamp: 2026-09-30T12:45+08:00
- hypothesis: "服务端写入路径卡死 / 发送队列无界积压（socket 半死导致 send 阻塞）"
  evidence: "12:44 netstat -anv：手机（192.168.110.205）两条 ESTABLISHED 连接 Recv-Q=0、Send-Q=0（无未确认积压），无 CLOSE_WAIT；此前 12:35-12:38 轮询同样 Recv-Q=0。"
  timestamp: 2026-09-30T12:44+08:00
- hypothesis: "手机点了『开始提词』但帧到达服务端后被状态机拒绝（会话已在跑导致 start_session 返回 Err）"
  evidence: "server.rs:263 的 `eprintln!(\"[lan] phone requested start_session\")` 在 match 状态检查**之前**无条件执行；该行在整个日志（53 行）中从未出现，`phone requested stop_session`（279 行）同样从未出现 → 任何控制帧都没有到达读循环，不是被状态机拒绝。"
  timestamp: 2026-09-30T12:40+08:00

- hypothesis: "H6（=B2）：独立 AI-Tab 行为缺陷——事件已到达手机，但 AI Tab 不按新问题刷新/锚定（memo 依赖、闭包、键冲突、锚点钉死在旧项、isAiThinking 卡死）"
  evidence: "代码层无缺陷（aiTabItems 纯派生 + accept() 每次新建数组；锚点 active 值随 lastAiItemId 变化；key 无碰撞；isAiThinking 逐事件全量重算），并由真实浏览器复现证伪：真实 dist + Playwright 的 10/10 断言显示，新 interviewer 问题到达必定立即出现节点与思考中、strategy 到达必定替换思考中并把最新卡居中（centerY 414 vs 视口中心 434）。→ 症状只能来自『事件未进入 state』，不是 AI-Tab 自身的渲染/锚定缺陷。"
  timestamp: 2026-09-30T12:52+08:00

- hypothesis: "M1：手机 WS 静默死亡/半开（作为当前持续态 → 无任何帧到达）"
  evidence: "用户判别测试 ①（不碰手机，桌面开始→停止会话）：手机底部按钮自行翻转（暂停提词 ⇄ 开始提词）→ 帧能到达手机 → 当前不存在『零帧』状态，M1 排除。（原始症状 #2『点击无效且日志无 phone requested 行』的机制另行成立：useWs.ts:207-214 非 OPEN 时静默丢弃 + 无重放，由修复 A 覆盖。）"
  timestamp: 2026-09-30T14:28+08:00

- hypothesis: "R1（重复 React key）是**本轮实机残留**（AI Tab 不随面试官提问更新）的原因"
  evidence: "R1 缺陷本身被实证（partial → 生产 3 张同 key 卡；dev 告警丢卡），但真实 sim 每轮仅一条 final、id 唯一、重发路径未注册为命令（source.rs:60-127 / lib.rs）→ 现场 events 中不可能出现同 id 多事件；且 4 个忠实模型每一步 AI Tab 都在更新、最新卡均可见，与『完全不动』不符。R1 降级为 Phase 2 潜在缺陷（已按 TDD 修复：commit 9fa65fb）。"
  timestamp: 2026-10-03T20:20+08:00

- hypothesis: "R2：事件到达且进入 state，但新问题卡落在视口外 / 锚点未重新居中（aiThinking 无 false→true 边沿、ResizeObserver 未触发）→ 用户看到『不动，然后一次性展示』"
  evidence: "4 个模型的逐步 boundingRect 测量（390×664 真 viewport）：run-residual2 10/10、run-residual3 12/12、partial 实验（修复前）—— 每一问与每一策略到达后最新卡均与视口相交；tab 切换往返后再次可见。证伪。"
  timestamp: 2026-10-03T20:24+08:00

- hypothesis: "Priority 2：resume/epoch 回放路径存在缺口（回放不含 session_started → 客户端游标不重置）"
  evidence: "读 state.rs + server.rs 的 resume 分支：epoch 不符 ⇒ 全量 timeline（含标记）；sinceSeq==0 或 > 最高 ⇒ 全量；否则仅尾部切片。harness 12/12 实测含空 timeline 帧与 epoch 全量回放。核验通过、无缺口 → 不需要 Rust 改动（避免 tauri dev 重建 → token 轮换 → 重新扫码）。"
  timestamp: 2026-10-03T20:24+08:00

## Evidence
<!-- APPEND only - facts discovered -->

- timestamp: 2026-09-30T12:20+08:00
  checked: /tmp/nextalk-dev.log
  found: `[lan] phone connected (1 total)`；此前无 WS 错误/400/401
  implication: 手机配对鉴权成功、WS 连接已建立（Phase 1 配对链路正常）——问题在连接之后的事件流动/控制流
- timestamp: 2026-09-30T12:20+08:00
  checked: 端口占用情况
  found: 127.0.0.1:8787 被非本项目 node 进程（PID 82377, career-ops-china）占用；桌面 LAN 服务绑 0.0.0.0:8787
  implication: localhost 访问命中错误服务；手机走局域网 IP 正常——已排除「手机连错服务」；桌面 localhost 自测结论不可信
- timestamp: 2026-09-30T12:20+08:00
  checked: 手机 H5 经局域网 IP 加载 + /ws 鉴权
  found: 页面正常渲染未配对态；无 token → 400、错 token → 401
  implication: H5 静态资源与 WS 鉴权路径正常；问题在配对成功后的事件流/控制流
- timestamp: 2026-09-30T12:20+08:00
  checked: Phase 1 已验证语义（历史）
  found: UAT-5（手机 start_session ↔ 桌面 start_session 双向）、SYNC-03（手机语言模式 push → 桌面应用）曾通过
  implication: 曾工作的链路 → 怀疑 Phase 1 完成后引入的回归（differential debugging）
- timestamp: 2026-09-30T12:35+08:00
  checked: 差分分析：`git log ffd5079..HEAD -- apps/desktop/src-tauri/src apps/teleprompter/src packages/protocol/src`
  found: 空；工作树对代码无改动（仅 .planning 文档未跟踪/改动）
  implication: 「Phase 1 之后的代码回归」假设被削弱——当前代码与当时通过 UAT 的代码一致，故障更可能是环境/状态相关而非代码变更
- timestamp: 2026-09-30T12:36+08:00
  checked: 60 秒 × 3 秒轮询 netstat + 日志行数/大小（/tmp/watch-conns.sh）
  found: 日志新增第 53 行，仍为 `[lan] phone connected (1 total)`（两行都写 1 total → 期间计数曾回到 0，即第一条连接确已断开且被应用察觉）；同一窗口手机 socket 集合 2 → 5 → 2（形态符合一次页面加载：多路并发资源 + 新 WS）；**全程从未出现 `[lan] phone requested start_session`**
  implication: 手机侧确有一次重连/重载（~12:36）；但用户的点击从未把控制帧送达服务端读循环
- timestamp: 2026-09-30T12:38+08:00
  checked: ping 手机（192.168.110.205）+ 桌面进程线程采样（sample 28412）
  found: ping 0% 丢包（22-62ms）；主线程空闲于 NSApplication run loop，tokio worker 全部 parked，采样窗口内无 sim/state/lan 帧
  implication: 弱证据：采样窗口内 100ms 模拟调度器未在活跃产出（与「采样时没有会话在跑」一致），网络链路健康
- timestamp: 2026-09-30T12:41+08:00
  checked: cargo test（apps/desktop/src-tauri，后台任务 bkchnk30d）
  found: 34 passed / 0 failed；integration 4 passed / 0 failed（session_integration.rs 全绿，含手机控制帧启动会话、断线手机重连恢复、手机计数）
  implication: 桌面侧「读循环 → start_session → 广播 → 写回 phone socket」全链在进程内被真实 WS 客户端验证为正确 → 断点只能在手机侧或其 socket 的存活状态
- timestamp: 2026-09-30T12:44+08:00
  checked: 服务端实际分发的产物（curl LAN IP，规避 127.0.0.1:8787 的错误服务）
  found: 服务端返回的 index.html 引用 index-Ch2A0d-y.js，其 SHA1 与本地 dist 一致（f8ded3e2…）
  implication: 手机运行的就是当前源码构建的 bundle；排除「旧构建/旧逻辑」分支
- timestamp: 2026-09-30T12:45+08:00
  checked: 客户端发送路径与 UI 反馈（useWs.ts:207-214、TeleprompterPage.tsx:201-216、useWs.test.tsx / TeleprompterPage.test.tsx）
  found: `sendSessionAction` 仅在 `socket.readyState === WS_OPEN` 时发送，否则**静默 return**（注释「re-sent by onopen」只对语言偏好成立，动作没有 pending 队列）；`toggleSession` 在发送前就 `setSessionActive(true)`（乐观点亮 暂停提词）；语言偏好有 `languageRef`+onopen 重放（WR-03），会话动作**没有**对应机制
  implication: 「点击无任何反馈、无日志、无帧」与「非 OPEN 时静默丢弃」完全吻合；且 UI 会显示会话已开始，用户会以为已启动
- timestamp: 2026-09-30T12:46+08:00
  checked: 前端单测（vitest run，apps/teleprompter）
  found: 4 个测试文件、43 个用例全部通过（含 useWs.test.tsx 的 resume/重连/去重与 TeleprompterPage.test.tsx 的「点击 → 开始提词 帧」「暂停提词 → stop_session 帧」）
  implication: 客户端 happy path（socket OPEN 时点击必发帧）、断线重连阶梯、resume 游标全部为绿色基线 → 现场故障不是这些已覆盖路径，而是「socket 非 OPEN 时点击」这一未覆盖的空窗

- timestamp: 2026-09-30T12:50+08:00
  checked: 只读审计 AI-Tab 数据链：TeleprompterPage.tsx（aiTabItems :128-146 / lastAiItemId :161 / isAiThinking :80-92 / useCenterAnchor 调用 :230-232）、hooks/useWs.ts（accept 去重 :100-129、sendSessionAction :207-214）、hooks/useCenterAnchor.ts、packages/protocol/src/index.ts（isServerEvent）、apps/desktop/src/components/AiTimeline.tsx（toTimelineItems/isAiThinking）
  found: aiTabItems 纯派生自 events（useMemo [events]），accept() 是唯一写入点且每次新建数组；isServerEvent 正确识别 session_started（:117-118）；桌面 toTimelineItems 与手机 aiTabItems 过滤/交织语义一致；未发现 memo/闭包/键/锚点/思考态缺陷。唯一「socket 健康也能冻结内容」的机制：两组去重游标只在 session_started 时清空（useWs.ts:104-122），而 strategy id 每会话复用（script.rs:190 s-r1..s-r4）、subtitle seq 每会话从 1 重数（source.rs:305-311）。
  implication: B2（独立 AI-Tab 缺陷）在代码层不成立；症状只可能来自「事件未进入 state」——M1（无帧）或 M2（帧到但被游标吞）。

- timestamp: 2026-09-30T12:52+08:00
  checked: Playwright 假手机实验（真实 dist 产物 + /tmp/fake-phone 自建 mock WS 服务器，127.0.0.1:8899；脚本 /tmp/fake-phone/server.mjs、run.mjs，10 项断言全绿）
  found: 健康路径：resume 帧发出；OPEN 时点击发出 `{t:'control',action:'start_session'}`；新 interviewer 问题到达 → AI Tab 立即出现「面试官提问」+ 思考中；strategy 到达 → 思考中消失、最新卡居中（centerY 414 vs 视口中心 434）；到达序交织 q1>s1>q2>s2 正确。M2 复现：socket 健康下补发整轮新会话（同时含 subtitle seq 1.. 与 strategy s-r1/s-r2）但不发 session_started → AI Tab 4 项 → 4 项、零更新（完全冻结），同时 status 帧到达使底部按钮翻成「暂停提词」。恢复：补发 session_started → 游标重置、内容恢复；页面刷新 + resume 全量回放同样恢复。
  implication: 用户新症状（AI Tab 停在旧回答卡、不随新事件更新）被 M2 机制逐字复现；M1 vs M2 的用户可见判别信号确立（不碰手机时，桌面开始/停止会话是否让手机按钮自行变化）；「刷新手机页面即恢复」可立即作为用户的应急自愈手段。服务端 Lagged 丢帧（server.rs:320-324，丢帧后静默继续）是 M2 最现实的触发路径 → 修复方案新增 C 项。

- timestamp: 2026-09-30T14:28+08:00
  checked: 用户判别测试 ①（不碰手机，桌面开始→停止模拟会话，观察手机底部按钮）
  found: 按钮自行变化（暂停提词 ⇄ 开始提词）→ 帧到达手机；M1（零帧死连接）排除
  implication: 根因方向确认为 M2（内容帧被客户端游标去重丢弃、status 帧照常到达）；修复核心 = B+C
- timestamp: 2026-09-30T14:28+08:00
  checked: 用户自愈观察 ②（刷新手机页面）
  found: 内容恢复（resume 全量回放，最后一条缓慢重渲染出现）
  implication: 「刷新页面（新页面 sinceEpoch=0 → 服务端全量回放含 session_started）」确实重置游标并恢复内容——既印证 M2，也作为用户侧应急手段记录
- timestamp: 2026-09-30T14:28+08:00
  checked: 环境变化与日志核对（grep `lagged` / `phone connected`；进程 33695）
  found: 日志 54 行；第 54 行 = 重启后手机重新扫码连接（第三次 `[lan] phone connected (1 total)`）；无 `lagged` 匹配（server.rs:322 实际措辞为 `lan ws: phone lagged, dropped N event(s); resuming`）；新进程 33695 运行正常
  implication: T2（Lagged 丢帧）未被现场日志坐实，但代码路径客观存在；T1 结构性隐患仍在 → B+C 覆盖两种触发（协调者决定不做进一步现场钉死）

- timestamp: 2026-10-01T10:07+08:00
  checked: 红阶段 ①（前端）：在 useWs.test.tsx 追加 A 组 3 例 + B 组 3 例（导入尚未导出的 WATCHDOG_SILENCE_MS / WATCHDOG_PROBE_GRACE_MS），在未改产品代码前运行 `pnpm --filter @nextalk/teleprompter test`（vitest 4.1.11）
  found: 4 failed | 45 passed（21 tests in useWs.test.tsx，4 个新失败全部与新增用例对应）：
    - A「a tap while the socket is down is flushed on open」/ B「a silent socket gets one resume probe」→ SyntaxError: "undefined" is not valid JSON（useWs.ts 尚无 pendingActionRef/看门狗，亦无这两个导出常量 → 期望帧不存在）
    - A「queued taps are last-wins and are not replayed after delivery」→ AssertionError: expected [] to have a length of 1 but got +0（非 OPEN 点击被静默丢弃，无任何 action 帧发出）
    - B「a reply to the probe keeps the socket and re-arms the silence window」→ expected 1 to be 2（无探测机制 → resume 帧数不增）
    全部 45 个既有用例保持通过 → 红只落在新增行为上。
  implication: 红阶段确认 A/B 三项行为（动作队列 flush、静默探测、探测应答续期）在当前实现中完全缺失；既有 45 例为绿基线不受影响。
- timestamp: 2026-10-01T10:08+08:00
  checked: 红阶段 ②（后端）：在 server.rs 追加 `a_lagged_receiver_is_resent_the_whole_timeline`（溢出 64 槽环形缓冲后断言 forwarder 首帧必须是整条 Timeline 快照），在未改 forward_events 前运行 `cargo test ... a_lagged_receiver_is_resent_the_whole_timeline`
  found: 1 failed | 0 passed（34 filtered out）。失败信息并非 5s 超时，而是更快、更有信息量的断言：`panicked at src/lan/server.rs:591: the recovery frame must be the whole-timeline snapshot, got Subtitle { id: "stale-17", ... seq: 17 }`，同批 stdout 为 `lan ws: phone lagged, dropped 16 event(s); resuming`。
  implication: 直接坐实 C 的缺陷形态——Lagged(16) 后 forwarder 从环形缓冲里最老的幸存帧 stale-17 继续发送，被丢的 1..16 帧（session_started 所在位置）永久消失；客户端若正好错过标记则整轮内容被游标吞掉。红阶段完成（前端 4 例 + 后端 1 例）。

- timestamp: 2026-10-01T10:20+08:00
  checked: 绿阶段 ①（前端）：实施 A（pendingActionRef last-wins + onopen 在 resume/language 之后 flush）与 B（纯定时器链看门狗，无 Date.now 依赖）后运行 `vitest run`（apps/teleprompter）
  found: 4 files / 49 tests 全绿（红阶段 4 个失败全部转绿，既有 45 例不变）。覆盖：非 OPEN 点击 → open 后按 resume→language→action 顺序 flush；last-wins 且送达后不重放；OPEN 直发；静默 T1 发一次探测（advanceTimersByTime(T1-1) 不触发、再 +1 触发）；grace 内应答 → 不关闭且续期；持续入站帧不触发探测；无应答 → close → 1s 阶梯新 socket + resume。
  implication: A/B 行为在单测层确立（含时序边界）；看门狗为纯定时器链，页面从挂起恢复时过期定时器立即触发 → 秒级自愈（真机待验）。
- timestamp: 2026-10-01T10:22+08:00
  checked: 绿阶段 ②（后端）：实施 C（Lagged → 向该客户端补发 `Timeline{events: state.timeline()}`，复用既有帧类型）后运行 `cargo test` + `cargo clippy --all-targets` + `cargo fmt --check`
  found: 35 unit + 4 integration 全绿（新用例 `a_lagged_receiver_is_resent_the_whole_timeline` 转绿；`a_lagged_receiver_keeps_forwarding` 随新签名更新后保持通过）；clippy 无警告、fmt 干净。
  implication: 服务端不再静默吞掉环形缓冲头部帧；与 C 的可证伪点一致（客户端确实消费快照内的 session_started → 由 e2e 真浏览器验证）。
- timestamp: 2026-10-01T10:26+08:00
  checked: 首轮 e2e 15/19 的 4 个失败归因（读 mock 服务器 stdout + 客户端帧日志时间线重放）
  found: 4 项全为驱动脚本缺陷、与产品无关：① B 的『已关闭』判定 `countEvent('close')>0` 被先前页面刷新的 teardown close 提前满足，且拿旧 close 时间戳与更晚的帧比较；② 驱动在 grace 窗口内自行 push 帧 → 被客户端计为入站 → 看门狗续期，预期中的 close 被取消（客户端行为正确）；③ A 的断言取自全量日志（匹配到 reload 的 open 与阶段一的 action 帧），`flushed` 谓词命中阶段一旧帧后立即返回 → 运行在 3s hold 未到期时就结束，page2 的升级被中止。修正（仅 harness）：断言按 `ts > clickTs` 限定 + 用计数增量（closesBefore+1 / opensBefore+1）+ grace 窗口内不 push + mock 不再把 timeline 帧并入自身时间线（与真实服务端一致：恢复快照是单客户端直发、不进 timeline）。
  implication: 无需产品改动；修正后的 harness 可信。产品侧三条新行为在首轮中其实已通过（B 的探测、C 的恢复），仅断言失焦。
- timestamp: 2026-10-01T10:30+08:00
  checked: 绿阶段 ③（真浏览器 e2e，/tmp/fake-phone = 真实 dist 产物 + mock WS，chromium 全 20 项断言，重跑于修正后的 harness）
  found: 20/20 通过。关键时序（epoch ms）：静默后 ~10s 发出探测 resume（resumes 2→3）且仍在同一连接（opens 2→2）；closeTs−lastInboundTs≈20.85s（T1+grace 如期）→ 1s 后新 open（opens 2→3）→ 补发内容即刻渲染到 AI Tab（q3 + 思考中）；A 中 holdTs=…797645 > page1CloseTs=…797450（被延迟的正是 page2 自己的升级）、clickTs=…797994 时 framesBefore=framesAfter=8（CONNECTING 期间零帧外发）、openTs=…800657（点击 +2.66s ≈ 3s hold 到期）、actionTs=openTs+3ms、order=[resume,bilingual,start_session]、actionFrames=1。M2 冻结复现（无标记 → 4 项不变、按钮仍翻成暂停）与 C 快照恢复（Timeline → 游标重置 → 恢复渲染）同样通过。
  implication: A+B+C 在真实浏览器 + 真实产物上端到端成立；M2 冻结仍是可复现基线（修复未使其失真——单测保证 accept() 的去重语义不变）。

- timestamp: 2026-10-01T10:34+08:00
  checked: 实机旁证（/tmp/nextalk-dev.log；当前 dev 运行 = 重建后 PID 41473；工作树已提交 11a2160 + 3d60c90；dist/index-Cab59miL.js 构建于 10:21:10，晚于 useWs.ts 最后修改 10:11:41；netstat：手机 192.168.1.3 ESTABLISHED ↔ 192.168.1.10:8787，进程 41473）
  found: 新进程日志末尾依次为 `[lan] phone connected (1 total)` ×4、`[lan] phone requested stop_session`、`[lan] phone connected (1 total)`、`[lan] phone requested start_session` —— 症状 #2 的判别行（此前整个日志从未出现）已在真机出现；手机已重连到重建后的新进程（新 token，无需再扫码）。
  implication: 「手机点击到达桌面读循环」在真机上已恢复。注意：A+B 需手机加载 10:21 之后的 bundle（刷新一次页面即可确保）；实时内容与 ~20s 自愈两项留给检查点的实机复验。

- timestamp: 2026-10-03T20:12+08:00
  checked: R1 判别实验（/tmp/fake-phone/server2.mjs 绑 127.0.0.1:8901，chromium 390×664，真实 dist；同一 question id 连发 3 条 partial：seq 1→2→3，final:false）
  found: 生产 bundle 渲染 **qCards=3**（同一 key `r1-q` 的三张「面试官提问」卡同时存在）且页面控制台无任何警告；列表本身未冻结（后续 strategy/下一问仍追加、最新项可见）。dev build（vitest/jsdom）对照：React 打印 `Warning: Encountered two children with the same key, r1-q` 且只渲染 2 张 → 生产/开发行为不一致，重复 key 已实证为未定义行为。
  implication: R1 是**真实缺陷**，但触发前提是同一 interviewer id 的多条事件。
- timestamp: 2026-10-03T20:16+08:00
  checked: 现场是否可能产生同 id 多事件（读 apps/desktop/src-tauri/src/sim/source.rs:60-127、sim/script.rs、lib.rs 命令注册）
  found: `round_events()` 每轮只发出**一条** interviewer final（`question_id="{round}-q"`，4 轮 id 唯一 r1-q..r4-q；SimSource 再把 seq 覆盖为会话内单调计数）；strategy `round_id="{id}"`。唯一会重发同 id 的 `interrupt_session_for`/`repeat_session_for` **未注册为 Tauri 命令**（lib.rs 只注册 get_pairing_info / start_session / stop_session）→ UI 无入口。
  implication: **R1 在现网（sim 源）不可触达**，实机 AI Tab 冻结不能用重复 key 解释；R1 是 Phase 2 真实 STT partial 到来时才引爆的潜在缺陷（仍值得修）。
- timestamp: 2026-10-03T20:20+08:00
  checked: R2（视口/锚点）判别：真实 sim 形态 4 轮 ×（question → 450ms → strategy），每步测量最新卡 boundingRect 与滚动容器视口相交性（/tmp/fake-phone/run-residual2.mjs，390×664，tab=ai 全程）
  found: 10/10 通过：每一问的**最新卡在屏幕上可见**（question 步与 strategy 步均 visible），scrollTop 随内容增长。
  implication: R2 证伪 —— 到达的事件总能被用户看见。
- timestamp: 2026-10-03T20:24+08:00
  checked: 忠实旅程模型（/tmp/fake-phone/run-residual3.mjs）：start → 2 轮 → kick（断开）→ 尾回放（sinceSeq/sinceEpoch 语义）→ stop → 重连 → 新会话 epoch+1 且复用 id（r1-q/s-r1）→ round1 → 会话中 kick → round2
  found: 12/12 通过，含 `resume sinceSeq=3 sinceEpoch=2 -> 0 event(s)`（空 timeline 帧被安全处理）与 epoch-mismatch 全量回放（含 session_started 标记）。每一步最新内容可见。
  implication: Priority 2（replay/epoch）**核验通过、无缺口**：服务端 resume 语义与客户端游标严格对应；无需追加 Rust 修复（也就避免了 tauri dev 重建 → 新 token → 用户重新扫码的连带影响）。
- timestamp: 2026-10-03T20:31+08:00
  checked: 在役 bundle 指纹取证（apps/teleprompter/dist/index.html → index-Vc4QBp_y.js，构建于 10:21:10；对照 10:34 实机日志）
  found: minified `dt.onopen` 内含 A 的 pendingAction flush（`const Qt=N.current;Qt!==null&&(N.current=null,el(dt,{t:"control",action:Qt})),tt()`）与 B 的看门狗装配（`tt()`）；C 的日志字符串（`lan ws: phone lagged, dropped … resending timeline`）存在于磁盘二进制。
  implication: 10:34 实机复验时 dist **已含 A+B**；但页面 JS 只在导航时加载 —— 长期驻留的手机页面是否已加载该 bundle **未经证实** → 复验前必须刷新手机页面。
- timestamp: 2026-10-03T20:33+08:00
  checked: 运行中镜像 vs 磁盘二进制 + Lagged 现场痕迹（进程 41473，启动于 10-01 10:14:22）
  found: 进程可执行文件 inode 13037343294 ≠ 磁盘二进制 inode 13037345751（**运行镜像早于最新 relink**）；全日志无任何 `lagged` 行。
  implication: C 是否在运行镜像中**未经证实**，且 C 从未被现场触发；重启桌面会让新镜像带上 C，但会轮换 pairing token（用户需重新扫码）—— 留作检查点的用户决策。

- timestamp: 2026-10-03T20:52+08:00
  checked: 修复后全量自验（commit 9fa65fb 落地后）
  found: vitest **52/52**（4 files；TeleprompterPage.test.tsx 21/21，含新增 streaming 行为单测与 toAiTabItems 纯函数 2 例）；对重建后 dist（hash 不变 index-Vc4QBp_y.js —— 仅格式化时保持一致）跑 harness：run-partial **7/7**（3 partial → 恰好 1 张卡、原位跟随最新、final+strategy 仍 1 张、second/third question 各 1 张且最新可见、零 duplicate-key 警告）、run-residual2 **10/10**、run-residual3 **12/12**。
  implication: R1 修毕且无回归；但**实机残留（AI Tab 不实时更新）在本轮 4 个模型下均无法复现** —— 只能通过 human-verify 收口（先刷新手机页面拿到新 bundle）。
- timestamp: 2026-10-03T20:52+08:00
  checked: 静态检查（tsc / eslint / prettier，apps/teleprompter）
  found: `tsc --noEmit` 报仓库级既有错误（所有 .tsx 都缺 `@types/react` 声明，工作区从未安装）→ 与本次改动无关；eslint 的 2 个 `Calling setState synchronously within an effect` 位于 TeleprompterPage.tsx:200/213（echoedLanguage 效应），在 HEAD 同码位已存在（验证为既有）；prettier 曾重排 3 行既有代码 + 本次新增。
  implication: 无新增静态检查债务；仅格式化（minified 产物 hash 不变）→ 既有 harness 结论不受影响。

## Resolution
<!-- OVERWRITE as understanding evolves -->

root_cause: "两条独立结论：
  (1)【首轮实机故障，已修】M2：客户端两组去重游标仅在 session_started 时重置（useWs.ts accept()），而 subtitle seq 每会话从 1 重数（sim/source.rs:305-311）、strategy id 每会话复用（sim/script.rs:190）→ 会话标记一旦丢失/错过（断线重连窗口；或 forward_events 遇 broadcast Lagged 丢环形缓冲头部、其中正含标记），新会话全部内容帧被判重复而静默丢弃，画面永久冻结在旧回答卡，而 status 帧照常到达（按钮/胶囊看似健康）。触发源由后端红测坐实：Lagged(16) 后 forwarder 从 stale-17 继续，被丢的 1..16（标记所在）永久消失。附带症状 #2（点击无效、日志无 phone requested 行）= useWs.ts 非 OPEN 时静默丢弃动作且无重放。
  (2)【本轮残留，缺陷为真但非现场原因】手机 aiTabItems 对同一 id 的多次事件逐个 push、渲染 key=item.id → partial 序列下生产 bundle 堆出 3 张同 key 卡（dev build 告警并丢卡）。但 sim 每轮仅一条 final、id 唯一 ⇒ 现场不可触达；实机『AI Tab 不实时更新』在本轮 4 个模型下均无法复现 —— 最可疑的环境变量是手机页面可能仍驻留着 10:21 之前的旧 bundle（页面 JS 只在导航时加载；未经证实，由检查点收口）。"
fix: "首轮 A+B+C（零协议变更、无新依赖，commits 11a2160 + 3d60c90）：A = 动作可重放（非 OPEN 点击 → pendingActionRef last-wins，onopen 在 resume→language 后 flush）；B = 存活看门狗（静默 10s 探测 resume，再过 10s 无入站 → close → 阶梯重连 + 全量回放）；C = 服务端 Lagged 当刻向该客户端补发 `Timeline{events: state.timeline()}`。
  本轮残留修复 commit 9fa65fb（仅 teleprompter 两文件）：TeleprompterPage 新增导出纯函数 toAiTabItems —— 按 id 首次占位、后续载荷原位覆盖（对齐桌面 toTimelineItems 的『记录』语义，渲染 key 稳定唯一），替换原内联 useMemo；配套 streaming 行为单测 + 纯函数单测。"
verification: "自验全绿：vitest 52/52（新增 3 例）；真实 dist + chromium harness：run-partial 7/7、run-residual2 10/10、run-residual3 12/12。【待】human-verify：手机刷新页面（必须）→ 核对 AI Tab 实时性 / 字幕同步 / 策略到达居中（见检查点）。"
files_changed:
  - apps/teleprompter/src/pages/TeleprompterPage.tsx
  - apps/teleprompter/src/pages/TeleprompterPage.test.tsx
  - apps/teleprompter/src/hooks/useWs.ts (首轮 A+B, 11a2160)
  - apps/teleprompter/src/hooks/useWs.test.tsx (首轮 A+B 用例)
  - apps/desktop/src-tauri/src/lan/server.rs (首轮 C, 3d60c90)

## Hardening Backlog
<!-- 本会话附带发现，非本次修复范围 -->

- 网络切换硬化（2026-09-30 附带发现，v1 可接受手动重启）：Wi-Fi 换网后桌面 LAN IP 变化时，配对 URL/二维码不自动刷新，且旧网络遗留的幽灵连接仍计入 phone_count、继续把二维码藏起 → 用户无法自助重新配对（本次靠手动重启应用恢复：新进程 33695 / 新 token / 重新扫码）。硬化方向：修复 B 的存活检测会让幽灵连接自然回落（phone_count 归零、二维码重现）；后续可加「本机 IP 变化检测 → 重新生成配对串/提示重新扫码」与 QR 页手动刷新入口。
- 桌面端同型缺陷（2026-10-03 附带发现）：apps/desktop/src/components/AiTimeline.tsx 的 `toTimelineItems`（:125-144）与手机修复前的实现同构 —— 对每个 interviewer subtitle 无去重地 push，同 id 多事件会得到同 key 重复项。sim 源不可触达，但 Phase 2 真实 STT 的 partial 会直接命中 → 建议在 Phase 2 开工前把同样的『按 id 原位覆盖』语义复制到桌面端（并可抽到共享 util）。
- AI Tab 思考态锚点（2026-10-03 附带发现，暂不处理）：`useCenterAnchor(thinkingAnchorRef, tab === 'ai' && aiThinking)` 的 active 值是布尔，思考卡已挂载期间若有新问题到达，锚点不会重新触发 → 需等策略到达（aiThinking true→false）才重新居中。仅在『上一轮策略永久丢失』时可见；C 修复后该窗口大幅缩小。留待 Phase 2 观察。
- iOS Safari 真机覆盖缺口（2026-10-03）：本轮所有复现/验证均在 chromium（本机无 webkit 缓存）。scrollIntoView/ResizeObserver 在 iOS WKWebView 上的差异无法本地覆盖 → 检查点的实机复验是唯一真值来源。

## 2026-10-03 追加（协调者直接实施，子代理额度耗尽后）

**最终根因（AI Tab 冻结）**: 模拟源对面试官问题只发**单条 FINAL**（source.rs 原实现），提问过程中没有任何中间帧可渲染——「AI Tab 冻到提问结束」是数据形态问题，不是渲染问题。字幕 Tab「在动」是用户回答行的打字机动画。证据：token 直连真服务器抓帧（2026-10-03），interviewer 帧均为单条 FINAL；修复后三帧流式。
**修复（commit 09a4419）**: 模拟源问题帧改为三帧流式（partial@1.5s → partial@2.5s → final@3.5s，rank 1..=3、answer rank 4，会话 seq 1..=16）；三帧为独立成熟里程碑（emitted 游标逐帧推进）。cargo 全绿 35 lib + 4 integration。AI Tab 侧已有 9fa65fb 按 id 原位更新承接流式帧。
**待用户实机复验**: 运行中的应用已自动重建（新进程）；重新扫码后，AI 辅助应随面试官开口逐帧出现问题卡（三次增长），提问结束出思考中、策略卡居中。

## 2026-10-04 定案（用户实机复验通过）

全链修复经用户实机复验确认：手机同步/启动（A 动作排队 + B 看门狗 + C 服务端 Lagged 自愈）、AI Tab 按 id 去重、模拟源词级流式（一帧一词/250ms）、字幕与时间轴按 id 原位增长、流式行定稿不重打、手机启动重建双栏窗口。提交：11a2160/3d60c90/9fa65fb/09a4419/bbd181a/ca90e13/7478662。
