---
quick_task: stop-keeps-stream
created: 2026-09-30
status: ready
task: 停止后双栏保留字幕与策略（与手机端一致），仅在新会话 session_started 清空重来——撤销 246baae 的 ended 清空
files_modified:
  - apps/desktop/src/hooks/useTauriEvents.ts
  - apps/desktop/src/hooks/useTauriEvents.test.ts
  - apps/desktop/src/pages/DualPanePage.test.tsx   # 任务描述漏列；不改必红（见 §3 修正说明）
  - apps/desktop/src/pages/DualPanePage.tsx        # 仅文件头注释 2 行，功能零改动
  - e2e/desktop.spec.ts
rust_changes: none
new_dependencies: none
commit_policy: 单任务单原子提交；fix 类型；禁止 Co-Authored-By 署名；git add 按文件精确暂存
---

# PLAN — 停止后保留双栏内容（quick: 20260930-stop-keeps-stream）

## 1. 目标

桌面端「停止会话」后，双栏扩展视图（`#/dual`）**保留**停止前的字幕与 AI 策略内容（与手机端
teleprompter 行为一致）；内容只在**下一次会话开始**（Rust 发布 `session_started`）时清空重来。
空态（「等待语音输入」/「AI 策略将自动生成」）只在从未渲染过内容时出现。

**行为修正对象**：提交 `246baae`（fix: drop the rendered session stream when the session ends）
在 `useTauriEvents.ts` 的 `applyStatus` 中给终态 `ended` 挂了 `setEvents([])`，挂在 `session` 与
`session_status` 两条通道上——本次撤销该清空，`session_started` 清空分支（原有逻辑）保持不变。

### 验收标准（Done 定义）

- [ ] A1 终态 `ended`（两条通道）到达后，`events` **保留**（字幕 + 策略仍在），`status` 仍正确为 `ended`。
- [ ] A2 `ended` 不清空 `events`、不重置 `languageMode`；上一次会话的内容在停止后继续渲染。
- [ ] A3 `session_started` 仍清空 `events` 并重置 `languageMode`；双栏回到锁定空态。
- [ ] A4 双栏空态只在无内容可渲染时出现（`subtitles.length === 0` / `timelineItems.length === 0`，
      既有条件，功能零改动）。
- [ ] A5 e2e：停止全流程后双栏内容保留、窗口保持打开（无 window API）；随后 `session_started` 清空回空态。
- [ ] A6 全部既有测试绿色：desktop 全量 vitest、`--grep "dual pane"` e2e；`demo.spec.ts` 不受影响。
- [ ] A7 零新增依赖、零 Rust 改动、零 teleprompter 改动。

## 2. 关键现状分析（决定实现方式，执行前必读）

- **清空逻辑位置**：`apps/desktop/src/hooks/useTauriEvents.ts`
  - 第 67-74 行：`applyStatus(next)` = `setStatus(next)` + `next === 'ended'` 时 `setEvents([])`（246baae 引入）。
    两条通道都经它：`session` 通道的 `{t:'status'}`（第 107 行）与 `session_status` 通道（第 118 行）。
  - 第 93-102 行：`session_started` 分支（原有逻辑，**保留不动**）——清 `events` + 重置 `languageMode`，
    注释为 WR-02/CR-01 语义。
- **手机端基准（行为参照，只读）**：`apps/teleprompter/src/hooks/useWs.ts:104-127` 仅在 `session_started`
  时 `fresh.length = 0` 替换全部事件（`setEvents(fresh)`），`ended` 不清空——即「停止保留、新会话清空」。
- **渲染过滤已确认安全**：`AiTimeline.tsx` 的 `toTimelineItems`（125-144 行）只取 interviewer 字幕与
  strategy 事件、`isAiThinking`（105-117 行）只看字幕轮次与策略轮次——保留后 `session` 通道 append 的
  `{t:'status',session:'ended'}` 事件不会污染右栏渲染。
