#!/usr/bin/env node
// cross-lingual-clone-probe.mjs — 02-04 T4.0a：跨语种克隆可行性探针（阶段最高风险假设的裁决实验）。
//
// 问题：火山 ICL 2.0 的克隆音色（训练音频为中文）能不能说出**英文**，听起来还是不是用户本人？
// 文档自相矛盾：.env.example 声称「ICL 2.0 音色仅支持训练音频同语种合成」，而合成接口提供了
// `explicit_language` + `tone_fidelity: false` 的跨语种路径。D-11 盲听实验只验证了**中文**
// （MOS 5.0/5.0），英文输出从未被听过。本探针产出三份对照音频 + 结论 JSON，交给用户人耳判定
// （T4.0b blocking-human gate）。探针失败 → D-11 的单一供应商结论失效，按 GOV-16 触发第二
// TTS 供应商决策，不得静默继续。
//
// 三份对照（全部落 tools/vendor-experiments/artifacts/，文件名带 UTC 时间戳）：
//   A clone-en  克隆音色 + 英文技术面试句 + seed-icl-2.0 + explicit_language='en' + tone_fidelity=false
//   B clone-zh  克隆音色 + 同句中译 + seed-icl-2.0（同语种基线，验证克隆链路本身未退化）
//   C preset-en 预置音色 + 同一英文句 + seed-tts-2.0（音质下限对照；预置路径参数与生产
//               客户端 request_body() 一致——若 C 失败，说明预置路径也需要补参数，T4.3 消费此结论）
//
// 线协议来源（不重写、照抄并注明）：
//   - 二进制帧 [0x11,0x10,0x10,0x00]+u32len+JSON 与帧解析：tools/vendor-experiments/volc-tts-stream.mjs
//     （2026-09-29 对实测服务验证；event 352=音频 / 152=SESSION_FINISHED / msgType 0b1111=错误）
//   - voice_clone 训练请求构造（仅当 VOLC_CLONE_SPEAKER_ID 缺失时的回退路径）：
//     tools/vendor-experiments/volc-voice-clone.mjs（POST /api/v3/tts/voice_clone 的 body 逐字段一致）
//
// 凭据（README 政策：运行时只从进程环境读，绝不落盘、绝不入 CLI 参数）：
//   VOLC_CLONE_ACCESS_TOKEN（优先，回退 VOLC_TTS_ACCESS_TOKEN）
//   VOLC_CLONE_SPEAKER_ID  盲听实验已产出的 S_xxx（避免重复训练）
//
// 用法：
//   set -a; source tools/vendor-experiments/.env; set +a
//   node tools/vendor-experiments/cross-lingual-clone-probe.mjs
//
// 退出码（供 gate 区分「假设失败」与「环境问题」）：
//   0  三份对照都产出音频
//   2  能力级失败（克隆英文路径被拒或不支持）——与假设直接相关
//   3  凭据/网络问题（与假设无关，需重跑）

import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import https from 'node:https';
import { fileURLToPath } from 'node:url';
import WebSocket from 'ws';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ARTIFACTS_DIR = path.join(HERE, 'artifacts');
const RESULTS_PATH = path.join(HERE, 'blind-clone-results.json');
const FAILURE_CASES_DIR = path.join(HERE, 'failure-cases');

// 02-03 已占用 0001..0020 的案例编号——计划文本写的 0003 早被数字漂移案例使用，
// 跨语种克隆案例顺延到下一个空号（runner 强制 id 唯一且四位零填充）。
const FAILURE_CASE_NAME = '0021-cross-lingual-clone.json';
// 当前仓库中真实存在、且语义最接近的防线：克隆资源握手/能力被拒 → 归类为终止失败且不回显凭据
// （tests/mock_vendors.rs）。T4.0b 判定后由 02-04 后续任务升级为正式回归（预置音色回退路径）。
const FAILURE_CASE_REGRESSION = 'cargo: volc_client_classifies_a_rejected_handshake_and_leaks_nothing';

