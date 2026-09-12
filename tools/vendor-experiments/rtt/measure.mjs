#!/usr/bin/env node
/**
 * measure.mjs — provider round-trip-time measurement for the D-04 vendor
 * experiments.
 *
 * What it measures: for one endpoint and one request body, the wall-clock
 * latency of N **sequential** requests, split into
 *   - ttfb_ms  : send -> response headers (the vendors' "first byte")
 *   - total_ms : send -> response end
 * Concurrency is deliberately absent: the product budget is per-request
 * latency on a warm connection, not throughput under load.
 *
 * Why it is vendor-agnostic: the endpoint, the body, the headers, and the
 * *name* of the credential's environment variable all come from the caller.
 * Nothing vendor-specific is baked in, so the Phase 2 planner can point it at
 * Gemini / Deepgram / MiniMax / Cartesia / Fish / Brave without editing this
 * file.
 *
 * Credentials (Phase 1 policy — see ../README.md): a key is read **only** from
 * the process environment, **only** at run time, and **only** when --auth-env
 * names the variable. No key is accepted on the command line — a command line
 * ends up in shell history. The value never appears in the output: the report
 * records `hasKey`, never the key. In Phase 1 the only sanctioned invocation is
 * `--help`, which performs no network call at all.
 */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { request as httpsRequest } from 'node:https';
import { dirname, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const USAGE = `measure.mjs — sequential round-trip-time measurement for one vendor endpoint (D-04).

Usage:
  node tools/vendor-experiments/rtt/measure.mjs --url <https URL> [options]

Required (or its environment fallback):
  --url <url>          Endpoint to call. Must be https:// (this tool speaks TLS
                       only and refuses cleartext). env: NEXTALK_EXPERIMENT_URL
  --payload <file>     JSON file sent as the request body. env:
                       NEXTALK_EXPERIMENT_PAYLOAD; optional when the endpoint
                       takes no body

Options:
  --header "K: V"      Extra request header; repeatable
  --auth-env <NAME>    Name of the environment variable holding the credential.
                       The NAME is passed, never the value. Missing variable is
                       a hard error before any request is sent
  --auth-header <name> Header the credential goes in (default: Authorization)
  --auth-scheme <word> Space-separated prefix before the credential. Default
                       "Bearer"; use "" for a raw value. E.g. Deepgram wants
                       --auth-scheme Token, Gemini wants
                       --auth-header x-goog-api-key --auth-scheme ""
  --method <verb>      HTTP method (default: POST)
  --runs <n>           Sequential requests to send (default: 10)
  --out <file>         Where the JSON report is written
                       (default: tools/vendor-experiments/rtt/OUTPUT.json)
  --timeout <ms>       Per-request timeout (default: 30000)
  -h, --help           Print this text and exit 0 — performs no network call

Output: one line per request, then min / p50 / p95 / max for ttfb_ms and
total_ms, plus the JSON report documented in ../README.md. The exit code is 0
only when every request succeeded.

Ground rules (see ../README.md): warm up with a discarded pass — the first
request of a cold connection carries DNS and TLS; run every vendor from one
machine on one network; keep the requests sequential.`;

/** A caller mistake: bad flag, missing file, refused URL. Exits 1 with usage. */
class UsageError extends Error {}

const out = (line = '') => process.stdout.write(`${line}\n`);
const fail = (line) => process.stderr.write(`${line}\n`);

const HELP_FLAGS = new Set(['-h', '--help']);
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

function defaultOutPath() {
  return fileURLToPath(new URL('./OUTPUT.json', import.meta.url));
}

function parseHeader(raw) {
  const separator = raw.indexOf(':');
  if (separator < 1) {
    throw new UsageError(`--header needs "Name: value", got: ${raw}`);
  }
  return { name: raw.slice(0, separator).trim(), value: raw.slice(separator + 1).trim() };
}

function parseCount(raw, flag) {
  const value = Number(raw);
  if (!Number.isInteger(value) || value < 1) {
    throw new UsageError(`${flag} needs a positive integer, got: ${raw}`);
  }
  return value;
}

function parseArgs(argv) {
  const opts = {
    url: process.env.NEXTALK_EXPERIMENT_URL ?? '',
    payloadPath: process.env.NEXTALK_EXPERIMENT_PAYLOAD ?? '',
    headers: [],
    authEnv: '',
    authHeader: 'Authorization',
    authScheme: 'Bearer',
    method: 'POST',
    runs: 10,
    outPath: defaultOutPath(),
    timeout: 30_000,
    help: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) throw new UsageError(`missing value for ${flag}`);
      return argv[i];
    };

    if (HELP_FLAGS.has(flag)) {
      opts.help = true;
      continue;
    }

    switch (flag) {
      case '--url':
        opts.url = value();
        break;
      case '--payload':
        opts.payloadPath = value();
        break;
      case '--header':
        opts.headers.push(parseHeader(value()));
        break;
      case '--auth-env':
        opts.authEnv = value();
        break;
      case '--auth-header':
        opts.authHeader = value();
        break;
      case '--auth-scheme':
        opts.authScheme = value();
        break;
      case '--method':
        opts.method = value().toUpperCase();
        break;
      case '--runs':
        opts.runs = parseCount(value(), flag);
        break;
      case '--out':
        opts.outPath = resolve(value());
        break;
      case '--timeout':
        opts.timeout = parseCount(value(), flag);
        break;
      default:
        throw new UsageError(`unknown argument: ${flag}`);
    }
  }

  return opts;
}

