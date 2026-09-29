#!/usr/bin/env node
// volc-tts-stream.mjs — 豆包语音合成大模型 2.0 (Seed-TTS 2.0) unidirectional streaming TTS.
// Protocol verified against the official demo flow (2026-09-29):
//   wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream
//   headers: X-Api-App-Id / X-Api-Access-Key / X-Api-Resource-Id: seed-tts-2.0 / X-Api-Request-Id
//   send: one binary frame [0x11,0x10,0x10,0x00] + u32 BE len + JSON
//         {"user":{"uid"}, "req_params":{"text","speaker","audio_params":{format,sample_rate}}}
//   recv frames: msg_type = byte1>>4 (0b1011 audio / 0b1001 json / 0b1111 error);
//         non-error: event u32 @4, sid_len u32 @8, session_id, payload_len u32, payload
// Uses the `ws` package (repo devDependency — Node's built-in WebSocket cannot set
// custom handshake headers).
// Usage: node tools/vendor-experiments/volc-tts-stream.mjs [zh text] [out.mp3]

import fs from 'node:fs';
import crypto from 'node:crypto';
import WebSocket from 'ws';

const APP_ID = process.env.VOLC_TTS_APP_ID;
const TOKEN = process.env.VOLC_TTS_ACCESS_TOKEN;
const VOICE = process.env.VOLC_TTS_VOICE ?? 'zh_female_vv_uranus_bigtts';
// seed-tts-2.0 = 预置音色；seed-icl-2.0 = 克隆音色（speaker 填 S_xxx 或 ICL_xxx）
const RESOURCE = process.env.VOLC_TTS_RESOURCE ?? 'seed-tts-2.0';
const TEXT =
  process.argv[2] ??
  '你能详细说一下你优化数据库的具体步骤吗？我们通过慢查询日志发现了商品详情页的连表查询瓶颈。';
const OUT = process.argv[3] ?? '/tmp/nexTalk-volc.mp3';

if (!APP_ID || !TOKEN) {
  console.error('VOLC_TTS_APP_ID / VOLC_TTS_ACCESS_TOKEN required');
  process.exit(1);
}

const URL = 'wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream';
const EVT_SESSION_FINISHED = 152;
const EVT_TTS_RESPONSE = 352;

const payload = Buffer.from(
  JSON.stringify({
    user: { uid: 'nexTalk-exp' },
    req_params: {
      text: TEXT,
      speaker: VOICE,
      audio_params: { format: 'mp3', sample_rate: 24000 },
    },
  }),
  'utf-8',
);
const len = Buffer.alloc(4);
len.writeUInt32BE(payload.length);
const frame = Buffer.concat([Buffer.from([0x11, 0x10, 0x10, 0x00]), len, payload]);

function parse(data) {
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

const ws = new WebSocket(URL, {
  headers: {
    'X-Api-Key': TOKEN,
    'X-Api-Resource-Id': RESOURCE,
    'X-Api-Request-Id': crypto.randomUUID(),
  },
  maxPayload: 20 * 1024 * 1024,
});

const audio = [];
let resolved = false;
const finish = (ok, detail) => {
  if (resolved) return;
  resolved = true;
  console.log(JSON.stringify(detail));
  if (ok && audio.length > 0) {
    const buf = Buffer.concat(audio);
    fs.writeFileSync(OUT, buf);
    console.log(JSON.stringify({ ok: true, out: OUT, bytes: buf.length, voice: VOICE, text: TEXT }));
    process.exit(0);
  }
  process.exit(1);
};

const started = Date.now();
ws.on('open', () => ws.send(frame));
ws.on('message', (data) => {
  const msg = parse(data);
  if (msg.error !== undefined) {
    finish(false, { ok: false, error: msg.error, message: msg.message });
    return;
  }
  if (msg.event === EVT_TTS_RESPONSE && msg.audio) {
    audio.push(msg.audio);
    return;
  }
  if (msg.event === EVT_SESSION_FINISHED) {
    const status = msg.json?.status_code;
    if (status !== undefined && status !== 20000000) {
      finish(false, { ok: false, session_status: status, body: msg.json });
      return;
    }
    finish(true, { ok: true, latency_ms: Date.now() - started, audio_frames: audio.length, usage: msg.json?.usage ?? null });
    return;
  }
  console.error('[frame]', msg.event ?? '?', msg.json ? JSON.stringify(msg.json).slice(0, 120) : '');
});
ws.on('error', (err) => finish(false, { ok: false, error: err.message }));
ws.on('close', (code) => {
  if (!resolved) finish(false, { ok: false, closed: code, audio_frames: audio.length });
});
