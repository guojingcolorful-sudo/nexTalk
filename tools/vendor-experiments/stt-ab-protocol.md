# STT A/B protocol

**Question this answers:** which speech-to-text provider is accurate and fast
enough for the user's Mandarin speech (the path that feeds the cloned voice)
and for the interviewer's English (the path that feeds subtitles and the
copilot)?

**Run window:** between Phase 1 and Phase 2 planning. Nothing here executes in
Phase 1 (see the API-key policy in [README.md](README.md)).

## Candidates

| Path           | Candidate                                                | Why it is in the test                                                 |
| -------------- | -------------------------------------------------------- | --------------------------------------------------------------------- |
| user zh        | Gemini Live API (`gemini-3.1-flash-live`, text modality) | Chirp-3-class Mandarin; fuses STT + translation into one hop          |
| user zh        | Deepgram `nova-3` (streaming)                            | The latency leader; the fallback if Live API interim text disappoints |
| interviewer en | Deepgram `nova-3` (streaming)                            | English is its strongest language; sub-300 ms partials                |
| interviewer en | OpenAI `gpt-4o-transcribe`                               | Single-vendor alternative if the OpenAI ecosystem is chosen           |

Full reasoning and 2026 pricing: [`../../.planning/research/STACK.md`](../../.planning/research/STACK.md).

## Utterance set (fixed, recorded once)

Record each utterance **once** as 48 kHz mono WAV with the real input chain
(the same microphone or BlackHole source the product uses), then feed the
identical files to every vendor. Re-recording per vendor invalidates the
comparison.

| id            | clip | register       | language      | tests                                  |
| ------------- | ---- | -------------- | ------------- | -------------------------------------- |
| `zh-tech-5s`  | 5 s  | technical      | 中            | domain nouns, product names            |
| `zh-tech-15s` | 15 s | technical      | 中            | a full STAR-shaped answer with numbers |
| `zh-chat-15s` | 15 s | conversational | 中            | fillers, restarts, natural disfluency  |
| `zh-code-60s` | 60 s | code-switched  | 中 + EN terms | K8s / Redis / P99 embedded in Chinese  |
| `en-tech-15s` | 15 s | technical      | EN            | the interviewer's question register    |
| `en-chat-60s` | 60 s | conversational | EN            | long-form interviewer follow-ups       |

Content: draw the Chinese clips from the project's own demo script
(`apps/desktop/src-tauri/src/sim/script.rs`) so the numbers describe the
traffic the product actually handles, and the English clips from a real
interview-question bank. Keep the transcripts with the recordings — they are
the ground truth the scoring uses.

## Procedure

1. **Warmup rule.** Run one full pass and discard it. Streaming endpoints
   negotiate a session and DNS/TLS handshakes land in the first request; the
   recorded pass must not contain them.
2. **3 runs per utterance, per vendor.** Feed the same file 3 times. Record
   every run; the reported figure is the median of the 3 (a single run is a
   sample, not a measurement).
3. **Identical parameters.** One request per utterance, same sample rate, same
   encoding, same language hint. Streaming vendors: keep the audio pacing
   identical (replay at real time) — blasting a file faster than real time
   measures the wrong thing.
4. **Capture the raw response**, not just the final transcript: interim
   results and their timestamps are what determine whether the cascade can
   start early.
5. **Measure first-partial latency, not just final latency.** The product needs
   a partial at ~300-600 ms to stay inside the 2 s budget.

## Scoring rubric

| Metric               | How it is measured                                                                                                                   | Pass bar                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- |
| Accuracy             | CER for Chinese (character error rate), WER for English, vs the recorded transcript; normalize punctuation and spaces before scoring | zh CER ≤ 8%, en WER ≤ 8%         |
| First partial        | ms from first audio frame sent to the first non-empty partial                                                                        | ≤ 600 ms                         |
| Final latency        | ms from last audio frame sent to the final transcript                                                                                | ≤ 1.2 s                          |
| Stability            | did the transcript rewrite itself mid-utterance (words appearing, then changing)?                                                    | no rewrite of already-final text |
| Code-switch handling | did the embedded English terms come back as Latin script, correctly spelled?                                                         | K8s / Redis / P99 exact          |
| Cost                 | vendor list price at the measured audio duration                                                                                     | ≤ the budget in STACK.md         |

Latency percentiles come from the per-run samples: report **p50 and p95** (and
min/max when the sample count allows). Use the same method as
`rtt/measure.mjs` so the numbers are comparable across experiments.

## Result table template

One row per (vendor, utterance) run; paste into the Phase 2 planning input.

```markdown
**Environment:** macOS <version> · <machine> · <microphone/BlackHole source> · <connection> · measured <date>

| vendor | model                 | utterance   | run | CER/WER | first partial (ms) | final (ms) | stable | cost |
| ------ | --------------------- | ----------- | --- | ------- | ------------------ | ---------- | ------ | ---- |
| gemini | gemini-3.1-flash-live | zh-tech-15s | 1   |         |                    |            |        |      |
```

Summary table (median of 3) that the vendor decision actually reads:

```markdown
| vendor | model | zh CER (median) | en WER (median) | first partial p50 | first partial p95 | ≤2s budget headroom | cost per interview hour |
| ------ | ----- | --------------- | --------------- | ----------------- | ----------------- | ------------------- | ----------------------- |
```

## Environment notes

- **One machine, one network, one sitting.** Run every vendor on the same Mac
  on the same connection; a vendor measured from a different network is not
  comparable. Record the environment next to the numbers.
- **Region matters.** MiniMax keys are not interchangeable between
  `api.minimax.io` and `api.minimax.chat`; the same split exists for several
  vendors' endpoints. Record which endpoint each row used.
- **No key in the recording.** The session log that accompanies a recording
  must not contain the credential.