- **空态条件**：`DualPanePage.tsx` 第 126 行 `subtitles.length === 0` → 「等待语音输入」；
  第 156-165 行 `timelineItems.length > 0 ? <AiTimeline/> : <AiTimeline 空态/>` → 「AI 策略将自动生成」。
  保留场景下 length > 0，空态自然不出现——无需功能改动。
- **`events` 消费者**：仅 `DualPanePage`（`ConsolePage` 只用 `status`，`QrCodeCard` 只用 `phoneCount`），
  影响面精确等于双栏面板。
- **Rust 线形（只读参考）**：`session_started` = `{"t":"session_started","epoch":N}`
  （`apps/desktop/src-tauri/src/state.rs:437-439`）；`stop_session` 只发终态，不发清空事件。
- **已有断言旧行为的测试（本次必须同步翻转）**：
  - `useTauriEvents.test.ts:103-123`（终态清空，hook 级）
  - `DualPanePage.test.tsx:135-159`（终态双栏回空态，组件级）——任务描述漏列此文件
  - `e2e/desktop.spec.ts:425-461`（停止流程，双栏回空态，e2e 级）

## 3. 文件清单（含对任务描述「3 个文件」的修正）

| 文件 | 动作 | 内容 |
|---|---|---|
| `apps/desktop/src/hooks/useTauriEvents.ts` | 修改 | 删除 `applyStatus` 的 `ended` 清空（恢复纯 `setStatus`）；更新注释锁定「停止保留、新会话清空」语义 |
| `apps/desktop/src/hooks/useTauriEvents.test.ts` | 修改 | 终态用例翻转为「保留；下一次 `session_started` 才清空」 |
| `apps/desktop/src/pages/DualPanePage.test.tsx` | **修改（任务描述漏列）** | 用例 5 翻转为「停止后双栏保留；`session_started` 回空态」；新增 strategy 夹具 |
| `apps/desktop/src/pages/DualPanePage.tsx` | 修改（**仅注释**） | 文件头第 36-39 行注释仍描述被撤销的旧行为（"terminal state drives both panes back to their empty states"），改为「停止后保留、下一次会话开始才清空」；**逻辑零改动** |
| `e2e/desktop.spec.ts` | 修改 | 停止流程 e2e 断言翻转为保留 + `session_started` 清空回空态 |
| `.planning/quick/20260930-stop-keeps-stream/PLAN.md` | 本文件 | 规划产物（提交 ①） |

**修正说明（必读）**：任务描述称只动 3 个文件、`DualPanePage.tsx` 无需改动。经核实：
1. `apps/desktop/src/pages/DualPanePage.test.tsx` 由上一轮 `56f90f4` 新建，其用例 5（135-159 行）断言
   `ended` 后「等待语音输入 / AI 策略将自动生成」——不改必红，**必改**（第 4 个文件）。
2. `DualPanePage.tsx` 功能确无需改动（空态条件自然满足）；但文件头注释（36-39 行）描述的是本轮要撤销的
   旧行为，保留会误导后续维护（上一轮正是注释与行为绑定的清空语义引发本次修正）。计划含**仅 2 行注释**修正，
   功能零改动、零测试影响；如坚持零改动可跳过此行，其余步骤不受影响。

**明确不修改**：`src-tauri/**`（Rust）、`apps/teleprompter/**`（行为基准，只读）、`ConsolePage.tsx`、
`ConfirmModal.tsx`、以及确认弹窗文案（见 §9 边界说明）。

## 4. 任务分解 — 单任务（TDD：先写测试跑红 → 实现跑绿 → 单提交）

### Task 1 — 停止保留、`session_started` 才清空（fix）

**文件**：§3 表中 5 个文件。

#### 测试用例意图（要求 2：每处断言的目的）

