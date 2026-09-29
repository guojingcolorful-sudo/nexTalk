#!/usr/bin/env node
// translation-probe.mjs — DeepSeek streaming translation latency probe (D-04 zero-dep).
// Measures TTFT (time to first token) + total + token count for a technical
// interview sentence. Reads DEEPSEEK_API_KEY from env; never writes keys to output.
// Usage: DEEPSEEK_API_KEY=... node tools/vendor-experiments/translation-probe.mjs [zh sentence]

import https from 'node:https';

const API_KEY = process.env.DEEPSEEK_API_KEY;
if (!API_KEY) {
  console.error('DEEPSEEK_API_KEY is required (read from env, never committed)');
  process.exit(1);
}

const ZH =
  process.argv[2] ??
  '你能详细说一下你优化数据库的具体步骤吗？我们通过慢查询日志发现了商品详情页的连表查询瓶颈，单次响应超过 800 毫秒。';
const EN_REF =
  'Could you walk me through the specific steps you took to optimize the database?';

const body = JSON.stringify({
  model: 'deepseek-chat',
  messages: [
    {
      role: 'system',
      content:
        'You are a faithful CN->EN translator for a technical interview. Translate the user sentence into fluent spoken English. Preserve numbers, units and technical terms exactly. Output only the translation, no explanations.',
    },
    { role: 'user', content: ZH },
  ],
  temperature: 0,
  stream: true,
  stream_options: { include_usage: true },
});

const started = Date.now();
let ttftMs = null;
let out = '';
let usage = null;

const req = https.request(
  {
    hostname: 'api.deepseek.com',
    path: '/chat/completions',
    method: 'POST',
    headers: {
      Authorization: `Bearer ${API_KEY}`,
      'Content-Type': 'application/json',
      Accept: 'text/event-stream',
      'Content-Length': Buffer.byteLength(body),
    },
  },
  (res) => {
    if (res.statusCode !== 200) {
      console.error('HTTP', res.statusCode);
      res.resume();
      process.exit(1);
    }
    let buffer = '';
    res.on('data', (chunk) => {
      if (ttftMs === null) ttftMs = Date.now() - started;
      // SSE events can split across TCP chunks — buffer and parse complete
      // events only, never per-chunk lines (a split event silently drops
      // tokens otherwise).
      buffer += chunk.toString();
      let boundary;
      while ((boundary = buffer.indexOf('\n\n')) !== -1) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        for (const line of event.split('\n')) {
          if (!line.startsWith('data:')) continue;
          const payload = line.slice(5).trim();
          if (payload === '[DONE]') continue;
          try {
            const json = JSON.parse(payload);
            if (json.choices?.[0]?.delta?.content) out += json.choices[0].delta.content;
            if (json.usage) usage = json.usage;
          } catch {
            /* keep-alive lines */
          }
        }
      }
    });
    res.on('end', () => {
      const totalMs = Date.now() - started;
      console.log(
        JSON.stringify(
          {
            input_chars: ZH.length,
            ttft_ms: ttftMs,
            total_ms: totalMs,
            output_chars: out.length,
            output_tokens: usage?.completion_tokens ?? null,
            prompt_tokens: usage?.prompt_tokens ?? null,
            chars_per_sec_after_ttft: ttftMs
              ? Math.round((out.length / (totalMs - ttftMs)) * 1000)
              : null,
            translation: out,
          },
          null,
          2,
        ),
      );
      console.log(`reference: ${EN_REF}`);
    });
  },
);
req.on('error', (err) => {
  console.error('request failed:', err.message);
  process.exit(1);
});
req.write(body);
req.end();
