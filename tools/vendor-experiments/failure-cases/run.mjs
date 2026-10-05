#!/usr/bin/env node
/**
 * 失败案例库 runner（02-03 T3.8 / D-12）。
 *
 * 案例库把真实故障沉淀为可回归的资产：每个 JSON 是一个已观测的失败模式
 * （场景覆盖 AI-SPEC 第 5 节：SSE 解析、数字漂移、术语漂移、partial/final
 * 误提交、抢话重叠、供应商断连/错误码、wpgs 重建、畸形 JSON、TTS 错误帧、
 * 熔断、置信溯源、误弃权、降级展示、写队列边界、克隆握手、术语命中）。
 * 每条 `regression_test` 必须指向仓库中真实存在的测试——案卷不是散文，
 * 是能跑的防线。
 *
 * 用法（零依赖，Node ESM）：
 *   node tools/vendor-experiments/failure-cases/run.mjs --check
 *     校验全库：schema 完整性（9 个字段，不多不少）、id 唯一且四位零填充、
 *     root_cause 属受控枚举 {术语缺失, 模型幻觉, 音频质量, 数字漂移, 其他}、
 *     source 属 {低置信事件, 用户反馈, 人工抽检}、案例总数 ≥20，
 *     以及每条 regression_test 指向的测试标识在仓库中真实存在（grep 查名）。
 *     枚举校验读「：」之前的部分——案卷沿用「枚举（细节）：说明」的写法。
 *
 *   node tools/vendor-experiments/failure-cases/run.mjs --run
 *     逐例执行 regression_test：cargo 目标走
 *     `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml <filter>`，
 *     vitest 目标走 `pnpm --filter <pkg> exec vitest run -t <pattern>`
 *     （包名由 grep 命中的测试文件路径推导）。任一失败即非零退出，
 *     并打印逐例结果与汇总。
 */

import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, '..', '..', '..');

const REQUIRED_KEYS = [
  'id',
  'title',
  'root_cause',
  'source',
  'input',
  'wrong_output',
  'expected_output',
  'fix',
  'regression_test',
];
const ROOT_CAUSES = ['术语缺失', '模型幻觉', '音频质量', '数字漂移', '其他'];
const SOURCES = ['低置信事件', '用户反馈', '人工抽检'];
const MIN_CASES = 20;

/** 本地运行时常在裸 shell 里：cargo 若存在就补进 PATH。 */
function withCargoOnPath() {
  const cargoBin = join(process.env.HOME ?? '', '.cargo', 'bin');
  if (process.env.HOME && !(process.env.PATH ?? '').includes(cargoBin)) {
    process.env.PATH = `${cargoBin}:${process.env.PATH ?? ''}`;
  }
}

function loadCases() {
  const files = readdirSync(HERE)
    .filter((name) => /^\d{4}-.+\.json$/.test(name))
    .sort();
  return files.map((file) => {
    const raw = readFileSync(join(HERE, file), 'utf8');
    return { file, data: JSON.parse(raw) };
  });
}

/** 「枚举（细节）：说明」→ 只看「：」之前——受控枚举必须出现在那里。 */
function enumHead(value) {
  const cut = value.indexOf('：');
  return cut === -1 ? value : value.slice(0, cut);
}

function fail(errors, message) {
  errors.push(message);
  return false;
}

/**
 * `regression_test` 形如 `cargo: <测试名>` 或 `vitest: <用例名>`。
 * 返回 { kind, name } 或 null。
 */
function parseRegressionTarget(value) {
  const match = /^(cargo|vitest):\s*(\S(?:.*\S)?)$/.exec(value);
  if (!match) return null;
  return { kind: match[1], name: match[2] };
}