const TTS_URL = 'wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream';
const VOICE_CLONE_URL = 'https://openspeech.bytedance.com/api/v3/tts/voice_clone';
const EVT_TTS_RESPONSE = 352;
const EVT_SESSION_FINISHED = 152;
const STATUS_SUCCESS = 20000000;
const RESOURCE_CLONE = 'seed-icl-2.0';
const RESOURCE_PRESET = 'seed-tts-2.0';
const PRESET_VOICE = 'zh_female_vv_uranus_bigtts';
const PROBE_TIMEOUT_MS = 45_000;
const MAX_TRAIN_BYTES = 10 * 1024 * 1024; // voice_clone 硬上限

// 固定对照句（便于人耳横向对比；含 p95/800/120 等数字与术语，侧面检查跨语种读法）。
const EN_TEXT =
  'I optimized the database by adding a composite index, cutting p95 latency from 800 milliseconds to 120.';
const ZH_TEXT = '我通过添加复合索引优化了数据库，把 p95 延迟从 800 毫秒降到了 120 毫秒。';

const TOKEN = process.env.VOLC_CLONE_ACCESS_TOKEN || process.env.VOLC_TTS_ACCESS_TOKEN || '';
const TOKEN_ENV = process.env.VOLC_CLONE_ACCESS_TOKEN
  ? 'VOLC_CLONE_ACCESS_TOKEN'
  : 'VOLC_TTS_ACCESS_TOKEN';

function log(message) {
  console.log(`[probe] ${message}`);
}

/** 帧解析：照抄 volc-tts-stream.mjs 的 parse()（该脚本对实测服务验证过）。 */
function parseFrame(data) {
  const msgType = (data[1] >> 4) & 0x0f;
  if (msgType === 0b1111) {
    const code = data.readUInt32BE(4);
    const size = data.readUInt32BE(8);
    return { error: code, message: data.subarray(12, 12 + size).toString('utf-8') };
  }
  let off = 4;
  const event = data.readUInt32BE(off);
  off += 4;
  const sidLen = data.readUInt32BE(off);
  off += 4 + sidLen;
  const payLen = data.readUInt32BE(off);
  off += 4;
  const body = data.subarray(off, off + payLen);
  if (msgType === 0b1011) return { event, audio: body };
  let json = null;
  try {
    json = body.length > 0 ? JSON.parse(body.toString('utf-8')) : null;
  } catch {
    json = null;
  }
  return { event, json };
}

/**
 * 一次合成探针。返回记录：
 * { label, resourceId, voice, params, authEnv, ok, error?, failure?, statusCode?, first_audio_ms, total_ms, bytes, out }
 * failure 仅用于退出码归因：'capability'（服务端明确的请求级拒绝/零音频）与
 * 'network'|'auth'|'protocol'（环境问题，重跑）。
 */
