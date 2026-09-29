#!/usr/bin/env node
// stt-ab.mjs — Chinese STT A/B: Deepgram Nova-3 (pre-recorded) vs 讯飞流式听写 (WSS).
// Zero-dep (node >= 21 global WebSocket). Keys read from env, never echoed.
// Usage:
//   node tools/vendor-experiments/stt-ab.mjs <wav path> [reference text]
// Output: one JSON per vendor — transcript, latency, char-level match vs reference.

import fs from 'node:fs';
import https from 'node:https';
import crypto from 'node:crypto';

const WAV = process.argv[2];
const REF = (process.argv[3] ?? '').replace(/[，。？\s]/g, '');
if (!WAV || !fs.existsSync(WAV)) {
  console.error('usage: node stt-ab.mjs <wav path> [reference text]');
  process.exit(1);
}

/** Char-level match rate via Levenshtein on code points. */
function charMatch(transcript, ref) {
  const a = [...transcript.replace(/[，。？、\s]/g, '')];
  const b = [...ref];
  const dp = Array.from({ length: a.length + 1 }, () => Array(b.length + 1).fill(0));
  for (let i = 0; i <= a.length; i++) dp[i][0] = i;
  for (let j = 0; j <= b.length; j++) dp[0][j] = j;
  for (let i = 1; i <= a.length; i++)
    for (let j = 1; j <= b.length; j++)
      dp[i][j] = Math.min(dp[i - 1][j] + 1, dp[i][j - 1] + 1, dp[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
  const dist = dp[a.length][b.length];
  return b.length === 0 ? 1 : Math.max(0, 1 - dist / b.length);
}

/** Deepgram pre-recorded Nova-3. */
function runDeepgram() {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const req = https.request(
      {
        hostname: 'api.deepgram.com',
        path: '/v1/listen?model=nova-3&language=zh-CN&smart_format=true&punctuate=true',
        method: 'POST',
        headers: {
          Authorization: `Token ${process.env.DEEPGRAM_API_KEY}`,
          'Content-Type': 'audio/wav',
          'Content-Length': fs.statSync(WAV).size,
        },
      },
      (res) => {
        let raw = '';
        res.on('data', (c) => (raw += c.toString()));
        res.on('end', () => {
          const totalMs = Date.now() - started;
          try {
            const json = JSON.parse(raw);
            const transcript = json.results?.channels?.[0]?.alternatives?.[0]?.transcript ?? '';
            resolve({
              vendor: 'deepgram-nova3',
              transcript,
              latency_ms: totalMs,
              match_rate: Number(charMatch(transcript, REF).toFixed(3)),
            });
          } catch (err) {
            reject(new Error(`deepgram parse: ${err.message} raw=${raw.slice(0, 200)}`));
          }
        });
      },
    );
    req.on('error', reject);
    fs.createReadStream(WAV).pipe(req);
  });
}

/** 讯飞流式听写 v2/iat over WSS. */
function runXfyun() {
  const APP_ID = process.env.XFYUN_APP_ID;
  const API_KEY = process.env.XFYUN_API_KEY;
  const API_SECRET = process.env.XFYUN_API_SECRET;
  if (!APP_ID || !API_KEY || !API_SECRET) throw new Error('XFYUN_* env vars required');

  const host = 'iat-api.xfyun.cn';
  const date = new Date().toUTCString();
  const origin = `host: ${host}\ndate: ${date}\nGET /v2/iat HTTP/1.1`;
  const signature = crypto.createHmac('sha256', API_SECRET).update(origin).digest('base64');
  const auth = Buffer.from(`api_key="${API_KEY}", algorithm="hmac-sha256", headers="host date request-line", signature="${signature}"`).toString('base64');
  const url = `wss://${host}/v2/iat?authorization=${encodeURIComponent(auth)}&date=${encodeURIComponent(date)}&host=${host}&appid=${APP_ID}`;

  return new Promise((resolve, reject) => {
    const started = Date.now();
    const ws = new WebSocket(url);
    const pcm = fs.readFileSync(WAV).slice(44); // strip 44-byte WAV header -> raw PCM16 LE
    const CHUNK = 1280; // 40ms @ 16kHz/16bit
    let text = '';
    let status = 0;
    const sendFrame = (statusCode, audio) =>
      ws.send(
        JSON.stringify({
          common: { app_id: APP_ID },
          business: { language: 'zh_cn', domain: 'iat', accent: 'mandarin', dwa: 'wpgs' },
          data: { status: statusCode, format: 'audio/L16;rate=16000', encoding: 'raw', audio: audio.toString('base64') },
        }),
      );

    ws.onopen = () => {
      for (let off = 0; off < pcm.length; off += CHUNK) {
        const last = off + CHUNK >= pcm.length;
        sendFrame(last ? 2 : 1, pcm.subarray(off, Math.min(off + CHUNK, pcm.length)));
        if (last) break;
      }
      status = 2;
    };
    ws.onmessage = (event) => {
      try {
        const json = JSON.parse(event.data);
        if (json.code !== 0) {
          reject(new Error(`xfyun code ${json.code}: ${json.message}`));
          return;
        }
        const wsResult = json.data?.result?.ws;
        if (wsResult) {
          for (const seg of wsResult) for (const cw of seg.cw ?? []) text += cw.w;
        }
        if (json.data?.status === 2) {
          const totalMs = Date.now() - started;
          resolve({
            vendor: 'xfyun-iat',
            transcript: text,
            latency_ms: totalMs,
            match_rate: Number(charMatch(text, REF).toFixed(3)),
          });
        }
      } catch (err) {
        reject(new Error(`xfyun frame parse: ${err.message}`));
      }
    };
    ws.onerror = (err) => reject(new Error(`xfyun ws error: ${err.message ?? 'unknown'}`));
  });
}

console.log(JSON.stringify({ reference: REF, wav: WAV, bytes: fs.statSync(WAV).size }));
const deepgram = await runDeepgram();
console.log(JSON.stringify(deepgram, null, 2));
const xfyun = await runXfyun();
console.log(JSON.stringify(xfyun, null, 2));