/** grep -rlF 的名字查存——案卷引用的测试必须真的在仓库里。 */
function grepFiles(pattern, roots, includes) {
  const args = ['-rlF'];
  for (const include of includes) args.push(`--include=${include}`);
  args.push(pattern, ...roots);
  const result = spawnSync('grep', args, { cwd: REPO_ROOT, encoding: 'utf8' });
  if (result.status !== 0) return [];
  return result.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

function resolveTarget(target) {
  if (target.kind === 'cargo') {
    const files = grepFiles(target.name, ['apps/desktop/src-tauri'], ['*.rs']);
    return files.length > 0 ? { files } : { files: [], error: 'no .rs file contains this name' };
  }
  const files = grepFiles(
    target.name,
    ['apps/desktop/src', 'apps/teleprompter/src'],
    ['*.test.ts', '*.test.tsx'],
  );
  if (files.length === 0) {
    return { files: [], error: 'no test file contains this name' };
  }
  // 同名用例可能同时存在于两包：桌面包优先，且 --run 只跑命中的那个包。
  const preferred = files.find((f) => f.startsWith('apps/desktop/')) ?? files[0];
  return { files, preferred, error: null };
}

function runCheck(cases) {
  const errors = [];
  const ids = new Map();

  for (const { file, data } of cases) {
    const keys = Object.keys(data).sort();
    if (JSON.stringify(keys) !== JSON.stringify([...REQUIRED_KEYS].sort())) {
      fail(errors, `${file}: 字段集必须恰为 9 个（缺/多字段）`);
      continue;
    }
    for (const key of REQUIRED_KEYS) {
      if (typeof data[key] !== 'string' || data[key].trim() === '') {
        fail(errors, `${file}: 字段 ${key} 必须是非空字符串`);
      }
    }
    if (!/^\d{4}$/.test(data.id)) {
      fail(errors, `${file}: id 必须是四位零填充，实际 ${JSON.stringify(data.id)}`);
    } else if (ids.has(data.id)) {
      fail(errors, `${file}: id ${data.id} 与 ${ids.get(data.id)} 重复`);
    } else {
      ids.set(data.id, file);
    }
    const causeHead = enumHead(data.root_cause);
    if (!ROOT_CAUSES.some((value) => causeHead.includes(value))) {
      fail(errors, `${file}: root_cause 不在受控枚举内：${causeHead}`);
    }
    const sourceHead = enumHead(data.source);
    if (!SOURCES.some((value) => sourceHead.includes(value))) {
      fail(errors, `${file}: source 不在受控枚举内：${sourceHead}`);
    }
    const target = parseRegressionTarget(data.regression_test);
    if (target === null) {
      fail(errors, `${file}: regression_test 必须是 \`cargo: <名>\` 或 \`vitest: <名>\``);
      continue;
    }
    const resolved = resolveTarget(target);
    if (resolved.error) {
      fail(errors, `${file}: regression_test 指向不存在的测试：${target.name}（${resolved.error}）`);
    }
  }

  if (cases.length < MIN_CASES) {
    fail(errors, `案例总数 ${cases.length} < ${MIN_CASES}`);
  }

  return errors;
}

/** cargo 一次运行会打印多份 summary；至少一个测试通过且零失败。 */
function runCargoCase(name) {
  const result = spawnSync(
    'cargo',
    ['test', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', name],
    { cwd: REPO_ROOT, encoding: 'utf8' },
  );
  if (result.error) {
    return { ok: false, detail: `cargo 不可用：${result.error.message}` };
  }
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  const passed = [...output.matchAll(/test result: ok\. (\d+) passed/g)].reduce(
    (sum, match) => sum + Number(match[1]),
    0,
  );
  const failed = [...output.matchAll(/(\d+) failed/g)].reduce(
    (sum, match) => sum + Number(match[1]),
    0,
  );
  if (passed === 0) {
    return { ok: false, detail: `过滤 ${name} 未命中任何测试`, output };
  }
  if (failed > 0 || result.status !== 0) {
    return { ok: false, detail: `${failed} 个测试失败`, output };
  }
  return { ok: true, detail: `${passed} 个测试通过` };
}

/** vitest 的 -t 是正则：案卷里的字面名先转义再匹配。 */
function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

const PACKAGE_BY_PREFIX = [
  ['apps/desktop/', '@nextalk/desktop'],
  ['apps/teleprompter/', '@nextalk/teleprompter'],
];

function runVitestCase(name) {
  const files = grepFiles(
    name,
    ['apps/desktop/src', 'apps/teleprompter/src'],
    ['*.test.ts', '*.test.tsx'],
  );
  const preferred = files.find((f) => f.startsWith('apps/desktop/')) ?? files[0];
  const entry = PACKAGE_BY_PREFIX.find(([prefix]) => preferred?.startsWith(prefix));
  if (!entry) {
    return { ok: false, detail: `找不到 ${JSON.stringify(name)} 所属的包` };
  }
  const result = spawnSync(
    'pnpm',
    ['--filter', entry[1], 'exec', 'vitest', 'run', '-t', escapeRegExp(name)],
    { cwd: REPO_ROOT, encoding: 'utf8' },
  );
  if (result.error) {
    return { ok: false, detail: `pnpm 不可用：${result.error.message}` };
  }
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  const passedMatch = /Tests\s+(\d+) passed/.exec(output);
  const passed = passedMatch ? Number(passedMatch[1]) : 0;
  if (passed === 0) {
    // -t 没命中时 vitest 会以 0 例通过退出——不算通过。
    return { ok: false, detail: `-t ${JSON.stringify(name)} 在 ${entry[1]} 未命中用例`, output };
  }
  if (result.status !== 0) {
    return { ok: false, detail: '存在失败用例', output };
  }
  return { ok: true, detail: `${passed} 个用例通过` };
}

function runCases(cases) {
  let passed = 0;
  let failed = 0;

  for (const { data } of cases) {
    const target = parseRegressionTarget(data.regression_test);
    if (target === null) {
      console.log(`FAIL ${data.id}  ${data.title} —— regression_test 格式非法`);
      failed += 1;
      continue;
    }
    const result =
      target.kind === 'cargo' ? runCargoCase(target.name) : runVitestCase(target.name);
    if (result.ok) {
      console.log(`ok   ${data.id}  ${data.title}（${result.detail}）`);
      passed += 1;
    } else {
      console.log(`FAIL ${data.id}  ${data.title} —— ${result.detail}`);
      if (result.output) {
        const tail = result.output.trimEnd().split('\n').slice(-15).join('\n');
        console.log(tail.replace(/^/gm, '     | '));
      }
      failed += 1;
    }
  }

  console.log(`\n失败案例库：${passed}/${passed + failed} 通过`);
  return failed === 0 ? 0 : 1;
}

function main() {
  const flags = new Set(process.argv.slice(2));
  if (flags.has('--check') && flags.has('--run')) {
    console.error('一次只跑一个（--check 或 --run）');
    return 2;
  }
  if (!flags.has('--check') && !flags.has('--run')) {
    console.error('用法: node run.mjs --check | --run');
    return 2;
  }

  withCargoOnPath();
  const cases = loadCases();
  console.log(`失败案例库：${cases.length} 条（${HERE}）`);

  if (flags.has('--check')) {
    const errors = runCheck(cases);
    if (errors.length > 0) {
      for (const error of errors) console.error(`FAIL ${error}`);
      console.error(`\n校验失败：${errors.length} 个问题`);
      return 1;
    }
    console.log(`校验通过：${cases.length} 条案例，字段/枚举/id/回归目标全部合规`);
    return 0;
  }

  // --run 前先把目标存在性校验一遍——防止 grep 失败被当作「运行失败」误报。
  const errors = runCheck(cases);
  if (errors.length > 0) {
    for (const error of errors) console.error(`FAIL ${error}`);
    console.error('\n案例库本身不合规，先修案卷再跑回归');
    return 1;
  }
  return runCases(cases);
}

process.exit(main());
