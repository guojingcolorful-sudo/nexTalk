---
quick_task: dualpane-stop-button
created: 2026-09-30
status: ready
task: 双栏扩展视图增加「停止」按钮（结束面试）——HeaderBar 红色按钮 + 锁定确认弹窗 + stop_session + 回空态
files_modified:
  - apps/desktop/src/hooks/useTauriEvents.ts
  - apps/desktop/src/hooks/useTauriEvents.test.ts
  - apps/desktop/src/pages/DualPanePage.tsx
  - apps/desktop/src/pages/DualPanePage.test.tsx   # 新建
  - e2e/desktop.spec.ts
rust_changes: none
new_dependencies: none
commit_policy: 每任务一个原子提交；conventional 类型（fix / feat）；禁止 Co-Authored-By 署名；git add 按文件精确暂存
---

# PLAN — 双栏扩展视图「停止」按钮（quick: 20260930-dualpane-stop-button）

## 1. 目标

在双栏扩展视图（`apps/desktop/src/pages/DualPanePage.tsx`，860×680，路由 `#/dual`）中，会话进行中
（`status ∈ {listening, generating}`）时，在蓝色 HeaderBar 的 `actions` 槽（现 MicStatusPill 旁）
显示红色「停止」按钮：

1. 点击 → 弹出确认弹窗，文案锁定为：标题 **停止会话？**、正文 **当前字幕与策略将清空**、按钮 **[取消 / 停止]**
   （来源：`.planning/phases/01-foundation-simulation-mode/01-UI-SPEC.md` 第 230 / 506 行 Copywriting Contract）。
2. 确认 → `invoke('stop_session')`（与 `ConsolePage.tsx:64-67` 的停止逻辑完全一致，无额外命令）。
3. 双栏窗口保持打开，不调用任何窗口 API（不 close / 不 hide）；Rust 发布 `ended` 后，
   字幕流与 AI 面板回到既有空态：「等待语音输入」/「AI 策略将自动生成」。

### 验收标准（Done 定义）

- [ ] A1 `status` 为 `listening` 或 `generating` 时，HeaderBar 中 MicStatusPill 旁出现红色（`variant="red"`, `size="sm"`）「停止」按钮；`idle` / `ended` 时不出现。
- [ ] A2 点击后弹窗显示锁定文案（标题/正文/按钮逐字匹配，按钮组件为 ConfirmModal 默认「取消」+ `confirmLabel="停止"`）。
- [ ] A3 「取消」关闭弹窗、不调用命令、会话继续（按钮仍在）。
- [ ] A4 「停止」调用 `invoke('stop_session')` 一次并关闭弹窗。
- [ ] A5 Rust 发布 `ended` 后：双栏仍在 `#/dual`（heading「扩展视图」可见）、未发出 `plugin:window|close`；
      两栏内容清空并显示空态文案；pill 与停止按钮消失。
- [ ] A6 全部既有测试保持绿色；不新增依赖；不修改 Rust。

## 2. 关键现状分析（决定实现方式，执行前必读）

- Rust 侧 `stop_session`（`src-tauri/src/state.rs:249-252`）只做两件事：epoch+1、`publish_status(Ended)`
  → 发出 `session_status {session:'ended'}`（`announce_status`，state.rs:207-210）与
  `session {t:'status',session:'ended'}`（`publish`，state.rs:197-203）。**不会清空 webview 内的渲染流**。
- 前端 `useTauriEvents`（`apps/desktop/src/hooks/useTauriEvents.ts`）目前的清空时机只有一个：
  `session_started` 标记（第 88-92 行，WR-02/CR-01：为下一次会话做准备）。即：**仅按现有行为，
  停止后双栏会残留旧字幕与策略，无法达成「回到空态」**。
- 结论（纯前端约束下的最小方案）：在 hook 中让**终态 `ended` 同时清空 `events`**。
  - `events` 的唯一消费者就是 DualPanePage（QrCodeCard 只用 `phoneCount`，ConsolePage 只用 `status`），
    影响面精确等于本任务目标面板。
  - 这使「当前字幕与策略将清空」在**停止时刻**成立（而非拖到下次开始），与锁定文案语义一致。