| # | 文件 | 用例 | 意图 |
|---|------|------|------|
| T1 | `useTauriEvents.test.ts` | 改写既有「终态清空」用例 → `keeps the stream when the terminal status arrives — 停止 retains until the next session` | 锁定生命周期：`ended`（双通道，含 `session` 通道 append 的 status 事件）后字幕+策略仍在、`status==='ended'`；随后 `session_started` → `events` 清零。**RED 锚点** |
| T2 | `useTauriEvents.test.ts` | 既有 `clears the stream and the applied mode when a new session announces itself`（74-101 行）**保持不动** | 继续锚定「新会话清空 + languageMode 重置」语义，防止本次改动误伤 |
| T3 | `DualPanePage.test.tsx` | 改写用例 5 → `keeps both panes after the terminal status; only the next session clears them` | 组件级证明用户可见行为：结束后面板内容仍在、无空态、pill/停止按钮消失；`session_started` 后回空态。**RED 锚点** |
| T4 | `e2e/desktop.spec.ts` | 改写停止流程用例 → `停止 from the header keeps both panes until the next session starts (window stays open)` | 浏览器级验收：确认弹窗→`stop_session`→`ended`→双栏保留→`session_started`→空态；全程无 window API |

#### RED（先改测试，跑到失败）

**T1 — `apps/desktop/src/hooks/useTauriEvents.test.ts`**：将 103-123 行用例改写为：

```ts
it('keeps the stream when the terminal status arrives — 停止 retains until the next session', async () => {
  const { result } = renderHook(() => useTauriEvents());
  await waitForListeners('session');
  await waitForListeners('session_status');

  act(() => {
    emit('session', { t: 'subtitle', id: 'r1-q', speaker: 'interviewer', seq: 1, zh: '问题', en: 'question', final: true });
    emit('session', { t: 'strategy', id: 's1', roundId: 'r1', title: '策略', bullets: ['先给结论'] });
    emit('session_status', { session: 'listening' });
  });
  expect(result.current.events).toHaveLength(2);

  // Rust 发布 停止 的顺序：announce_status(session_status) → publish(session)。
  act(() => {
    emit('session_status', { session: 'ended' });
    emit('session', { t: 'status', session: 'ended' });
  });

  // 停止保留：两条通道的终态都不再清空渲染流（与手机端一致）。
  expect(result.current.status).toBe('ended');
  expect(result.current.events).toHaveLength(3); // 字幕+策略+session 通道 append 的 ended status
  expect(result.current.events.some((e) => e.t === 'subtitle' && e.id === 'r1-q')).toBe(true);
  expect(result.current.events.some((e) => e.t === 'strategy' && e.id === 's1')).toBe(true);

  // 只有下一次会话开始才清空重来。
  act(() => {
    emit('session', { t: 'session_started', epoch: 2 });
  });
  expect(result.current.events).toHaveLength(0);
});
```

预期 RED 签名：`expected [...] to have length 3` / `received length 0`（当前实现 `ended` 即清空）。

**T3 — `apps/desktop/src/pages/DualPanePage.test.tsx`**：
- 在 QUESTION_EVENT 夹具（46-54 行）后新增策略夹具（形状对齐 e2e 夹具 `desktop.spec.ts:298-304`）：

```ts
const STRATEGY_EVENT = {
  t: 'strategy',
  id: 's-r1',
  roundId: 'r1',
  title: '数据库优化',
  bullets: ['慢查询日志定位'],
};
```

- 将 135-159 行用例 5 改写为：

```ts
it('keeps both panes after the terminal status; only the next session clears them', async () => {
  await renderDualPane();

  act(() => {
    emit('session_status', { session: 'listening' });
    emit('session', QUESTION_EVENT);
    emit('session', STRATEGY_EVENT);
  });
  const subtitles = screen.getByRole('region', { name: '实时字幕' });
  expect(within(subtitles).getByText(QUESTION_EVENT.en)).toBeTruthy();
  expect(screen.getByText('数据库优化')).toBeTruthy();

  fireEvent.click(screen.getByRole('button', STOP_LABEL));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', STOP_LABEL));

  // Rust 停止 publishes the terminal state on both channels, in this order.
  act(() => {
    emit('session_status', { session: 'ended' });
    emit('session', { t: 'status', session: 'ended' });
  });

  // 停止保留：内容仍在、无空态；控件随 status 收起（与手机端一致）。
  expect(within(subtitles).getByText(QUESTION_EVENT.en)).toBeTruthy();
  expect(within(subtitles).queryByText('等待语音输入')).toBeNull();
  expect(screen.getByText('数据库优化')).toBeTruthy();
  expect(screen.queryByText('AI 策略将自动生成')).toBeNull();
  expect(screen.queryByText(PILL_COPY)).toBeNull();
  expect(screen.queryByRole('button', STOP_LABEL)).toBeNull();

  // 只有新会话开始才清空重来。
  act(() => {
    emit('session', { t: 'session_started', epoch: 2 });
  });
  expect(within(subtitles).getByText('等待语音输入')).toBeTruthy();
  expect(within(subtitles).queryByText(QUESTION_EVENT.en)).toBeNull();
  expect(screen.getByText('AI 策略将自动生成')).toBeTruthy();
});
```

