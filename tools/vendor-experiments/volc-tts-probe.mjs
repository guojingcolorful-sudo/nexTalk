#!/usr/bin/env node
// volc-tts-probe.mjs — try every 火山 TTS auth/resource combination against the
// TTS grant (VOLC_TTS_*), reporting which one works. Zero-dep. Keys never echoed.
// Usage: node tools/vendor-experiments/volc-tts-probe.mjs [zh text]

import https from 'node:https';
import crypto from 'node:crypto';

const APP_ID = process.env.VOLC_TTS_APP_ID;
const TOKEN = process.env.VOLC_TTS_ACCESS_TOKEN;
const TEXT = process.argv[2] ?? '你能详细说一下你优化数据库的具体步骤吗？';

const VARIANTS = [
  { name: 'v1 / volc.megatts.default', path: '/api/v1/tts', resource: 'volc.megatts.default' },
  { name: 'v1 / volc.service_type.10029', path: '/api/v1/tts', resource: 'volc.service_type.10029' },
  { name: 'v3 / volc.megatts.default', path: '/api/v3/tts/unidirectional', resource: 'volc.megatts.default' },
  { name: 'v3 / volc.service_type.10029', path: '/api/v3/tts/unidirectional', resource: 'volc.service_type.10029' },
];

function attempt(variant) {
  return new Promise((resolve) => {
    const body = JSON.stringify({
      app: { appid: APP_ID, token: 'access_token', cluster: 'volcano_tts' },
      user: { uid: 'nexTalk-exp' },
      audio: { voice_type: 'BV700_streaming', encoding: 'mp3', speed_ratio: 1.0 },
      request: { reqid: crypto.randomUUID(), text: TEXT, text_type: 'plain', operation: 'query' },
    });
    const req = https.request(
      {
        hostname: 'openspeech.bytedance.com',
        path: variant.path,
        method: 'POST',
        headers: {
          Authorization: `Bearer;${TOKEN}`,
          'Resource-Id': variant.resource,
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
            resolve({ variant: variant.name, http: res.statusCode, code: json.code, message: json.message });
          } catch {
            resolve({ variant: variant.name, http: res.statusCode, code: null, message: raw.slice(0, 120) });
          }
        });
      },
    );
    req.on('error', (err) => resolve({ variant: variant.name, http: 0, code: null, message: err.message }));
    req.write(body);
    req.end();
  });
}

if (!APP_ID || !TOKEN) {
  console.error('VOLC_TTS_APP_ID / VOLC_TTS_ACCESS_TOKEN required');
  process.exit(1);
}
console.log(`probing ${VARIANTS.length} resource-id variants for appid grant...`);
for (const variant of VARIANTS) {
  const result = await attempt(variant);
  console.log(JSON.stringify(result));
}