- 该清空必须同时挂在**两条状态通道**上（Rust 两个事件都会发），且 `session` 通道的"先 append 后清空"
  顺序要正确（见 Task 1 测试）：`session_status` 路径直接清空；`session` 路径在 `{t:'status',session:'ended'}`
  被 append 之后立即清空，净结果为 `[]`。

## 3. 文件清单

| 文件 | 动作 | 内容 |
|---|---|---|
| `apps/desktop/src/hooks/useTauriEvents.ts` | 修改 | 新增 `applyStatus(next)`：`setStatus` + `next === 'ended'` 时 `setEvents([])`；两条监听通道改用之。更新注释说明「停止即清空」。 |
| `apps/desktop/src/hooks/useTauriEvents.test.ts` | 修改 | 新增用例：终态清空渲染流（覆盖两条通道及 append→clear 顺序）。RED 先行。 |
| `apps/desktop/src/pages/DualPanePage.tsx` | 修改 | HeaderBar actions 加红色「停止」按钮；`useState` + ConfirmModal + `invoke('stop_session')`；更新文件头注释。 |
| `apps/desktop/src/pages/DualPanePage.test.tsx` | **新建** | 组件测试（vitest + @testing-library/react），mock `@tauri-apps/api/event` 与 `@tauri-apps/api/core`。 |
| `e2e/desktop.spec.ts` | 修改 | 在既有 `dual pane extended view` describe（第 306 行起）内新增停止全流程 e2e。 |
| `.planning/quick/20260930-dualpane-stop-button/PLAN.md` | 本文件 | 规划产物（docs 提交）。 |

**明确不修改**：`src-tauri/**`（Rust）、`apps/teleprompter/**`、`ConfirmModal.tsx` / `HeaderBar.tsx` /
`NeobrutalismButton.tsx`（复用现有组件）、`ConsolePage.tsx`（仅作参考逻辑，零改动）。

## 4. 任务分解（TDD：先写测试跑红 → 实现跑绿 → 单提交）

### Task 1 — useTauriEvents：终态 `ended` 清空渲染流（fix）

**文件**：`apps/desktop/src/hooks/useTauriEvents.ts`、`apps/desktop/src/hooks/useTauriEvents.test.ts`

**RED（先写测试，跑到失败）** — 在既有测试文件的 `describe('useTauriEvents')` 内追加：

```ts
it('clears the stream on the terminal status so 停止 empties both panes', async () => {
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

  expect(result.current.status).toBe('ended');
  expect(result.current.events).toHaveLength(0);
});
```

预期 RED 原因：当前实现中 `session` 通道会把 `{t:'status',...}` append 进 `events`，最终长度为 1 而非 0。

**GREEN（实现）** — 在 hook 内、`useEffect` 之前提取共享函数（`setStatus`/`setEvents` 引用稳定，
两处闭包安全）：

```ts
// 停止: stop_session publishes the terminal status without a session_started
// marker, so the terminal transition also drops the rendered stream — the
// locked 停止 copy (当前字幕与策略将清空) must hold when the session stops,
// not only when the next one starts (WR-02/CR-01 follow-up).
const applyStatus = (next: SessionStatus): void => {
  setStatus(next);
  if (next === 'ended') setEvents([]);
};
```

- `session` 监听循环内：`if (item.t === 'status') setStatus(item.session);` → `if (item.t === 'status') applyStatus(item.session);`（位于 `setEvents(previous => [...previous, ...batch])` 之后，净结果 `[]`）。
- `session_status` 监听：`if (next !== null) setStatus(next);` → `if (next !== null) applyStatus(next);`。
- 不要动 `session_started` 分支与 `languageMode`（新会话开始时已重置，`ended` 时无需处理——无气泡可渲染，保持最小变更）。

**Verify（自动化）**：

```bash
pnpm --filter @nextalk/desktop exec vitest run src/hooks/useTauriEvents.test.ts   # 新用例 RED → GREEN
pnpm --filter @nextalk/desktop test                                              # 全量桌面单测无回归
```

**Done**：新用例绿；既有 4 个 hook 用例绿；全部桌面单测绿。

**提交**：`fix(desktop): drop the rendered session stream when the session ends`
（仅 `git add` 上述 2 个文件）

---