function synthProbe({ label, resourceId, voice, text, params, token, authEnv, outPath }) {
  return new Promise((resolve) => {
    const started = Date.now();
    const chunks = [];
    let firstAudioMs = null;
    let settled = false;

    const record = {
      label,
      resourceId,
      voice,
      params,
      authEnv,
      ok: false,
      error: null,
      failure: null,
      statusCode: null,
      first_audio_ms: null,
      total_ms: null,
      bytes: null,
      out: null,
    };

    const payload = Buffer.from(
      JSON.stringify({
        user: { uid: 'nexTalk-probe' },
        req_params: {
          text,
          speaker: voice,
          audio_params: { format: 'mp3', sample_rate: 24000, ...params },
        },
      }),
      'utf-8',
    );
    const len = Buffer.alloc(4);
    len.writeUInt32BE(payload.length);
    const frame = Buffer.concat([Buffer.from([0x11, 0x10, 0x10, 0x00]), len, payload]);

    const ws = new WebSocket(TTS_URL, {
      headers: {
        'X-Api-Key': token,
        'X-Api-Resource-Id': resourceId,
        'X-Api-Request-Id': crypto.randomUUID(),
      },
      maxPayload: 20 * 1024 * 1024,
    });

    const finish = (patch) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      Object.assign(record, patch);
      try {
        ws.close();
      } catch {
        /* already closed */
      }
      resolve(record);
    };

    const timer = setTimeout(() => {
      finish({ error: `timeout after ${PROBE_TIMEOUT_MS}ms`, failure: 'network' });
    }, PROBE_TIMEOUT_MS);

    ws.on('unexpected-response', (_req, res) => {
      let body = '';
      res.on('data', (c) => (body += c.toString()));
      res.on('end', () => {
        finish({
          statusCode: res.statusCode,
          error: `handshake HTTP ${res.statusCode}: ${body.slice(0, 300)}`,
          failure: res.statusCode === 401 || res.statusCode === 403 ? 'auth' : 'network',
        });
      });
    });
    ws.on('open', () => ws.send(frame));
    ws.on('message', (data) => {
      let msg;
      try {
        msg = parseFrame(data);
      } catch (err) {
        finish({ error: `unparseable frame: ${err.message}`, failure: 'protocol' });
        return;
      }
      if (msg.error !== undefined) {
        finish({
          statusCode: msg.error,
          error: `server error frame ${msg.error}: ${msg.message}`,
          failure: 'capability',
        });
        return;
      }
      if (msg.event === EVT_TTS_RESPONSE && msg.audio) {
        if (firstAudioMs === null) firstAudioMs = Date.now() - started;
        chunks.push(msg.audio);
        return;
      }
      if (msg.event === EVT_SESSION_FINISHED) {
        const status = msg.json?.status_code ?? null;
        if (status !== null && status !== STATUS_SUCCESS) {
          finish({
            statusCode: status,
            error: `session status_code ${status}: ${JSON.stringify(msg.json).slice(0, 300)}`,
            failure: 'capability',
          });
          return;
        }
        if (chunks.length === 0) {
          finish({ statusCode: status, error: 'session finished with zero audio frames', failure: 'capability' });
          return;
        }
        const buf = Buffer.concat(chunks);
        fs.mkdirSync(path.dirname(outPath), { recursive: true });
        fs.writeFileSync(outPath, buf);
        finish({
          ok: true,
          statusCode: status,
          first_audio_ms: firstAudioMs,
          total_ms: Date.now() - started,
          bytes: buf.length,
          out: path.relative(HERE, outPath),
        });
        return;
      }
      // 其他事件（如用量 JSON）与参考脚本一致：忽略。
    });
    ws.on('error', (err) => {
      finish({ error: `${err.code ?? 'ws'}: ${err.message}`, failure: 'network' });
    });
    ws.on('close', (code) => {
      if (!settled) {
        finish({ error: `socket closed before completion (code ${code})`, failure: 'network' });
      }
    });
  });
}

/**
 * 训练回退：仅当 VOLC_CLONE_SPEAKER_ID 缺失时使用。训练请求逐字段照抄
 * volc-voice-clone.mjs（speaker_id 是调用方给定的字段——因此新训需要一个目标 id，
 * 由 VOLC_CLONE_NEW_SPEAKER_ID 提供；探针绝不自行编造音色 id）。
 */
