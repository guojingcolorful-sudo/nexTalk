# Vendor experiments (D-04)

The Phase 2 planner picks the STT / translation / TTS / search vendors on the
strength of measurements, not on vendor marketing. This directory holds the
framework for those measurements: the protocols, the schema, and the tooling.
Nothing here runs during the application's build or at runtime — the whole
directory is offline by design.

## API-key policy

**No API keys in Phase 1: all experiments run between Phase 1 and Phase 2
planning; keys live in a local `.env` (gitignored), never committed, never in
source.**

Concretely:

- the scripts here read credentials **only at run time** from the process
  environment (`--auth-env NAME` names which variable to read); no key is
  accepted as a CLI argument, so none can leak into shell history or a
  committed command line;
- no `.env` file is committed, and nothing in `tools/` loads one implicitly —
  export the variables in the shell that runs the experiment;
- threat T-01-15 covers this boundary: the schemas below carry no secret
  fields, and the Phase 1 verification path (`--help`) performs no network
  call at all.

If a key is ever pasted into a committed file, treat it as leaked, rotate it
in the vendor console first, then clean the history.

## Experiments

| Experiment                            | Protocol                                                             | Tooling                            | Feeds                                                     |
| ------------------------------------- | -------------------------------------------------------------------- | ---------------------------------- | --------------------------------------------------------- |
| STT A/B (Mandarin accuracy + latency) | [stt-ab-protocol.md](stt-ab-protocol.md)                             | manual, same-Mac capture           | STT vendor for the user's Chinese path                    |
| Blind voice-clone listening test      | [blind-clone-test-set.schema.json](blind-clone-test-set.schema.json) | manual grading                     | clone TTS vendor (MiniMax / Cartesia / Fish / ElevenLabs) |
| Provider RTT (TTFB + total)           | this file                                                            | [rtt/measure.mjs](rtt/measure.mjs) | latency budget per hop                                    |

Vendor shortlist and the reasoning behind each candidate:
[`../../.planning/research/STACK.md`](../../.planning/research/STACK.md).
The pipeline stages these experiments decide:
`STT (user zh) → translate → clone TTS`, plus STT for the interviewer's English
and the copilot/search vendors.

## Running the RTT measurement

```bash
# Always start here: prints usage and exits 0 without touching the network.
node tools/vendor-experiments/rtt/measure.mjs --help

# Phase 2 only — a real endpoint, a real key in the environment:
export NEXTALK_EXPERIMENT_KEY="<vendor key>"
node tools/vendor-experiments/rtt/measure.mjs \
  --url https://api.example.com/v1/chat/completions \
  --payload ./payload.json \
  --header "Content-Type: application/json" \
  --auth-env NEXTALK_EXPERIMENT_KEY \
  --runs 10 \
  --out tools/vendor-experiments/rtt/OUTPUT.json
```

Endpoint and payload can also come from the environment
(`NEXTALK_EXPERIMENT_URL`, `NEXTALK_EXPERIMENT_PAYLOAD`), so a run is a
copy-paste away from a vendor's own quickstart.

Vendors disagree on where the credential goes: MiniMax and Cartesia want
`Authorization: Bearer <key>`, Deepgram wants `Authorization: Token <key>`,
Gemini wants `x-goog-api-key: <key>`. `--auth-header` and `--auth-scheme`
(the latter empty for a raw value) cover all three, so the tool stays
vendor-agnostic without ever taking a key value from the command line.

Expected output: one line per request (`#1 ttfb=…ms total=…ms status=200`),
then the p50 / p95 / min / max summary, and an `OUTPUT.json` file holding

```json
{
  "config": {
    "url": "…",
    "runs": 10,
    "method": "POST",
    "authEnv": "…",
    "hasKey": true,
    "headers": ["Content-Type"],
    "measuredAt": "2026-09-11T…Z"
  },
  "samples": [{ "index": 1, "ttfb_ms": 210, "total_ms": 640, "status": 200, "bytes": 812 }],
  "stats": {
    "ttfb_ms": { "min": 190, "p50": 210, "p95": 340, "max": 351 },
    "total_ms": { "min": 610, "p50": 640, "p95": 820, "max": 844 }
  }
}
```

`hasKey` records only whether a credential was present — never its value.
Commit the `OUTPUT.json` you want the Phase 2 planner to read (it is
measurement data, not a secret); never commit the key or the shell command
that carried it.

## Ground rules

- **Sequential requests only.** Concurrency measures the vendor's queueing
  behaviour, not the per-request latency the product budget needs (≤2 s end to
  end, of which a single hop gets a few hundred milliseconds).
- **Same machine, same network, same time of day.** Latency numbers from a
  different network are not comparable.
- **Warm up first.** Run the measurement once, discard it, then run the
  recorded pass (see the STT protocol's warmup rule).
- **Record the environment** (macOS version, machine, connection) next to the
  numbers. A latency table without its environment is not evidence.