### Task 2 — DualPanePage：HeaderBar「停止」按钮 + 锁定确认弹窗 + stop_session（feat）

**文件**：`apps/desktop/src/pages/DualPanePage.tsx`、`apps/desktop/src/pages/DualPanePage.test.tsx`（新建）、`e2e/desktop.spec.ts`

**RED ①（组件测试先行）** — 新建 `apps/desktop/src/pages/DualPanePage.test.tsx`。
测试基建照抄 `src/hooks/useTauriEvents.test.ts` 的 mock 模式（`vi.hoisted` 注册表）+ `vi.mock('@tauri-apps/api/core')`
暴露 invoke spy；`beforeEach`/`afterEach` 清空 handlers 与 spy，`afterEach(cleanup)`（照抄
`VoiceEnrollmentPage.test.tsx` 的显式 cleanup 惯例）。无 Router 需求（DualPanePage 不依赖 router）。

用例清单（断言点）：
1. `idle`：无「停止」按钮（`queryByRole('button', { name: '停止' })` 为 null）、无 pill。
2. `listening` → 弹出发送 `session_status {session:'listening'}`：pill「麦克风开启-监听中」可见 + 「停止」按钮可见；
   再发 `generating`：「停止」按钮仍在（会话进行中两个状态都覆盖）。
3. 点「停止」→ 弹窗（`getByRole('dialog')`）含 `停止会话？` / `当前字幕与策略将清空` / 按钮 `取消`、`停止`；
   点「取消」→ 弹窗消失（`queryByRole('dialog')` 为 null）、invoke 未被调用、「停止」按钮仍在。
4. 点「停止」→ 点弹窗内「停止」（`dialog.getByRole('button', { name: '停止' })` 作用域定位，避免与头部按钮同名冲突）：
   `expect(invokeMock).toHaveBeenCalledTimes(1)` 且 `toHaveBeenCalledWith('stop_session')`；弹窗消失。
5. 清空回流：发 `listening` + 一条 `subtitle`（面试官）→ 字幕区可见英文文本；点「停止」确认 →
   依次发 `session_status {session:'ended'}` 与 `session {t:'status',session:'ended'}`（还原 Rust 双通道）→
   字幕区出现「等待语音输入」且原文本消失；AI 区出现「AI 策略将自动生成」；pill 与「停止」按钮均消失。
   （用例 5 的空态部分依赖 Task 1，先落地 Task 1 可减免红因干扰。）

**RED ②（e2e 先行）** — 在 `e2e/desktop.spec.ts` 既有 `test.describe('dual pane extended view')`
（第 306 行起，viewport 860×680 + `installTauriMock` 已就绪）内、"reflects listening, generating, and
ended session states" 用例（第 410-423 行）之后插入新用例：

```ts
test('停止 from the header ends the session behind the locked confirm, keeping the window open', async ({ page }) => {
  await page.goto('/#/dual');
  await waitForListeners(page);

  await emit(page, 'session_status', { session: 'listening' });
  await emit(page, 'session', QUESTION_EVENT);
  await expect(page.getByRole('region', { name: '实时字幕' })).toContainText(QUESTION_EN);

  // 头部停止控件（MicStatusPill 旁）。
  await page.getByRole('button', { name: '停止', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('停止会话？');
  await expect(dialog).toContainText('当前字幕与策略将清空');

  // 取消：不触达命令，会话继续。
  await dialog.getByRole('button', { name: '取消' }).click();
  await expect(dialog).toHaveCount(0);
  expect((await calls(page)).map((call) => call.cmd)).not.toContain('stop_session');
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeVisible();

  // 确认：stop_session 到达命令层，弹窗关闭。
  await page.getByRole('button', { name: '停止', exact: true }).click();
  await dialog.getByRole('button', { name: '停止' }).click();
  expect((await calls(page)).map((call) => call.cmd)).toContain('stop_session');
  await expect(dialog).toHaveCount(0);

  // Rust 发布终态：窗口保持打开，双栏回到锁定空态。
  await emit(page, 'session_status', { session: 'ended' });
  await expect(page).toHaveURL(/#\/dual$/);
  await expect(page.getByRole('heading', { name: '扩展视图' })).toBeVisible();
  await expect(page.getByText('麦克风开启-监听中')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '停止', exact: true })).toHaveCount(0);
  await expect(page.getByRole('region', { name: '实时字幕' })).toContainText('等待语音输入');
  await expect(page.getByRole('region', { name: '实时字幕' })).not.toContainText(QUESTION_EN);
  await expect(page.getByRole('region', { name: 'AI 辅助' })).toContainText('AI 策略将自动生成');
  expect((await calls(page)).map((call) => call.cmd)).not.toContain('plugin:window|close');
});
```

