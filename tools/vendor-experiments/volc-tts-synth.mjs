#!/usr/bin/env node
// volc-tts-synth.mjs — synthesize a Chinese test sentence via 火山引擎 通用 TTS (D-04 zero-dep).
// Reads VOLC_TTS_APP_ID / VOLC_TTS_ACCESS_TOKEN from env; writes the mp3 to a file.
// 声音复刻（volc.megatts.voiceclone）的 key 另存于 VOLC_CLONE_*，供克隆盲听实验专用。
// Usage: node tools/vendor-experiments/volc-tts-synth.mjs [zh text] [out.mp3]

import https from 'node:https';
import fs from 'node:fs';
import crypto from 'node:crypto';

// 通用 TTS 需要独立的资源授权（Resource-Id: volc.megatts.default）——
// 与声音复刻（volc.megatts.voiceclone）的 key 不通用，故分开存放。
const APP_ID = process.env.VOLC_TTS_APP_ID;
const TOKEN = process.env.VOLC_TTS_ACCESS_TOKEN;
if (!APP_ID || !TOKEN) {
  console.error('VOLC_TTS_APP_ID / VOLC_TTS_ACCESS_TOKEN are required (通用 TTS 未开通——声音复刻 key 不适用于本脚本)');
  process.exit(1);
}

const TEXT =
  process.argv[2] ??
  '你能详细说一下你优化数据库的具体步骤吗？我们通过慢查询日志发现了商品详情页的连表查询瓶颈。';
const OUT = process.argv[3] ?? '/tmp/nexTalk-stt-test.mp3';

const body = JSON.stringify({
  app: { appid: APP_ID, token: 'access_token', cluster: 'volcano_tts' },
  user: { uid: 'nexTalk-exp' },
  audio: { voice_type: 'BV700_streaming', encoding: 'mp3', speed_ratio: 1.0 },
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
      Authorization: `Bearer;${TOKEN}`,
      'Resource-Id': 'volc.megatts.default',
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
        if (json.code !== 3000) {
          console.error('volc tts error:', json.code, json.message ?? raw.slice(0, 200));
          process.exit(1);
        }
        fs.writeFileSync(OUT, Buffer.from(json.data, 'base64'));
        console.log(JSON.stringify({ code: json.code, bytes: Buffer.byteLength(json.data, 'base64'), out: OUT, text: TEXT }));
      } catch (err) {
        console.error('parse error:', err.message, raw.slice(0, 200));
        process.exit(1);
      }
    });
  },
);
req.on('error', (err) => {
  console.error('request failed:', err.message);
  process.exit(1);
});
req.write(body);
req.end();
