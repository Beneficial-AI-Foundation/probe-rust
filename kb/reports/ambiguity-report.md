---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: branch fix/impl-header-candidate-filter, uncommitted working tree on top of 343a140 (impl-header rules reworded: contradicted means a Manifest segment lacks a header path, only Manifest segments are evidence, and LLBC candidates are always compatible. Touches P20, glossary, docs/SCHEMA.md, CHANGELOG.md, code-quality auditor rule)
status: 0 critical, 0 open (1 warning and 1 info fixed in this round; I2 resolved by the sibling auditors rerunning)
---

KB revision: audited on the feature branch working tree. The P20, glossary,
SCHEMA, CHANGELOG and auditor-rule edits ship in the same pull request as the
code, so there is no cross-PR merge-order constraint.

The rewording is consistent in the five changed files. One unchanged glossary
entry and one unchanged architecture sentence still describe the header stage
as if it applied to both sources.

## Warnings

### [W1] Span disambiguation and architecture.md do not say that the header stage is Manifest-only
- **Location**: kb/engineering/glossary.md:103 ("Span disambiguation") and kb/engineering/architecture.md:100 (Charon component)
- **Issue**: The glossary entry says "Trait type arguments that are paths are compared by the impl-header stage before this one, which drops contradicted candidates; same-self-type impls that differ only in primitive arguments or type parameters ... are split by span alone." This implies that impls that differ in a path argument never reach the span stage as a tie. That is now true only on the Manifest source. On the LLBC source every candidate is compatible (P20, properties.md:198; `header_evidence`, src/charon_names.rs:811), so `PartialEq<Wrapper<Foo>>` and `PartialEq<Foo> for Wrapper<Foo>` are split by span alone (`test_resolve_llbc_subset_candidate_kept_beside_superset_sibling`). architecture.md:100 says "the impl-header filter compares the paths SCIP resolves in the impl header with the candidate's impl segment, so it holds when Charon ran on patched source" with no source qualifier. Neither sentence states the old "each side names a path the other lacks" rule. But both read as if LLBC segments were evidence.
- **Resolution**: fixed with the recommended wording.
- **Recommendation**: In the glossary, write: "On the Manifest source, the impl-header stage compares the trait type arguments that are paths and drops contradicted candidates. On the LLBC source, and for impls that differ only in primitive arguments or type parameters, the span decides alone." In architecture.md, add "(Manifest source only, because LLBC segments are no evidence)" after "impl segment".

## Info

### [I1] Glossary "compatible" lists fewer cases than the rustdoc
- **Location**: kb/engineering/glossary.md:69 ("Impl-header evidence") and src/charon_names.rs:780-784 (`HeaderEvidence::Compatible`)
- **Issue**: The glossary defines compatible as "the segment names every header path and possibly the default and associated type arguments that Charon adds, or there is nothing to compare". The rustdoc also names primitives in the segment (`u8` of `TryFrom<u8>`) and the code also returns Compatible for a segment with the header paths in another order (`Tr<B, A>` for `Tr<A, B>`). Both cases fall under "otherwise" and P20 covers them, so there is no contradiction, but the parenthesis reads as a complete list.
- **Resolution**: fixed with the recommended wording.
- **Recommendation**: "the segment names every header path, possibly in another order or with more types (primitives, or the default and associated type arguments that Charon adds), or there is nothing to compare".

### [I2] Sibling reports name a removed test and the old subset rule
- **Location**: kb/reports/test-report.md:17, 25, 34, 39 and kb/reports/quality-report.md:28
- **Issue**: Outside this auditor's scope. test-report.md lists `test_resolve_header_subset_is_compatible`, which no longer exists (the Manifest case is now `test_resolve_manifest_missing_header_path_contradicts`, src/charon_names.rs:3397), and states "Compatible (superset, subset, ...)". On the Manifest source a subset is now Contradicted. quality-report.md:28 calls the LLBC case "subset-beside-superset", which is still accurate for LLBC.
- **Recommendation**: Regenerate both reports with /test-quality-auditor and /code-quality-auditor on this working tree.

## Verified clean

- No leftover old contradiction rule. "each side names a path the other lacks" survives only in a test docstring (src/charon_names.rs:3572-3573) that describes an LLBC rendering and then says LLBC segments are no evidence. That is a factual description, not the rule. No KB, docs, CHANGELOG or auditor-rule text uses "subset" or "superset" for header evidence.
- Contradicted has one meaning in P20 (properties.md:198), the glossary (glossary.md:69), SCHEMA (`charon-def-id` row), CHANGELOG 0.12.1 and the `HeaderEvidence::Contradicted` rustdoc: a Manifest segment lacks a header path. The code (`header_evidence`) returns Contradicted only on that condition.
- "Only Manifest segments are evidence" and "LLBC candidates are always compatible" agree across P20, glossary, SCHEMA ("LLBC segments are no evidence"), CHANGELOG ("LLBC segments are no evidence ... matched by span as before"), the code-quality rule ("Manifest only") and the code (early return for non-Manifest sources).
- Lone candidate: contradicted is NoMatch and exact trait-impl is Match without span, both Manifest-only. P20 now drops "from the Manifest source" from the exact clause. That is correct, because LLBC candidates are never exact. The glossary "Candidate resolution" entry says "(both only on the Manifest source)", and SCHEMA says "a lone manifest candidate". These agree with `validate_single_candidate`.
- Exact has one definition (same paths, same order, nothing else, complete header) in P20, glossary, SCHEMA, CHANGELOG and the `HeaderEvidence::Exact` rustdoc.
- The P20 example of extra Manifest paths (`TryFrom<u8, alloc::string::String>`) and of LLBC adding an associated type (`{Mul<c::Wrapper, c::Bar> for c::X}`) match the rustdoc and the tests.
- Terms used by the change (impl-header evidence, Manifest source, LLBC source, match key, file proof, span disambiguation) all have glossary entries.
- `last-updated` of properties.md, glossary.md and architecture.md is 2026-10-27. kb/index.md (2026-09-01) and kb/engineering/index.md (2026-08-11) are older than 30 days. This change does not touch what they describe.