（`emit` / `calls` / `waitForListeners` / `QUESTION_EVENT` / `QUESTION_EN` 均为该文件既有辅助与夹具。）

**GREEN（实现）** — `DualPanePage.tsx` 精确改动：

- 第 1 行改为 `import { useMemo, useRef, useState } from 'react';`
  （注：现有 `useEffect` 导入未被使用，随行移除）。
- 新增导入：`import { invoke } from '@tauri-apps/api/core';`、`ConfirmModal`、`NeobrutalismButton`（`../components/`）。
- 组件内新增状态与处理（与 ConsolePage 一致的模式）：

```tsx
const [confirmStop, setConfirmStop] = useState(false);

const stopSession = () => {
  setConfirmStop(false);
  invoke('stop_session').catch((err) => console.error('stop_session failed', err));
};
```

- HeaderBar `actions`（第 72-78 行）改为：

```tsx
actions={
  listening ? (
    <div className="flex items-center gap-2">
      <MicStatusPill />
      <NeobrutalismButton variant="red" size="sm" onClick={() => setConfirmStop(true)}>
        停止
      </NeobrutalismButton>
    </div>
  ) : null
}
```

- 根 div（`.dot-matrix-root`，第 68 行开的 div）关闭前挂弹窗——即第 150 行 `</div>` 之前、两栏容器
  （第 149 行 `</div>` 之后）：

```tsx
<ConfirmModal
  open={confirmStop}
  title="停止会话？"
  body="当前字幕与策略将清空"
  confirmLabel="停止"
  onCancel={() => setConfirmStop(false)}
  onConfirm={stopSession}
/>
```

- 文件头注释补一句：HeaderBar actions 在会话进行中提供停止控件（红色，锁定确认）；确认后由 Rust 终态驱动双栏回空态，窗口不关闭。
- 约束：**不得**调用任何窗口 API（无 `getCurrentWindow().close()/hide()`）。

**Verify（自动化）**：

```bash
pnpm --filter @nextalk/desktop exec vitest run src/pages/DualPanePage.test.tsx   # 组件测试 RED → GREEN
pnpm --filter @nextalk/desktop test                                             # 桌面全量单测
pnpm --filter @nextalk/desktop exec tsc --noEmit                                # 类型检查（若报与本次改动无关的既有错误，记录后继续，不修无关文件）
pnpm --filter @nextalk/desktop build                                            # preview 依赖 dist，必须先构建
pnpm exec playwright test e2e/desktop.spec.ts --project=desktop --grep "dual pane"
```

环境说明：若 `localhost:1420` 上已有调试会话的 vite dev server，Playwright 会复用（`reuseExistingServer: true`），
行为等价；若 teleprompter 的 8791 服务启动失败（缺 dist），先跑
`pnpm --filter @nextalk/teleprompter build`。不要 kill 任何既有进程。

**Done**：组件 5 用例绿；新增 e2e 绿；桌面全量单测绿；build 成功。

**提交**：`feat(desktop): stop the session from the dual-pane header`
（仅 `git add` 上述 3 个文件）

## 5. 提交拆分（共 3 个原子提交，禁止 `git add -A`）

| # | 提交信息 | 文件 |
|---|---------|------|
| ① | `docs(quick): plan the dual-pane stop control` | `.planning/quick/20260930-dualpane-stop-button/PLAN.md` |
| ② | `fix(desktop): drop the rendered session stream when the session ends` | `useTauriEvents.ts` + `useTauriEvents.test.ts` |
| ③ | `feat(desktop): stop the session from the dual-pane header` | `DualPanePage.tsx` + `DualPanePage.test.tsx` + `e2e/desktop.spec.ts` |

