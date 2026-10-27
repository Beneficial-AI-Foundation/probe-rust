---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB, working tree on fix/impl-header-candidate-filter)
scope: branch fix/impl-header-candidate-filter vs main, plus the uncommitted working-tree change on top of 343a140 (source-aware `header_evidence`: only Manifest segments are evidence, a Manifest segment that lacks a header path is Contradicted, LLBC candidates are always Compatible), seventh round
status: 0 critical, 0 open (2 warnings fixed in this round)
---

Validation: `cargo fmt -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 269 passed (1 ignored).

KB revision: the P20 impl-header text is an amendment on this branch. On `main`, P20 still says that trait type arguments are not a matching signal. The code and the amended P20 are in the same pull request. Thus this is not a cross-PR merge-order constraint. But the P20 results below hold only against the branch text.

## Findings

### [W1] architecture.md still says the impl-header filter applies to LLBC collisions
- **Resolution**: fixed. The passage now says the impl-header stage uses Manifest segments only.

kb/engineering/architecture.md:100 is the "Supports multi-crate LLBCs" paragraph of the Charon component. It says that colliding match keys are resolved by "file, self-type, dedup, impl-header, and span filters", and that the impl-header filter "compares the paths SCIP resolves in the impl header with the candidate's impl segment, so it holds when Charon ran on patched source". After this change, `header_evidence` (src/charon_names.rs:810-817) returns `Compatible` for every `EnrichmentSource::Llbc` candidate, so on the LLBC path the header stage compares nothing and the span alone decides. The paragraph must say that the impl-header filter uses only `translation.json` (Manifest) segments. Severity: Warning (documentation drift).

### [W2] glossary "Span disambiguation" says path type arguments are always compared before the span stage
- **Resolution**: fixed. The passage now says the impl-header stage uses Manifest segments only.

kb/engineering/glossary.md:103 says: "Trait type arguments that are paths are compared by the impl-header stage before this one, which drops contradicted candidates; same-self-type impls that differ only in primitive arguments or type parameters ... are split by span alone." This is true only on the Manifest source. On the LLBC source, impls that differ in a path argument (`From<Foo>` and `From<Bar> for X`) are also split by span alone, because LLBC candidates are always compatible. P20 and the "Impl-header evidence" entry already say this. The "Span disambiguation" entry must add the Manifest-only condition. Severity: Warning (documentation drift).

## Verified clean

- P20, `header_evidence` (src/charon_names.rs:798-838): on a source other than Manifest, or with no header paths, it returns `Compatible`. With no typed impl segment, it returns `Compatible`. If any header path is missing from the segment's path set, it returns `Contradicted`. Extra segment paths do not contradict. `Exact` still needs a complete header, a path-only segment and equal path sequences in order. The enum doc comments (src/charon_names.rs:774-789), the function doc comment, P20 (kb/engineering/properties.md:198), the glossary "Impl-header evidence" entry, docs/SCHEMA.md `charon-def-id` row, the CHANGELOG 0.12.1 entry and the auditor rule P20 line all describe the same rule.
- P20, lone candidate: `validate_single_candidate` now gets `Exact` only from Manifest candidates, so dropping the explicit `source == Manifest` guard from the `Exact` arm (src/charon_names.rs:948-952) keeps the old behavior: an exact Manifest trait impl matches without the span check, and every LLBC candidate needs the span. The function doc comment (src/charon_names.rs:896-902), P20 ("LLBC candidates are never exact") and the glossary "Candidate resolution" entry ("both only on the Manifest source") agree.
- P20, multi-candidate pipeline: the order is still same file, self type, dedup, header, span (src/charon_names.rs:1001-1052). The header stage drops only `Contradicted` candidates, and if none is left the result is NoMatch. On the LLBC source this stage drops nothing. The `resolve_charon_candidate` step list (src/charon_names.rs:976-978) and the inline comment at 1041-1045 agree.
- Behavior change is fail-closed: on the Manifest source a segment that is a strict subset of the header was `Compatible` before and is `Contradicted` now. This can only remove a `charon-def-id`, never add a wrong one, which matches the Manifest fail-closed contract. `scip_type_symbol_to_path` (src/lib.rs:647-673) keeps only type descriptors (`#` suffix), so constants and other non-type symbols in a header cannot make a spurious missing path. Residual risk: a header type that the manifest renders without its defining path would now reject the candidate. The KB already names the one known case (an alias from another crate). I found no other case in the code; this is not verified against a real manifest beyond the SPQR sample from the previous round.
- Tests: `test_resolve_manifest_missing_header_path_contradicts` pins a Manifest strict-subset segment as `Contradicted` and the exact sibling winning without the span. `test_resolve_llbc_candidate_with_associated_type_not_contradicted` pins the LLBC "each side names a path the other lacks" case as a span match. `test_resolve_single_llbc_candidate_exact_needs_span` now asserts `Compatible` for LLBC. `test_resolve_llbc_subset_candidate_kept_beside_superset_sibling` and `test_resolve_exact_candidate_does_not_drop_compatible_sibling` cover the "compatible never dropped" rule on both sources. All existing `header_evidence` calls pass `EnrichmentSource::Manifest` where they test the evidence rule.
- P3: `named` is a local `HashSet` used only for membership; nothing serialized depends on its order. `impl_header_paths` and `impl_header_complete` stay `#[serde(skip)]`.
- Doc comments in the diff are wrapped as paragraphs and within the line limit.
- P1-P19, C1-C3: this change does not touch them.
