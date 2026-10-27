---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB), P20 as amended in the working tree
scope: branch fix/impl-header-candidate-filter, uncommitted working-tree change (HeaderEvidence Exact/Compatible/Contradicted, ordered exactness, Manifest-only lone-candidate span skip). Only properties the change touches were re-judged.
status: 0 critical, 0 warnings, 0 info
---

`cargo test`: 268 passed (265 unit, 3 integration), 1 ignored.

The P20 text is amended in the same uncommitted change (kb/engineering/properties.md, glossary.md). The tests were judged against the amended text, so the code and the KB edit must land in the same pull request.

If a new rule is reverted, a test fails:
- Ordered exactness: `test_resolve_single_candidate_swapped_arguments_need_span` (sorting the paths again makes `Tr<B, A>` exact and the lone Manifest trait impl a Match).
- Manifest-only span skip: `test_resolve_single_llbc_candidate_exact_needs_span` (dropping the source check makes the LLBC candidate a Match).
- Header stage drops only Contradicted: `test_resolve_llbc_subset_candidate_kept_beside_superset_sibling` (the old filter kept only def 2, which names every header path. Its span misses, so the result was NoMatch).
- Exact does not displace Compatible: `test_resolve_exact_candidate_does_not_drop_compatible_sibling` (a "prefer Exact" filter keeps only def 1, the lone exact Manifest trait impl matches without span, and the test expects def 2).
- Source order of header paths: `test_impl_header_symbols_paths_in` lists the self type `Error` before `From`, so removing `sort_by_key(start)` in `paths_in` returns `Error, From, serialize::Error` and the test fails. `test_convert_with_impl_header_symbols` pins the same order end to end.

## Previous findings

- [I1] `sort_by_key(start)` in `paths_in` not pinned: closed. The fixture now lists the `Error` occurrence out of source order (src/lib.rs, `test_impl_header_symbols_paths_in`).
- [I2] No test that an Exact candidate keeps a Compatible sibling: closed. `test_resolve_exact_candidate_does_not_drop_compatible_sibling` asserts both evidence levels, then that the span picks the compatible candidate on the Manifest source.
- Stale test name: closed. `test_resolve_impl_header_unknown_falls_back_to_span` is now `test_resolve_untyped_impl_segment_falls_back_to_span`, and `test_resolve_header_subset_is_unknown` is now `test_resolve_header_subset_is_compatible`. No test name or doc comment uses the two retired evidence names.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not re-audited | Untouched. Header fields are `#[serde(skip)]` |
| P12 | n/a | n/a | Retired |
| P13-P19 | existing suites | Not re-audited | Untouched |
| P20 | test_resolve_single_candidate_swapped_arguments_need_span, test_resolve_single_llbc_candidate_exact_needs_span, test_resolve_llbc_subset_candidate_kept_beside_superset_sibling, test_resolve_exact_candidate_does_not_drop_compatible_sibling, test_resolve_multi_candidate_by_impl_header_with_shifted_spans, test_resolve_single_candidate_impl_header, test_resolve_untyped_impl_segment_falls_back_to_span, test_resolve_single_candidate_primitive_sibling_needs_span, test_resolve_header_subset_is_compatible, test_header_evidence_exact_counts_repeats, test_impl_segment, test_impl_header_symbols_paths_in, test_convert_with_impl_header_symbols, existing P20 tests | Full | |
| C1-C3 | existing | Not re-audited | Untouched |

## Verified clean

- P20 evidence levels: Exact (ordered, with multiplicity, complete header, path-only segment), Compatible (superset, subset, no header, untyped segment), Contradicted.
- Header stage: contradicted candidates dropped, all contradicted ends NoMatch (`serialize` atom in the shifted-spans test), compatible and exact candidates both kept for the span stage.
- Lone candidate: contradicted is NoMatch. An exact Manifest trait impl is a Match without span. Exact LLBC, exact inherent, swapped-order, primitive-sibling and incomplete-header candidates all need the span.
- Lib-name rendering through `build_impl_header_symbols` (third-round I1 stays fixed).
- The rename `FunctionSpan::impl_header_paths` / `SpanInfo::impl_header_paths` to `impl_header_path_count` is mechanical. The completeness check in `convert_to_atoms_with_lines_internal` is still covered by `test_convert_with_impl_header_symbols` (`impl_header_complete`) and the primitive-sibling test.
- When Charon enrichment runs, the `extract.rs` gate builds header symbols. Otherwise it skips them. It is one-line wiring with no test, accepted as before. The `--offline` / `--no-deps` fallback in `lib_crate_names` is still process-only and untested, accepted in an earlier round.