规范：conventional 类型（fix / feat）；**不加 Co-Authored-By 署名**（用户全局规范）；每步提交后工作区中
其他会话的文件保持未暂存状态。执行完成后按 quick 流程在
`.planning/quick/20260930-dualpane-stop-button/SUMMARY.md` 写摘要。

## 6. 并发冲突防护（phone-sync-start 调试会话，开工前必做）

存在进行中的 GSD 调试会话 `.planning/debug/phone-sync-start.md`（status: investigating，`files_changed: []`），
其焦点：Rust `state.rs` / `lan/server.rs` 的广播→phone 写路径、teleprompter `hooks/useWs.ts`；
其假设 H2 涉及前端收包过滤，理论上有扩展到 `useTauriEvents.ts` 的可能（它修改产品代码前需经用户 checkpoint）。

开工前检查（缺一不可）：
1. `git status --short` + `git diff --name-only`；确认本任务 5 个代码/测试文件全部干净。
   若任一文件已被调试会话改动 → **停止并上报冲突**，不自行合并。
2. 提交一律按路径精确 `git add <file>`；工作区现存其他会话的未提交文件
   （`.planning/ROADMAP.md`、`.planning/ref/*`、`.planning/research/*`）与未跟踪目录 `.planning/debug/`
   **一律不得纳入本次提交**。
3. 不触碰：`src-tauri/**`、`apps/teleprompter/**`、`.planning/debug/**`、`.planning/ROADMAP.md`。
4. 环境：调试会话的开发实例可能在运行（tauri dev，日志 `/tmp/nextalk-dev.log`）；
   不要 kill 任何进程；本任务验证只走 vitest（jsdom）与 Playwright（1420/8791），与桌面 LAN 端口 8787 无关。
   如需在真实双栏窗口做人工冒烟，须先与用户确认（该运行实例属于调试会话环境）。

## 7. 威胁模型（security_enforcement=true，STRIDE 摘要）

| ID | 类别 | 组件 | 处置 | 说明 |
|----|------|------|------|------|
| T-QUICK-01 | Tampering | hook 事件流清空 | mitigate | 清空仅由经 `isServerEvent`/`narrowStatus` 校验收窄的合法 `ended` 触发；畸形载荷仍被既有守卫丢弃（T-01-02 不变） |
| T-QUICK-02 | E（误操作破坏） | 双栏「停止」按钮 | mitigate | 破坏性操作必须经锁定 ConfirmModal 二步确认；初始焦点在「取消」；文案与 ConsolePage 完全一致 |
| T-QUICK-03 | DoS/重复 | `stop_session` 重复调用 | accept | Rust 停止幂等（epoch+1 后重发 ended）；确认即关弹窗，重复窗口极小 |
| T-QUICK-SC | Supply chain | 依赖安装 | n/a | 零新增依赖，无包管理安装 |

无新增信任边界：仅复用既有 IPC 命令，无新输入面、无新数据流。

## 8. 明确不做（Out of Scope）

- 不动 Rust（`stop_session` 行为保持不变）；不加任何新 Tauri 命令/权限。
- 不改 ConsolePage 的停止逻辑；不抽取共享 hook（两处 4 行逻辑重复可接受，避免过度抽象）。
- 不处理「弹窗打开期间会话被外部（手机）停止」的竞态：确认/取消仍可用，重复 `stop_session` 幂等——有意接受。
- 不做 visual regression 截图基线（仓库 e2e 无截图惯例）、不做分支策略（quick 直接提交 main）。

## 9. 已否决方案（避免返工）

- **渲染层抑制**（`status==='ended'` 时页面渲染空数组，不改 hook）：被否——在 React 状态外复制会话生命周期语义，
  与「Rust 单一事实源 + session_started 清空在 hook 内」的既有架构（useTauriEvents.ts:84-103 注释）相悖。
- **Rust 侧在 stop 时发清空事件**：被否——任务约束「纯前端，不动 Rust」。

## 10. 回归风险

- hook 清空影响面：`events` 唯一消费者是 DualPanePage；ConsolePage 只用 `status`、QrCodeCard 只用 `phoneCount`。
- 既有 e2e「reflects listening, generating, and ended session states」不携带内容事件，清空为其空操作，不受影响。
- 既有「streams the simulated question…」「drops a malformed payload…」用例不触发 `ended`，不受影响。
