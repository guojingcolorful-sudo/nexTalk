---
phase: 2
slug: real-cloud-pipeline-audio-core
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-30
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Vitest 4（既有，apps 各包）· Playwright（既有 e2e）· cargo test（`--manifest-path apps/desktop/src-tauri/Cargo.toml`，**无 Cargo workspace**）· 新增：失败案例库 runner（`tools/vendor-experiments/failure-cases/run.mjs`）· 延迟 rig（02-01 交付） |
| **Config file** | 既有 `vitest.config.ts` ×2 + 根 `playwright.config.ts`；新增 `.github/workflows/ci.yml`（02-03 T6 引入） |
| **Quick run command** | `pnpm --filter <changed-pkg> test` + `cargo test --manifest-path ... <module>` |
| **Full suite command** | `pnpm -r test && pnpm exec playwright test && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml && node tools/vendor-experiments/failure-cases/run.mjs` |
| **Estimated runtime** | 2-4 分钟（无真实 key——供应商客户端测试全部走 mock 服务器） |

---

## Sampling Rate

- **每次任务提交后：** 变更包单测 + 对应 cargo 模块测试
- **每个波次后：** `pnpm -r test` + cargo 全量 + 失败案例库回归
- **阶段门（02-01 交付的硬闸）：** e2e 预算断言 ≤2000ms；跨波合并后全量套件
- **`/gsd:verify-work` 前：** 全量套件全绿 + 真实 key 现场延迟实跑（live latency run，02-05 收尾人工项）一次并记录瀑布入 SUMMARY
- **最大反馈延迟：** <30s（单元）/ ~3 分钟（全量）

---

## Per-Task Verification Map（关键任务）

| Plan | Task | 验证命令（要点） | 类型 |
|------|------|-----------------|------|
| 02-01 | T1 rig | `cargo test --test latency_rig`（流式 TTFB 五边界 + 预算硬断言 + 重叠证明） | automated |
| 02-02 | T2-T5 四客户端 | mock 服务器驱动的 cargo 测试（无 key）；真实 key 冒烟 `#[ignore]` | automated+mock |
| 02-02 | T6 协议扩展 | protocol 包 vitest + Rust serde 镜像测试双端同步 | automated |
| 02-03 | T1 commit gate | poisoned-partial 签名测试（GOV-15 零违反） | automated |
| 02-03 | T3 barge-in | ~100ms 停止断言（注入时钟） | automated |
| 02-03 | T5 溯源+计量 | JSONL schema 校验 + 计量归因测试 | automated |
| 02-03 | T6 案例库 | `run.mjs` 全量回归（0001/0002 转正） | automated |
| 02-04 | T4.0 跨语种探针 | 真实 key 合成英文 → **人工盲听判定**（blocking-human） | human gate |
| 02-04 | T2 训练 | voice_clone mock 测试；真实训练人工执行 | automated+mock |
| 02-05 | T0 工具链 | 包合法性人工核验（blocking-human） | human gate |
| 02-05 | T4 设备热切换 | `tests/audio_devices.rs` 故障注入；真实拔插 `#[ignore]` 人工一次 | automated+mock |

---

## Human Checkpoints

1. 02-02 T0：供应商依赖包合法性门（hmac/sha2/tokio-tungstenite 0.30）
2. 02-04 T4.0b：跨语种克隆盲听判定（决定 D-11 主选去留）
3. 02-04 T3：cpal [SUS]/hound/rubato 5.0 包核验
4. 02-05 T0：meson/ninja/pkg-config 工具链安装同意（含 AEC 显式降级选项）

---

## Threats

| Threat ID | Category | Verification |
|-----------|----------|--------------|
| T-02-SC | 供应链篡改（新依赖包） | 每个 blocking-human 门 + 包审计记录 |
| T-02-22 | 设备名欺骗 | `audio_devices.rs` Test 4（按角色不按名字） |
| T-02-23 | 回调阻塞 DoS | 溢出计数测试 |
| T-02-24 | 回采流信息泄露 | 默认不启用 + 不入 JSONL 断言 |
