---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB)
scope: branch fix/impl-header-candidate-filter (a105d5a, dc33019, 716a431, 53e9d6e) vs main, fourth round. Only properties the branch touches were re-judged.
status: 0 critical, 0 warnings, 1 info
---

`cargo test`: 264 passed, 1 ignored.

Third-round I1 is fixed: `test_convert_with_impl_header_symbols` passes `{"x": "xlib"}` and asserts `xlib::Error` and `xlib::serialize::Error`, so lib-name rendering through header building is pinned. The call site at extract.rs:54 is still not exercised by a test; that is the one-line wiring and is accepted as is.

## Info

### [I1] The `--offline` / `--no-deps` fallback order is untested
- **Location**: src/lib.rs:683-699 (`lib_crate_names`)
- **Issue**: Only the JSON parser is tested. Swapping or dropping the fallback would keep every test green. It needs a `cargo` process, so a unit test is costly.
- **Recommendation**: Optional: an ignored integration test on a fixture workspace with a renamed dependency lib, or leave as is.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not re-audited | Untouched. Header fields are `#[serde(skip)]` |
| P12 | n/a | n/a | Retired |
| P13-P19 | existing suites | Not re-audited | Untouched |
| P20 | test_resolve_multi_candidate_by_impl_header_with_shifted_spans, test_resolve_single_candidate_impl_header, test_resolve_impl_header_unknown_falls_back_to_span, test_resolve_single_candidate_primitive_sibling_needs_span, test_resolve_header_subset_is_unknown, test_header_evidence_exact_counts_repeats, test_impl_segment, test_scip_type_symbol_to_path, test_lib_crate_names_from_metadata, test_impl_header_symbols_paths_in, test_occurrence_bounds, test_convert_with_impl_header_symbols, test_impl_header_ranges, existing P20 tests | Full | I1 is the process fallback only |
| C1-C3 | existing | Not re-audited | Untouched |

## Verified clean

- Evidence levels, lone-candidate rules, all-contradicted NoMatch, shifted spans, primitive siblings, subset as no evidence, multiplicity.
- Lib-name rendering through `build_impl_header_symbols`; metadata parsing positive and negative cases.
