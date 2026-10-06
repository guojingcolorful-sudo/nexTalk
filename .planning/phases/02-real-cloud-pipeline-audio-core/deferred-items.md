# Deferred Items — Phase 02

Out-of-scope findings surfaced during execution (SCOPE BOUNDARY rule: recorded, not fixed).

## From 02-01

| Item | File | Detail | Suggested owner |
|------|------|--------|-----------------|
| Pre-existing rustfmt drift | `apps/desktop/src-tauri/src/sim/source_test.rs` | The crate is not rustfmt-clean at this file (`cargo fmt` was run over the crate during 02-01 and reflowed three expressions there); the reformat was reverted to keep the task commit scoped to the budget gate. A repo-wide `cargo fmt` will pick it up. | Any wave that already touches `sim/`; or a dedicated `style:` chore |
| Pre-existing compiler warning | `apps/desktop/src-tauri/src/sim/source_test.rs:248` | `unused variable: zh` in `round_one_content_is_byte_exact` — the binding is destructured but only `en` is asserted. Predates 02-01. | Same as above |

## From 02-03

| Item | File | Detail | Suggested owner |
|------|------|--------|-----------------|
| vitest has no `globals: true` | `apps/desktop/vitest.config.ts` | Testing Library auto-cleanup is inactive, so each new React test file must add its own `afterEach(() => cleanup())` or a prior render leaks into the next query (it bit the DiagnosticsPage 超预算 badge test during T3.7). Already logged as an 01-03 decision; it recurs as a per-file tax on every new suite. | A `chore(test):` enabling `globals: true` (or a shared setup file), then dropping the manual cleanups |
| vitest CLI filter is not per-file | `apps/desktop` test script | The plan's verify form `pnpm --filter @nextalk/desktop test -- <File>` runs the whole desktop suite (10 files), not the named file — RED evidence stays valid (`Failed to resolve import`), but the invocation must not be read as a per-file filter. For a real per-file run, call `vitest run <path>` directly. | Wave/plan authors writing verify blocks |

## From 02-05

All five predate this plan; none is caused by the audio work, and the full suite
is green with them. Recorded per the SCOPE BOUNDARY rule (do not fix, do not
re-run builds hoping they resolve).

| Item | File | Detail | Suggested owner |
|------|------|--------|-----------------|
| Pre-existing compiler warning | `apps/desktop/src-tauri/src/pipeline/validate.rs:461` | `unused_mut`: `let mut value = digits` — the binding is never mutated. | A `fix:`/`style:` chore; unrelated to audio |
| Pre-existing compiler warning | `apps/desktop/src-tauri/src/sim/source_test.rs:256` | `unused variable: zh` in `round_one_content_is_byte_exact` (still open from 02-01 — the line moved from 248 to 256). | Same as above |
| Pre-existing clippy lint | `apps/desktop/src-tauri/src/audio/capture.rs:292` | `clone_on_copy` — a `Copy` value is `.clone()`d. (Line number is from before T5.5 appended the per-role assembly; the lint itself is unchanged.) | A `refactor(audio):` pass once 02-05 closes |
| Pre-existing clippy lint | `apps/desktop/src-tauri/src/enroll/capture.rs:343` | `clone_on_copy`, same shape as above. | Same as above |
| Pre-existing clippy lint | `apps/desktop/src-tauri/src/pipeline/cascade.rs:616` | `drain_collect` — `drain(..).collect()` where `mem::take` reads better. | Same as above |
| Harmless linker warning | `webrtc-audio-processing-sys` (build script) | Every link prints three `ld: directory not found for option '-L…/lib/x86_64-linux-gnu'` (and aarch64/lib64) warnings: the sys crate emits Linux library paths that do not exist on macOS. Non-fatal, present since T5.1's first bundled build. | Upstream crate; suppress with a `build.rs` filter only if it starts hiding real output |
