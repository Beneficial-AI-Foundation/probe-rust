---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB, working tree on fix/impl-header-candidate-filter)
scope: branch fix/impl-header-candidate-filter (a105d5a..c6a6120) vs main, plus the uncommitted working-tree change (HeaderEvidence Exact/Compatible/Contradicted, ordered exactness, Manifest-only span skip), sixth round
status: 0 critical, 0 warnings, 0 info
---

Validation: `cargo fmt -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 268 passed (1 ignored).

KB revision: the P20 impl-header text is an amendment on this branch. On `main`, P20 still says that trait type arguments are not a matching signal. The code and the amended P20 are in the same pull request. Thus this is not a cross-PR merge-order constraint. But the P20 results below hold only against the branch text.

## Findings

None.

## Verified clean

- Previous [I1] fixed: the P20 "Where" list (kb/engineering/properties.md:212) now names `FunctionSpan::impl_header_path_count` in the `rust_parser.rs` entry. No document still uses the old field name `FunctionSpan::impl_header_paths`.
- Previous [I2] fixed: the `test_resolve_single_candidate_primitive_sibling_needs_span` doc comment (src/charon_names.rs:3349-3352) is now wrapped as one paragraph. No other added doc line in the diff is wrapped badly or is over the line limit.
- `ImplHeaderSymbols::paths_in` sorts by `HeaderSymbolRef::start`, which is `(line, column)`. Thus the source order that the doc comments and P20 claim holds for multi-line headers. A test lists the occurrences out of order to pin this.


- P20, multi-candidate pipeline: `resolve_charon_candidate` (src/charon_names.rs:1030-1041) runs the filters in this order: same file, self type, dedup, header, span. The header stage drops only `Contradicted` candidates. If no candidate is left, the result is NoMatch. P20, the glossary (Candidate resolution, Impl-header evidence, Span disambiguation), the docs/SCHEMA.md `charon-def-id` row and kb/engineering/architecture.md:100 all say the same. No document still describes the removed "keep confirming candidates" rule.
- P20, `HeaderEvidence`: if neither path set contains the other, the code returns `Contradicted`. If the header is complete, the segment is path-only and the two path sequences are equal in order, it returns `Exact`. It returns `Compatible` in all other cases, including no header paths and no typed segment. The enum doc, the `header_evidence` doc and the P20 text agree. Tests pin the `Tr<B, A>` vs `Tr<A, B>` case (Compatible, then NoMatch on a disjoint span) and the multiplicity case.
- P20, ordering: `ImplHeaderSymbols::paths_in` sorts by `(line, column)` start. Thus multi-line headers keep source order. `impl_segment` no longer sorts. Both doc comments state the order. A sample of the SPQR verification repo's translation.json shows that manifest segments render `Trait<args> for Self` and keep ADT arguments (`Deref<sorted_vec::SortedVec<T>> for sorted_vec::SortedSet<T>`). This agrees with the P20 claim that only the LLBC rendering drops them.
- P20, lone candidate: `validate_single_candidate` rejects `Contradicted`. It skips the span comparison only for `Exact` with `EnrichmentSource::Manifest` and a trait-impl segment. In all other cases it applies the span comparison. P20, the glossary, SCHEMA.md, the CHANGELOG 0.12.1 entry and the function doc comment agree. Tests cover the LLBC exact case (`test_resolve_single_llbc_candidate_exact_needs_span`) and the LLBC subset-beside-superset case (`test_resolve_llbc_subset_candidate_kept_beside_superset_sibling`).
- P20, outcomes, self-type no-signal fallback, inclusive span overlap, Manifest fail-closed file proof: this diff does not change them.
- extract.rs: if `--translation` or `--with-charon` is given, extract builds the header symbols. These are the only paths that call `enrich_atoms`. The CHANGELOG states this. With `None`, header paths are empty, and empty paths give `Compatible`.
- P3: `impl_header_paths` and `impl_header_complete` stay `#[serde(skip)]`. The wire format does not change.
- The auditor rule P20 line (.cursor/rules/auditors/code-quality-auditor.md) agrees with the new behavior.
- P1-P19, C1-C3: this change does not touch them.
