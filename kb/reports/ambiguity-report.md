---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: branch fix/impl-header-candidate-filter, uncommitted working tree (impl-header evidence reworded to exact / compatible / contradicted in P20, glossary, docs/SCHEMA.md, CHANGELOG.md, code-quality auditor rule)
status: 0 critical, 0 warnings, 0 info (one info found and fixed in this round)
---

KB revision: audited on the feature branch working tree. The P20 and glossary
edits ship in the same pull request as the code, so there is no cross-PR
merge-order constraint. Fifth-round findings are resolved:

- W1 (weaker "exact" in SCHEMA and CHANGELOG): both now add "in the same order
  and no other type" to the exact condition. docs/SCHEMA.md has it in the
  `charon-def-id` row. Both now match P20 and the glossary.
- I1 (Ambiguous signal list): properties.md:205 now reads "no signal (self type,
  impl header, span) can split".
- I2 (old test name): `test_resolve_impl_header_unknown_falls_back_to_span` is
  gone. Its replacement `test_resolve_header_subset_is_compatible`
  (src/charon_names.rs:3388) and its docstring use "compatible". The repeats test
  is now `test_header_evidence_exact_counts_repeats` and its docstring says "in
  order, repeats included".

## Info

### [I1] Repeats-test docstring states the example the other way round
- **Location**: src/charon_names.rs:3422-3423 (`test_header_evidence_exact_counts_repeats`)
- **Issue**: Outside the KB. The docstring says "a header that names `Error` twice is not exactly a segment that names it once". The test does the opposite: the segment (`From<c::Error> for c::Error`) names `c::Error` twice, the header that names it twice is Exact, and the header that names it once is Compatible. The rule is the same in both directions, so this is phrasing only, but the example does not match the fixture.
- **Recommendation**: "a segment that names `Error` twice is not exactly a header that names it once".
- **Resolution**: fixed. The docstring now uses the recommended wording.

## Verified clean

- No leftover "confirmed" (in the header-evidence sense), "unknown", "no evidence" or "multiplicity" in kb/engineering/, docs/, CHANGELOG.md, .cursor/rules/ or the header code. The "confirmed" at properties.md:205 means "proved by file path", an older and different sense.
- Exact has one definition in P20 (properties.md:198), the glossary (glossary.md:69), SCHEMA, CHANGELOG and the `HeaderEvidence::Exact` rustdoc (src/charon_names.rs:777-779). Each says the same paths, in the same order, and nothing else. Each states that order counts (`Tr<B, A>` vs `Tr<A, B>`).
- Compatible (superset, subset, nothing to compare) agrees between the glossary and the `HeaderEvidence::Compatible` rustdoc (src/charon_names.rs:780-786). Contradicted, compatible and exact partition the cases.
- Behavior agrees across P20, the glossary entries "Candidate resolution" and "Span disambiguation", SCHEMA, CHANGELOG, architecture.md:100 and the code-quality auditor rule. Contradicted candidates are dropped. When every candidate is contradicted, the result is NoMatch. Compatible candidates go to the span stage, and none is dropped in favor of another. A lone contradicted candidate is NoMatch. Only a lone exact trait-impl candidate from the Manifest source skips the span check. LLBC and inherent-impl candidates always need it.
- P20 **Where** symbols exist: `FunctionSpan::impl_header` and `FunctionSpan::impl_header_path_count` (src/rust_parser.rs:42, 51), `build_impl_header_symbols`, `lib_crate_names`, `AtomWithLines::impl_header_complete` (src/lib.rs:608, 682, 314).
- `last-updated` of properties.md, glossary.md and architecture.md is 2026-10-27. kb/index.md (2026-09-01) and kb/engineering/index.md (2026-08-11) are older than 30 days. This change does not touch what they describe.
- P1-P19, C1-C3: not touched.
