# Deferred Items — Phase 02

Out-of-scope findings surfaced during execution (SCOPE BOUNDARY rule: recorded, not fixed).

## From 02-01

| Item | File | Detail | Suggested owner |
|------|------|--------|-----------------|
| Pre-existing rustfmt drift | `apps/desktop/src-tauri/src/sim/source_test.rs` | The crate is not rustfmt-clean at this file (`cargo fmt` was run over the crate during 02-01 and reflowed three expressions there); the reformat was reverted to keep the task commit scoped to the budget gate. A repo-wide `cargo fmt` will pick it up. | Any wave that already touches `sim/`; or a dedicated `style:` chore |
| Pre-existing compiler warning | `apps/desktop/src-tauri/src/sim/source_test.rs:248` | `unused variable: zh` in `round_one_content_is_byte_exact` — the binding is destructured but only `en` is asserted. Predates 02-01. | Same as above |