/**
 * Turn the parsed flags into a runnable config, or throw UsageError. Every
 * check that can fail happens here, before the first byte goes out.
 */
function resolveConfig(opts) {
  if (!opts.url) {
    throw new UsageError('--url is required (or set NEXTALK_EXPERIMENT_URL)');
  }
  if (!opts.url.startsWith('https://')) {
    throw new UsageError(
      '--url must be https:// — this tool refuses cleartext endpoints so a credential can never cross the wire unencrypted',
    );
  }

  let endpoint;
  try {
    endpoint = new URL(opts.url);
  } catch {
    throw new UsageError(`--url is not a valid URL: ${opts.url}`);
  }

  let credential = null;
  if (opts.authEnv) {
    if (!ENV_NAME.test(opts.authEnv)) {
      throw new UsageError(
        `--auth-env takes the variable NAME, not the value. Got what looks like a secret: ${opts.authEnv.slice(0, 4)}… — export the key first, then pass its name`,
      );
    }
    credential = process.env[opts.authEnv] ?? '';
    if (credential === '') {
      throw new UsageError(
        `--auth-env ${opts.authEnv} was named but that environment variable is empty; export it in this shell before running the experiment`,
      );
    }
  }

  let body = '';
  if (opts.payloadPath) {
    const payloadPath = resolve(opts.payloadPath);
    try {
      body = readFileSync(payloadPath, 'utf8');
    } catch (error) {
      throw new UsageError(`--payload ${payloadPath} could not be read: ${error.message}`);
    }
    try {
      JSON.parse(body);
    } catch (error) {
      throw new UsageError(`--payload ${payloadPath} is not valid JSON: ${error.message}`);
    }
  }

  return {
    ...opts,
    url: endpoint.href,
    hostname: endpoint.hostname,
    port: endpoint.port === '' ? undefined : Number(endpoint.port),
    path: `${endpoint.pathname}${endpoint.search}`,
    bodyLength: Buffer.byteLength(body),
    body,
    credential,
    hasKey: credential !== null,
  };
}

function buildHeaders(config) {
  const headers = { 'content-type': 'application/json' };
  for (const header of config.headers) {
    headers[header.name.toLowerCase()] = header.value;
  }
  if (config.bodyLength > 0) {
    headers['content-length'] = String(config.bodyLength);
  }
  if (config.credential !== null) {
    headers[config.authHeader.toLowerCase()] = config.authScheme
      ? `${config.authScheme} ${config.credential}`
      : config.credential;
  }
  return headers;
}

const round1 = (value) => Math.round(value * 10) / 10;

function describeError(error) {
  return error instanceof Error ? error.message : String(error);
}