预期 RED 签名：`Unable to find an element with the text: <QUESTION_EVENT.en>`（当前实现 `ended` 清空后空态出现）。

**T4 — `e2e/desktop.spec.ts`**：改写 425-461 行停止流程用例。用例名改为
`停止 from the header keeps both panes until the next session starts (window stays open)`；
第 429-431 行的准备段增加策略事件；451-459 行断言翻转为保留 + 新会话回空态：

```ts
    await emit(page, 'session_status', { session: 'listening' });
    await emit(page, 'session', QUESTION_EVENT);
    await emit(page, 'session', STRATEGY_EVENT);
    await expect(page.getByRole('region', { name: '实时字幕' })).toContainText(QUESTION_EN);
    await expect(page.getByRole('region', { name: 'AI 辅助' })).toContainText('数据库优化');
    // …取消/确认弹窗段（433-449 行）保持不变…
    // Rust 发布终态：窗口保持打开，双栏保留停止前的字幕与策略（与手机端一致）。
    await emit(page, 'session_status', { session: 'ended' });
    await expect(page).toHaveURL(/#\/dual$/);
    await expect(page.getByRole('heading', { name: '扩展视图' })).toBeVisible();
    await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
    await expect(page.getByRole('button', { name: '停止', exact: true })).toHaveCount(0);
    await expect(page.getByRole('region', { name: '实时字幕' })).toContainText(QUESTION_EN);
    await expect(page.getByRole('region', { name: '实时字幕' })).not.toContainText('等待语音输入');
    await expect(page.getByRole('region', { name: 'AI 辅助' })).toContainText('数据库优化');
    await expect(page.getByRole('region', { name: 'AI 辅助' })).not.toContainText('AI 策略将自动生成');

    // 只有下一次会话开始（Rust session_started）才清空重来。
    await emit(page, 'session', { t: 'session_started', epoch: 2 });
    await expect(page.getByRole('region', { name: '实时字幕' })).toContainText('等待语音输入');
    await expect(page.getByRole('region', { name: '实时字幕' })).not.toContainText(QUESTION_EN);
    await expect(page.getByRole('region', { name: 'AI 辅助' })).toContainText('AI 策略将自动生成');
    expect((await calls(page)).map((call) => call.cmd)).not.toContain('plugin:window|close');
```

（弹窗文案断言第 436-437 行 `停止会话？` / `当前字幕与策略将清空` **保持不变**——弹窗不改，见 §9。
夹具 `QUESTION_EVENT`/`QUESTION_EN`/`STRATEGY_EVENT` 均为该文件既有辅助（282-304 行）。）

**RED 验证命令**（e2e 的 RED 证据可选：其断言语义已由 T1/T3 在单测层先证）：
```bash
pnpm --filter @nextalk/desktop exec vitest run src/hooks/useTauriEvents.test.ts src/pages/DualPanePage.test.tsx
# 预期：T1 长度 0≠3；T3 找不到 QUESTION_EVENT.en —— 两处 RED
```

#### GREEN（改实现）

**`apps/desktop/src/hooks/useTauriEvents.ts`**：
- 删除第 67-74 行整块（旧注释 + `applyStatus`）。
- 第 107 行：`if (item.t === 'status') applyStatus(item.session);` → `if (item.t === 'status') setStatus(item.session);`
- 第 118 行：`if (next !== null) applyStatus(next);` → `if (next !== null) setStatus(next);`
- 更新第 93-96 行 `session_started` 分支注释，显式记录本次决策，防止再次回摆：

