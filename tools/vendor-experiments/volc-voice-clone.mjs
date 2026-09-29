#!/usr/bin/env node
// volc-voice-clone.mjs — 火山声音复刻 2.0 (ICL 2.0) V3 training via the official API:
//   POST https://openspeech.bytedance.com/api/v3/tts/voice_clone
//   headers: X-Api-Key / X-Api-Request-Id, body: {speaker_id, audio:{data,format}, text, language}
// Zero-dep. Reads VOLC_TTS_ACCESS_TOKEN (X-Api-Key) + VOLC_CLONE_SPEAKER_ID (S_xxx from console).
// The training transcript (--text) must match the audio closely (WER gate, 45001109).
// Usage:
//   node tools/vendor-experiments/volc-voice-clone.mjs <audio.m4a> [transcript text]

import fs from 'node:fs';
import https from 'node:https';
import crypto from 'node:crypto';

const API_KEY = process.env.VOLC_TTS_ACCESS_TOKEN;
const SPEAKER_ID = process.env.VOLC_CLONE_SPEAKER_ID;
const AUDIO = process.argv[2];
const TEXT =
  process.argv[3] ??
  '大家好，我是做后端开发的，平时主要负责服务的性能优化和稳定性建设。今天想跟大家分享一下我最近做的一个数据库优化项目。';

if (!API_KEY) {
  console.error('VOLC_TTS_ACCESS_TOKEN (X-Api-Key) required');
  process.exit(1);
}
if (!SPEAKER_ID) {
  console.error('VOLC_CLONE_SPEAKER_ID (console S_xxx speaker id) required');
  process.exit(1);
}
if (!AUDIO || !fs.existsSync(AUDIO)) {
  console.error('usage: node volc-voice-clone.mjs <audio.m4a> [transcript]');
  process.exit(1);
}

const audioBytes = fs.readFileSync(AUDIO);
if (audioBytes.length > 10 * 1024 * 1024) {
  console.error('audio too large (>10MB per API limit)');
  process.exit(1);
}

const body = JSON.stringify({
  speaker_id: SPEAKER_ID,
  audio: { data: audioBytes.toString('base64'), format: 'm4a' },
  text: TEXT,
  language: 0, // 0 = 中文
});

const req = https.request(
  {
    hostname: 'openspeech.bytedance.com',
    path: '/api/v3/tts/voice_clone',
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Api-Key': API_KEY,
      'X-Api-Request-Id': crypto.randomUUID(),
      'Content-Length': Buffer.byteLength(body),
    },
  },
  (res) => {
    let raw = '';
    res.on('data', (c) => (raw += c.toString()));
    res.on('end', () => {
      console.log(JSON.stringify({ http: res.statusCode, logid: res.headers['x-tt-logid'] ?? null }));
      try {
        console.log(JSON.stringify(JSON.parse(raw), null, 2));
      } catch {
        console.log(raw.slice(0, 500));
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
