#!/usr/bin/env node
// volc-tts-synth.mjs — synthesize a Chinese test sentence via 火山豆包语音合成大模型 2.0
// (Seed-TTS 2.0, unidirectional). Zero-dep. Keys read from env, never echoed.
// Auth: X-Api-App-Id / X-Api-Access-Key / X-Api-Resource-Id: seed-tts-2.0（与声音复刻共用鉴权）
// Usage: node tools/vendor-experiments/volc-tts-synth.mjs [zh text] [out.mp3]

import https from 'node:https';
import fs from 'node:fs';
import crypto from 'node:crypto';

const APP_ID = process.env.VOLC_TTS_APP_ID;
const TOKEN = process.env.VOLC_TTS_ACCESS_TOKEN;
if (!APP_ID || !TOKEN) {
  console.error('VOLC_TTS_APP_ID / VOLC_TTS_ACCESS_TOKEN are required (与声音复刻共用鉴权)');
  process.exit(1);
}

const TEXT =
  process.argv[2] ??
  '你能详细说一下你优化数据库的具体步骤吗？我们通过慢查询日志发现了商品详情页的连表查询瓶颈。';
const OUT = process.argv[3] ?? '/tmp/nexTalk-volc.mp3';

// 音色：豆包语音合成大模型 2.0 预置音色（用户控制台确认，2026-09-29）
const VOICE = process.env.VOLC_TTS_VOICE ?? 'zh_female_vv_uranus_bigtts';

function synth(voice) {
  return new Promise((resolve) => {
    // 已确认的鉴权组合（2026-09-29 实测）：/api/v1/tts + X-Api-* 头 +
    // Resource-Id: seed-tts-2.0 + 经典 v1 body schema
    const body = JSON.stringify({
      app: { appid: APP_ID, token: 'access_token', cluster: 'volcano_tts' },
      user: { uid: 'nexTalk-exp' },
      audio: { voice_type: voice, encoding: 'mp3', speed_ratio: 1.0 },
      request: {
        reqid: crypto.randomUUID(),
        text: TEXT,
        text_type: 'plain',
        operation: 'query',
      },
    });
    const req = https.request(
      {
        hostname: 'openspeech.bytedance.com',
        path: '/api/v1/tts',
        method: 'POST',
        headers: {
          'X-Api-App-Id': APP_ID,
          'X-Api-Access-Key': TOKEN,
          'X-Api-Resource-Id': 'seed-tts-2.0',
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(body),
        },
      },
      (res) => {
        let raw = '';
        res.on('data', (c) => (raw += c.toString()));
        res.on('end', () => {
          try {
            const json = JSON.parse(raw);
            resolve({ voice, http: res.statusCode, code: json.code, message: json.message, data: json.data });
          } catch {
            resolve({ voice, http: res.statusCode, code: null, message: raw.slice(0, 160) });
          }
        });
      },
    );
    req.on('error', (err) => resolve({ voice, http: 0, code: null, message: err.message }));
    req.write(body);
    req.end();
  });
}

const result = await synth(VOICE);
console.log(JSON.stringify({ voice: VOICE, http: result.http, code: result.code, message: result.message }));
if (result.code === 3000 && result.data) {
  fs.writeFileSync(OUT, Buffer.from(result.data, 'base64'));
  console.log(JSON.stringify({ ok: true, out: OUT, bytes: Buffer.byteLength(result.data, 'base64'), voice: VOICE, text: TEXT }));
  process.exit(0);
}
process.exit(1);