```ts
// WR-02/CR-01: the session restarted — clear the previous stream so the new
// session never stacks on top of it. 停止/ended 故意不清空（2026-09-30 用户实测
// 修正，撤销 246baae）：双栏在会话结束后保留内容，与手机端一致；空态只在
// 从未渲染过内容时出现。
```

- 约束：除上述外不动任何逻辑；`session_started` 分支的 `setEvents([])` 与 `setLanguageMode(null)` 保持。

**`apps/desktop/src/pages/DualPanePage.tsx`（仅注释）**：第 36-39 行文件头注释末句
"confirming sends `stop_session` and the Rust terminal state drives both panes back to their
empty states" 改为「confirming sends `stop_session`; the panes keep their content after the stop
（与手机端一致），只有下一次会话开始时清空重来」。**不得改动任何 JSX/逻辑。**

#### Verify（自动化，顺序固定）

```bash
# 1) vitest：目标 RED → GREEN，再全量
pnpm --filter @nextalk/desktop exec vitest run src/hooks/useTauriEvents.test.ts src/pages/DualPanePage.test.tsx
pnpm --filter @nextalk/desktop test

# 2) build（e2e 的 1420 preview 依赖 fresh dist）
pnpm --filter @nextalk/desktop build

# 3) playwright grep "dual pane"
pnpm exec playwright test e2e/desktop.spec.ts --project=desktop --grep "dual pane"
```

环境说明（沿用上一轮约定）：1420/8791 均 `reuseExistingServer: true`，已有调试实例可复用、不要 kill 任何进程；
若 8791 启动失败（teleprompter 缺 dist），先跑 `pnpm --filter @nextalk/teleprompter build`。
补充回归（非阻塞）：`pnpm exec playwright test e2e/demo.spec.ts --project=desktop`。

#### Done

T1/T3 绿、T2 与其余既有单测绿、桌面全量 vitest 绿、build 成功、`--grep "dual pane"` e2e 绿。

#### 提交

单原子提交（TDD 为过程序：测试先改跑红、实现后跑绿，**一个提交**）：

```
fix(desktop): keep the rendered stream after 停止, clear only on the next session

- drop the ended-status clearing from both status channels in useTauriEvents
- keep 停止 content on screen like the phone; session_started still resets
- flip the hook, component and e2e assertions to retention + re-clear
```

仅 `git add` 上述 5 个文件；**不加 Co-Authored-By 署名**。

## 5. 提交拆分（共 2 个原子提交，禁止 `git add -A`）

| # | 提交信息 | 文件 |
|---|---------|------|
| ① | `docs(quick): plan the stop-keeps-stream fix` | `.planning/quick/20260930-stop-keeps-stream/PLAN.md`（本文件；执行开始时提交） |
| ② | `fix(desktop): keep the rendered stream after 停止, clear only on the next session` | §3 表 5 个文件（`useTauriEvents.ts`、`useTauriEvents.test.ts`、`DualPanePage.test.tsx`、`DualPanePage.tsx`、`e2e/desktop.spec.ts`） |

规范：conventional 类型（fix / docs）；**不加 Co-Authored-By 署名**（用户全局规范）；每步按路径精确 `git add`，
工作区中其他会话的文件保持未暂存。执行完成后按 quick 流程写
`.planning/quick/20260930-stop-keeps-stream/SUMMARY.md`（另一次 docs(quick) 提交）。

## 6. 并发冲突防护（phone-sync-start 调试会话，开工前必做）

进行中的 GSD 调试会话 `.planning/debug/phone-sync-start.md`（status: investigating）的焦点是
`apps/teleprompter/**` 与 `src-tauri/src/lan/server.rs` 的广播→phone 写路径。本任务与其**无文件交集**，
但过程纪律不退让：