/** One request. Never rejects — a failure is a recorded sample, not an abort. */
function send(config, index) {
  return new Promise((resolve) => {
    const startedAt = process.hrtime.bigint();
    const elapsed = () => Number(process.hrtime.bigint() - startedAt) / 1e6;
    const failed = (error) => resolve({ index, error: describeError(error) });

    const req = httpsRequest(
      {
        method: config.method,
        hostname: config.hostname,
        port: config.port,
        path: config.path,
        headers: buildHeaders(config),
      },
      (res) => {
        const ttfb = elapsed();
        let bytes = 0;
        res.on('data', (chunk) => {
          bytes += chunk.length;
        });
        res.on('end', () =>
          resolve({
            index,
            ttfb_ms: round1(ttfb),
            total_ms: round1(elapsed()),
            status: res.statusCode ?? 0,
            bytes,
          }),
        );
        res.on('error', failed);
      },
    );

    req.setTimeout(config.timeout, () => {
      req.destroy(new Error(`timed out after ${config.timeout} ms`));
    });
    req.on('error', failed);
    if (config.bodyLength > 0) req.write(config.body);
    req.end();
  });
}

/** Nearest-rank percentile over an ascending array (the method the STT
 *  protocol cites, so both experiments report comparable numbers). */
function percentile(sorted, p) {
  if (sorted.length === 0) return null;
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(sorted.length - 1, Math.max(0, rank))];
}

function summarize(samples, key) {
  const values = samples
    .map((sample) => sample[key])
    .filter((value) => typeof value === 'number')
    .sort((a, b) => a - b);
  return {
    min: values.at(0) ?? null,
    p50: percentile(values, 50),
    p95: percentile(values, 95),
    max: values.at(-1) ?? null,
  };
}

function formatLine(sample) {
  if (sample.error) return `#${sample.index} error=${sample.error}`;
  return `#${sample.index} ttfb=${sample.ttfb_ms}ms total=${sample.total_ms}ms status=${sample.status} bytes=${sample.bytes}`;
}

/**
 * Origin + path with every query VALUE replaced by `<redacted>`. Several of
 * the vendors this tool targets accept the key in the query string, and both
 * sinks (stdout and OUTPUT.json) are routinely pasted into a transcript or
 * committed as the experiment result — the module's "the value never appears
 * in the output" promise has to hold for the URL too (WR-07).
 */
function redactUrl(href) {
  let url;
  try {
    url = new URL(href);
  } catch {
    return '<unparseable url>';
  }
  const search = [...url.searchParams]
    .map(([name]) => `${name}=<redacted>`)
    .join('&');
  return `${url.origin}${url.pathname}${search === '' ? '' : `?${search}`}`;
}

function formatStats(label, stats) {
  const value = (number) => (number === null ? 'n/a' : String(number));
  return `${label} min ${value(stats.min)}  p50 ${value(stats.p50)}  p95 ${value(stats.p95)}  max ${value(stats.max)}`;
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (error) {
    fail(`error: ${describeError(error)}\n`);
    fail(USAGE);
    process.exitCode = 1;
    return;
  }

  if (opts.help) {
    out(USAGE);
    return;
  }

  let config;
  try {
    config = resolveConfig(opts);
  } catch (error) {
    fail(`error: ${describeError(error)}\n`);
    fail(USAGE);
    process.exitCode = 1;
    return;
  }

  out(`endpoint: ${config.method} ${redactUrl(config.url)}`);
  out(`runs:     ${config.runs}`);
  out(`auth:     ${config.hasKey ? `${config.authEnv} (set)` : 'none'}`);
  out(`body:     ${config.bodyLength} bytes`);
  out('');

  const samples = [];
  for (let index = 1; index <= config.runs; index += 1) {
    const sample = await send(config, index);
    samples.push(sample);
    out(formatLine(sample));
  }

  const ttfb = summarize(samples, 'ttfb_ms');
  const total = summarize(samples, 'total_ms');
  const failures = samples.filter((sample) => sample.error).length;

  out('');
  out(formatStats('ttfb_ms ', ttfb));
  out(formatStats('total_ms', total));
  out('');

  const report = {
    config: {
      url: redactUrl(config.url),
      method: config.method,
      runs: config.runs,
      authEnv: config.hasKey ? config.authEnv : null,
      hasKey: config.hasKey,
      headers: config.headers.map((header) => header.name),
      measuredAt: new Date().toISOString(),
    },
    samples,
    stats: { ttfb_ms: ttfb, total_ms: total },
  };

  mkdirSync(dirname(config.outPath), { recursive: true });
  writeFileSync(config.outPath, `${JSON.stringify(report, null, 2)}\n`);
  out(`report:   ${config.outPath}`);

  if (failures > 0) {
    fail(
      `error: ${failures}/${config.runs} request(s) failed — the numbers above are not a valid measurement`,
    );
    process.exitCode = 1;
  }
}

await main();
