---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB), P20 as amended on branch fix/impl-header-candidate-filter
scope: round 4. Commits 313fc16 and fc9afb8. A type alias defined in the workspace drops the impl header. Two packages with one name and version but different crate names give an unknown crate. I judged only the touched properties again.
status: 0 critical, 0 warnings, 0 info
---

`cargo test`: 272 passed (269 unit, 3 integration), 1 ignored.

P20 is amended on this branch. I judged the tests against the branch text. The code and the KB edit must land in the same pull request (#40).

Mutations, each on a scratch copy in /tmp:

- Alias reference skipped (`continue` instead of `None`): `test_impl_header_symbols_alias_drops_header` fails (round 3, code unchanged since).
- `by_version` back to `insert` (last entry wins): `test_crate_names_from_metadata` fails at the `fork-pkg` assertion (round 3, code unchanged since).
- `by_version` set to `None` on every repeated key, including keys whose crate names agree: `test_crate_names_from_metadata` now fails at the new `same-pkg` assertion (src/lib.rs:2730). In round 3 this mutation passed.

## Critical

None.

## Warnings

None.

## Info

None.

## Round-3 finding verified

- **W1 (agreeing sources)**: fixed. fc9afb8 adds two `same-pkg` 1.0.0 entries with the lib `same_pkg` and asserts `Some("same_pkg")`. The third mutation above now fails.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not judged again | Untouched by these commits |
| P12 | n/a | n/a | Retired |
| P13-P19 | existing suites | Not judged again | Untouched by these commits |
| P20 | test_impl_header_symbols_alias_drops_header, test_crate_names_from_metadata, test_impl_header_symbols_paths_in, existing P20 tests | Full for the new rules | Alias: a positive and a negative case. Version merge: a disagreeing case (`fork-pkg`) and an agreeing case (`same-pkg`) |
| C1-C3 | existing | Not judged again | Untouched by these commits |

## Verified clean

- Alias rule: a header with an alias reference gives no paths. The other references keep their paths in source order.
- Version merge: disagreement gives `None` through `by_version` and through `by_name`. Agreement keeps the crate name.
- `crate_names` itself (the `cargo metadata` runner with the `--offline` and `--no-deps` fallback) still has no test. This was true before these commits.