1. **预检**：执行开始时先跑 `git status --short` + `git diff --name-only`，确认本任务 5 个目标文件全部干净。
   规划时点实测：目标文件全部干净（工作区仅 `.planning/ROADMAP.md`、`.planning/ref/*`、`.planning/research/*`
   被其他会话修改，`.planning/debug/` 未跟踪）。若执行时任一目标文件已脏 → **停止并上报冲突**，不自行合并。
2. **精确 add**：每步提交 `git add <精确路径>`；`.planning/ROADMAP.md`、`.planning/ref/*`、`.planning/research/*`、
   `.planning/debug/**` 一律不得纳入本次任何提交。
3. **不触碰**：`src-tauri/**`、`apps/teleprompter/**`、`.planning/debug/**`、`.planning/ROADMAP.md`。
4. **环境**：调试会话可能开着 tauri dev / vite（1420）实例——不要 kill 任何进程；Playwright 复用既有
   server（`reuseExistingServer: true`）行为等价；本任务验证只走 vitest（jsdom）与 Playwright（1420/8791）。

## 7. 威胁模型（security_enforcement=true，STRIDE 摘要）

| ID | 类别 | 组件 | 处置 | 说明 |
|----|------|------|------|------|
| T-QUICK-01 | Tampering | hook 事件流清空路径 | accept | 清空现在**只剩** `session_started` 一个入口（经 `payload?.t` 标记匹配 + `isServerEvent` 收窄）；畸形载荷守卫（T-01-02）不变，攻击面净缩小 |
| T-QUICK-02 | Information disclosure | 双栏停止后保留内容 | accept | 内容停留用户本机桌面窗口的 React 状态中（用户明确要求，与手机端一致）；纯本地渲染、无新增数据流/传输；窗口归用户控制 |
| T-QUICK-03 | DoS/资源 | `events` 保留至下次会话 | accept | 上界为一个会话的事件量，`session_started` 即释放；不产生跨会话无界增长 |
| T-QUICK-SC | Supply chain | 依赖安装 | n/a | 零新增依赖，无包管理安装 |

无新增信任边界：零新 IPC 命令、零新输入面、零新数据流。

## 8. 明确不做（Out of Scope）

- 不动 Rust（`stop_session` / `session_started` 发布逻辑不变）；不加新 Tauri 命令/权限。
- 不改 teleprompter（行为基准，只读参考）；不改 `ConsolePage.tsx`（只消费 `status`）。
- 不引入「是否曾经有过会话」的独立状态：保留场景由既有条件（`subtitles.length === 0` /
  `timelineItems.length === 0`）自然满足；「有会话但零内容」停止后与从未有会话显示同一空态
  （无可保留内容，行为等价，有意接受）。
- 不做分支/PR（quick 流程直接 main 提交）；不做 visual regression 截图基线（仓库无此惯例）。

## 9. 边界与备注

- **确认弹窗文案「当前字幕与策略将清空」保持不变**（任务明确 DualPanePage 无功能改动）：该文案在会话
  生命周期语义下仍然成立——内容**将**在下一次会话开始时清空。若用户后续希望改为「停止后内容保留」类文案，
  属独立小任务，将触及 `DualPanePage.tsx` + 3 处断言（`DualPanePage.test.tsx:110`、`desktop.spec.ts:437`、
  `demo.spec.ts:232`）——本次不做。
- `languageMode` 在 `ended` 时保持（本次未改、246baae 也未改）：停止后气泡保留其渲染形态；`session_started`
  仍重置为 per-speaker 默认（T2 覆盖）。

## 10. 回归风险

- `e2e/demo.spec.ts:241`（console 停止后 CTA 恢复）与 `:371-374`（ended 后 pill/indicator 收起）只断言
  `status` 驱动元素，不断言内容清空 → 不受影响。
- `e2e/desktop.spec.ts:410-423`（reflects listening/generating/ended）不携带内容事件 → 保留/清空均空操作。
- `e2e/skeleton.spec.ts:109`（初始空态）无会话事件 → 不受影响。
- `apps/desktop/src/pages/DualPanePage.test.tsx` 其余 4 个用例（idle/控件/取消/确认命令）不涉 `ended` → 不受影响。
