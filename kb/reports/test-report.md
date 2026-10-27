---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB), P20 as amended in the working tree
scope: branch fix/impl-header-candidate-filter, uncommitted working-tree change on top of 343a140 (source-aware `header_evidence`: only Manifest segments are evidence, a Manifest segment lacking a header path is Contradicted, LLBC candidates are always Compatible). Only properties the change touches were re-judged.
status: 0 critical, 0 warnings, 0 info
---

`cargo test`: 269 passed (266 unit, 3 integration), 1 ignored.

The P20 text is amended in the same uncommitted change (kb/engineering/properties.md, glossary.md). The tests were judged against the amended text, so the code and the KB edit must land in the same pull request.

Each rule was checked by mutation on a scratch copy of `src/charon_names.rs`. If a rule is reverted, a test fails:
- Only Manifest segments are evidence (drop the `source != Manifest` check): `test_resolve_llbc_candidate_with_associated_type_not_contradicted` (the LLBC `Mul<c::Wrapper, c::Bar>` segment becomes contradicted and the lone candidate NoMatch), `test_resolve_llbc_subset_candidate_kept_beside_superset_sibling` (the atom's own subset impl is dropped) and `test_resolve_single_llbc_candidate_exact_needs_span` (evidence becomes Exact, the test asserts Compatible).
- A Manifest segment lacking a header path is Contradicted (restore the old "each side lacks a path" rule): `test_resolve_manifest_missing_header_path_contradicts` (`From<c::Foo>` is a subset of the `From<Wrapper<Foo>>` header, so the old rule calls it Compatible).
- Extra Manifest segment paths stay Compatible (require set equality instead): `test_resolve_exact_candidate_does_not_drop_compatible_sibling` (the `Box<c::Foo, alloc::alloc::Global>` superset candidate becomes contradicted).
- Exact Manifest trait impl matches without span (remove the Exact arm): `test_resolve_single_candidate_impl_header`, `test_resolve_multi_candidate_by_impl_header_with_shifted_spans`, `test_resolve_manifest_missing_header_path_contradicts`.
- Header stage drops Contradicted candidates (remove the `retain`): `test_resolve_multi_candidate_by_impl_header_with_shifted_spans`, `test_resolve_manifest_missing_header_path_contradicts`.

Rules from the previous round (ordered exactness, Manifest-only lone-candidate span skip, Exact does not displace Compatible, source order of header paths) keep their tests: `test_resolve_single_candidate_swapped_arguments_need_span`, `test_resolve_single_llbc_candidate_exact_needs_span`, `test_resolve_exact_candidate_does_not_drop_compatible_sibling`, `test_impl_header_symbols_paths_in`.

## Previous findings

- None open. The previous round closed clean.
- `test_resolve_header_subset_is_compatible` is removed. Its rule (a Manifest or LLBC subset segment is Compatible) is retired by the amended P20. The LLBC half is now covered by `test_resolve_llbc_subset_candidate_kept_beside_superset_sibling` and `test_resolve_llbc_candidate_with_associated_type_not_contradicted`; the Manifest half is reversed and covered by `test_resolve_manifest_missing_header_path_contradicts`. No test name, doc comment, KB page, SCHEMA.md or CHANGELOG entry still refers to the removed test or to the bidirectional contradiction rule.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not re-audited | Untouched |
| P12 | n/a | n/a | Retired |
| P13-P19 | existing suites | Not re-audited | Untouched |
| P20 | test_resolve_manifest_missing_header_path_contradicts, test_resolve_llbc_candidate_with_associated_type_not_contradicted, test_resolve_llbc_subset_candidate_kept_beside_superset_sibling, test_resolve_single_llbc_candidate_exact_needs_span, test_resolve_exact_candidate_does_not_drop_compatible_sibling, test_resolve_single_candidate_swapped_arguments_need_span, test_resolve_multi_candidate_by_impl_header_with_shifted_spans, test_resolve_single_candidate_impl_header, test_resolve_untyped_impl_segment_falls_back_to_span, test_resolve_single_candidate_primitive_sibling_needs_span, test_header_evidence_exact_counts_repeats, test_impl_segment, test_impl_header_symbols_paths_in, test_convert_with_impl_header_symbols, existing P20 tests | Full | Each new rule fails a test when reverted |
| C1-C3 | existing | Not re-audited | Untouched |

## Verified clean

- P20 evidence levels per source. Manifest: Exact (ordered, with multiplicity, complete header), Compatible (superset, no header, untyped segment), Contradicted (segment lacks a header path). LLBC: always Compatible, including a segment that both drops and adds paths.
- Header stage: contradicted candidates dropped, all contradicted ends NoMatch (`serialize` atom in the shifted-spans test), compatible and exact candidates both kept for the span stage.
- Lone candidate: contradicted is NoMatch; an exact Manifest trait impl is a Match without span; exact-looking LLBC, inherent, swapped-order, primitive-sibling and incomplete-header candidates need the span.
- Moving the Manifest check from `validate_single_candidate` into `header_evidence` keeps the lone-candidate behavior: the LLBC exact case is still pinned by `test_resolve_single_llbc_candidate_exact_needs_span`, now at both the evidence and the outcome level.
- The `extract.rs` gate and the `cargo metadata --offline` / `--no-deps` fallback in `lib_crate_names` are unchanged and remain accepted without tests, as in earlier rounds.
