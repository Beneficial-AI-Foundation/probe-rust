---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB)
scope: branch fix/impl-header-candidate-filter (a105d5a, dc33019, 716a431, 53e9d6e) vs main, fourth round
status: 0 critical, 0 warnings, 1 info
---

Validation: `cargo fmt -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 264 passed (1 ignored).

Third-round W1 is fixed: `lib_crate_names` reads `cargo metadata --offline` (full graph, so renamed dependency libs are mapped) and falls back to `--no-deps`, then to an empty map.

## Info

### [I1] Duplicated `cargo metadata` lib-target parsing (carried, left on purpose)
- **Location**: src/lib.rs:683-729, src/public_api.rs:430-447
- **Issue**: The invocation and the lib-target predicate are copied from `public_api.rs`.
- **Recommendation**: Share one helper when either side changes next.

## Verified clean

- P20: the renamed map is applied only to the crate segment; the fallback chain matches the P20 text; failure is non-fatal and yields `-` read as `_`.
- `--offline` adds no network access; a failed resolve falls through to `--no-deps`.
- P3: header fields remain `#[serde(skip)]`; wire format unchanged.
- P1-P19, C1-C3: not touched.