function trainVoiceClone(speakerId, samplePath) {
  return new Promise((resolve, reject) => {
    const bytes = fs.readFileSync(samplePath);
    if (bytes.length > MAX_TRAIN_BYTES) {
      reject(new Error(`sample > 10MB (${bytes.length} bytes) — voice_clone rejects it`));
      return;
    }
    const body = JSON.stringify({
      speaker_id: speakerId,
      audio: { data: bytes.toString('base64'), format: path.extname(samplePath).slice(1) || 'wav' },
      text: process.env.VOLC_CLONE_SAMPLE_TEXT ?? '大家好，我是做后端开发的，平时主要负责服务的性能优化和稳定性建设。',
      language: 0, // 训练音频语言：中文
    });
    const req = https.request(
      {
        hostname: 'openspeech.bytedance.com',
        path: '/api/v3/tts/voice_clone',
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Api-Key': TOKEN,
          'X-Api-Request-Id': crypto.randomUUID(),
          'Content-Length': Buffer.byteLength(body),
        },
      },
      (res) => {
        let raw = '';
        res.on('data', (c) => (raw += c.toString()));
        res.on('end', () => {
          if (res.statusCode !== 200) {
            reject(new Error(`voice_clone HTTP ${res.statusCode}: ${raw.slice(0, 300)}`));
            return;
          }
          resolve({ http: res.statusCode, body: raw.slice(0, 300) });
        });
      },
    );
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

async function resolveSpeakerId() {
  if (process.env.VOLC_CLONE_SPEAKER_ID) return process.env.VOLC_CLONE_SPEAKER_ID;
  const newId = process.env.VOLC_CLONE_NEW_SPEAKER_ID;
  const sample = process.env.VOLC_CLONE_SAMPLE ?? path.join(process.env.HOME ?? '', 'nexTalk', 'voice-sample.m4a');
  if (!newId || !fs.existsSync(sample)) {
    console.error(
      'VOLC_CLONE_SPEAKER_ID missing — the probe synthesizes through the clone registered by the\n' +
        'blind experiment (D-11). To train a fresh clone first, set VOLC_CLONE_NEW_SPEAKER_ID (a new\n' +
        `S_xxx) and VOLC_CLONE_SAMPLE (a 30s–3min Chinese recording; default ${sample}).`,
    );
    process.exit(3);
  }
  log(`no clone on record — training ${newId} from ${sample} (request shape ported from volc-voice-clone.mjs)`);
  const trained = await trainVoiceClone(newId, sample);
  log(`voice_clone training accepted: ${JSON.stringify(trained)}`);
  return newId;
}

/** 把 cross_lingual 段写回 blind-clone-results.json——既有盲听结论原样保留。 */
function writeResults(block) {
  const data = JSON.parse(fs.readFileSync(RESULTS_PATH, 'utf8'));
  data.cross_lingual = block; // 重跑即更新本段；盲听段（date/test/samples/verdict/decision）不动
  fs.writeFileSync(RESULTS_PATH, `${JSON.stringify(data, null, 2)}\n`);
}

/** 失败即入库（D-20）：能力级失败沉淀为案例 0021（计划文本的 0003 编号已被占用）。 */
function writeFailureCase(recordA) {
  const casePath = path.join(FAILURE_CASES_DIR, FAILURE_CASE_NAME);
  const failure = {
    id: '0021',
    title: '跨语种克隆失败：克隆音色无法按 explicit_language=en 输出英文（T4.0a 探针实测）',
    root_cause: '其他：火山 ICL 2.0 克隆资源拒绝跨语种英文合成（原始错误见 blind-clone-results.json）',
    source: '人工抽检（02-04 T4.0a 跨语种克隆探针，真实凭据实测）',
    input: `克隆音色 ${recordA.voice}（seed-icl-2.0）+ 英文句；params=${JSON.stringify(recordA.params)}`,
    wrong_output: `克隆英文路径无音频产出：${recordA.error}`,
    expected_output: '克隆音色输出英文且保持说话人相似度（T4.0b 人耳判定）；失败时按 GOV-16 触发第二 TTS 供应商决策',
    fix: '探针失败后不得静默继续：按 GOV-16 由用户拍板第二 TTS 供应商（MiniMax speech-2.6-turbo / Cartesia sonic-3.5）；管线在无可用克隆时回退预置音色（T4.2/T4.3，回退路径落地后本案例的回归目标升级为该路径的测试）',
    regression_test: FAILURE_CASE_REGRESSION,
  };
  fs.writeFileSync(casePath, `${JSON.stringify(failure, null, 2)}\n`);
  log(`failure case written: ${path.relative(HERE, casePath)}`);
}

async function main() {
  if (process.argv.includes('--help')) {
    console.log('usage: node cross-lingual-clone-probe.mjs   (creds via env; see header)');
    console.log('exit: 0 all three artifacts | 2 capability failure (clone-en) | 3 credentials/network');
    return 0;
  }
  if (!TOKEN) {
    console.error('VOLC_CLONE_ACCESS_TOKEN (or VOLC_TTS_ACCESS_TOKEN) required — exit 3');
    return 3;
  }

  const speaker = await resolveSpeakerId();
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  log(`speaker=${speaker} resource=clone:${RESOURCE_CLONE} preset:${RESOURCE_PRESET} tokenEnv=${TOKEN_ENV}`);

  const targets = [
    {
      label: 'clone-en',
      resourceId: RESOURCE_CLONE,
      voice: speaker,
      text: EN_TEXT,
      params: { explicit_language: 'en', tone_fidelity: false },
    },
    {
      label: 'clone-zh',
      resourceId: RESOURCE_CLONE,
      voice: speaker,
      text: ZH_TEXT,
      params: {}, // 同语种基线：与 volc-tts-stream.mjs / 盲听实验一致的默认参数
    },
    {
      label: 'preset-en',
      resourceId: RESOURCE_PRESET,
      voice: PRESET_VOICE,
      text: EN_TEXT,
      params: { explicit_language: 'en', tone_fidelity: false }, // 与生产 request_body() 完全一致
    },
  ];

  const artifacts = [];
  for (const target of targets) {
    log(`synth ${target.label} …`);
    const record = await synthProbe({
      ...target,
      token: TOKEN,
      authEnv: TOKEN_ENV,
      outPath: path.join(ARTIFACTS_DIR, `${stamp}-${target.label}.mp3`),
    });
    log(
      record.ok
        ? `${target.label} ok: first_audio=${record.first_audio_ms}ms total=${record.total_ms}ms bytes=${record.bytes}`
        : `${target.label} FAILED: ${record.error}`,
    );
    artifacts.push(record);
  }

  const [a, b, c] = artifacts;
  const allOk = a.ok && b.ok && c.ok;
  const envFailure = artifacts.some(
    (p) => !p.ok && p.failure !== 'capability',
  );

  let exitCode;
  if (allOk) exitCode = 0;
  else if (envFailure) exitCode = 3;
  else if (!a.ok && !b.ok && !c.ok) exitCode = 3; // 服务端整体异常，无法归因 → 重跑
  else exitCode = 2; // 环境健康下出现能力级失败

  writeResults({
    probed_at: new Date().toISOString(),
    speaker_id: speaker,
    explicit_language: 'en',
    tone_fidelity: false,
    artifacts,
    exit_code: exitCode,
    verdict: 'pending_user_judgement',
  });

  if (exitCode === 2 && !a.ok) {
    writeFailureCase(a);
  }

  console.log('');
  console.log(`probe result: ${exitCode === 0 ? 'ALL THREE ARTIFACTS' : 'NOT ALL OK'} (exit ${exitCode})`);
  for (const record of artifacts) {
    console.log(
      `  ${record.ok ? 'ok  ' : 'FAIL'} ${record.label.padEnd(9)} ${record.ok ? record.out : record.error}`,
    );
  }
  if (exitCode === 0) {
    console.log('\nNext: T4.0b — listen to A (clone-en) vs B (clone-zh) vs C (preset-en) and give the verdict.');
  }
  return exitCode;
}

main()
  .then((code) => process.exit(code))
  .catch((err) => {
    // 未预期崩溃按环境问题处理（exit 3）——verify 门只看 exit 3，绝不让崩溃伪装成通过。
    console.error(`[probe] unexpected error: ${err?.stack ?? err}`);
    process.exit(3);
  });
