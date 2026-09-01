---
auditor: test-quality-auditor
date: 2026-09-01
status: resolved 2026-09-01 — 0 open (was 0 critical, 3 warnings, 2 info)
---

Scope: the uncommitted change in `src/charon_names.rs` — multi-candidate Charon resolution reworked into filters (file → self-type → dedup → strict-max span) with a `ResolveOutcome` enum, Ambiguous clearing the atom's `rust-qualified-name`, `format_type` handling object-form literal types — against the new **P20** in `kb/engineering/properties.md`. All 55 `charon_names` unit tests pass (`cargo test --lib charon_names`).

## Coverage summary

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P20 — no arbitrary pick in multi-candidate Charon resolution | `charon_names`: `test_resolve_multi_candidate_eq_collision`, `_try_from_split_by_span`, `_self_type_and_ambiguity`, `_all_spans_elsewhere_is_no_match`, `_dedup_same_function`, `_file_filter`, `test_enrich_ambiguous_clears_rqn_and_stamps_nothing`, `test_format_type_literal_object_forms`, `test_self_type_from_qualified_name`, `test_self_type_from_display_name` | Partial | File filter (elsewhere + file-less → NoMatch), self-type split, dedup collapse, span split, Ambiguous-on-unusable-spans, all-spans-elsewhere → NoMatch, Ambiguous clearing RQN/def-id on both sources, and object-form literals are all pinned. Gaps: span-overlap **tie** → Ambiguous (Warning 1), Manifest fail-closed without file proof (Warning 2), self-type fallback not distinguishable (Warning 3) |

Impact on pre-existing coverage: **no test was deleted or weakened.** The single-candidate policy tests (`test_resolve_single_candidate_cross_file_rejected`, `_same_file_span_mismatch_rejected`, `_same_file_span_match`, `_no_span_preserved`, `_real_file_zero_lines_accepted`) survive unchanged and still exercise `validate_single_candidate` through the `[single]` entry path; the manifest-source tests (`manifest_enrich_stamps_def_id_and_keeps_scip_rqn`, `resolve_enrichment_prefers_manifest_over_llbc`, provenance-clearing tests) are untouched. The deleted production code (`disambiguate_by_span`, the first-match heuristic-RQN fallback) is exactly the behavior P20 forbids; no test depended on it.

## Critical

None.

## Warnings

1. **Span-overlap tie → Ambiguous is untested.** P20's Ambiguous case is asserted only via the *missing-span* route (`test_resolve_multi_candidate_self_type_and_ambiguity` uses `(0, 0)` lines, hitting the `None => Ambiguous` arm). The other Ambiguous arm — two distinct survivors with **equal positive** overlap, `winners.len() > 1` at `src/charon_names.rs:917` — has no test. A regression that broke the strict-max tie check (e.g. `>=` picking the first winner) would reintroduce exactly the arbitrary-pick bug P20 exists to forbid, and no test would fail. Needs one case: two same-file candidates with identical (or equally overlapping) spans, expect `Ambiguous`.

2. **Manifest fail-closed on file-less candidates is a P20 claim with no test.** P20 says a lone survivor is "validated against the atom's file and span (Manifest fails closed on candidates without file proof)". The branch (`src/charon_names.rs:789`, `source == Manifest && !has_usable_file → NoMatch`) predates this change but was untested before and remains untested — no test constructs a Manifest single candidate with `file_path: None`/`""` and asserts `NoMatch` (the LLBC counterpart, lenient acceptance, IS pinned by `test_resolve_single_candidate_no_span_preserved`). Note the branch is unreachable via the multi→1 reduction (the file filter guarantees survivors carry a matching file), so the test belongs on the `[single]` entry path. The reachable multi→1 case worth pinning instead: self-type filter reduces to one candidate whose span excludes the atom → `NoMatch` via `validate_single_candidate`.

3. **Self-type fallback (no candidate's self type matches → keep all same-file candidates) is not distinguishably covered.** The `matching.is_empty() → same_file` branch (`src/charon_names.rs:~880`) is exercised only incidentally by `test_resolve_multi_candidate_all_spans_elsewhere_is_no_match` (legacy `{impl Trait}` blocks yield no self type), but that test expects `NoMatch` — a bug replacing the fallback with an early `NoMatch` would pass it. Needs one positive case: atom `Gamma::conv` whose self type matches no candidate, two same-file candidates with disjoint usable spans, atom overlapping one → expect `Match` of that one via fallback + span.

## Info

1. **NoMatch preserving the heuristic RQN is asserted only structurally.** The `ResolveOutcome::NoMatch => {}` arm in `enrich_atoms` trivially leaves `rust_qualified_name` alone, and no enrich-level test sets a heuristic RQN, drives a multi-candidate `NoMatch`, and asserts it survives (contrast the Ambiguous test, which does assert clearing). Cheap to add as a third arm of `test_enrich_ambiguous_clears_rqn_and_stamps_nothing`-style setup with non-overlapping spans.

2. **Mixed span availability among survivors is untested.** Two distinct survivors where one has a usable overlapping span and the other has no span data resolves to `Match` (the `filter_map` drops the `None`, best is unique). Plausible in real LLBC (manual impl + compiler-generated sibling); currently no test pins whether that should be `Match` (as implemented) or `Ambiguous`.

---

P1–P19, C1–C3: not touched by this changeset; prior report (2026-08-11) findings were resolved and existing coverage is unchanged.

## Resolution (2026-09-01)

All findings verified against the current working tree; `cargo test --lib charon_names` passes 61/61 (was 55 at audit time — the 6 tests below are new).

- **[W1] RESOLVED** — `test_resolve_multi_candidate_span_tie_is_ambiguous` (src/charon_names.rs:3078) pins two same-file survivors with equal positive overlap → `Ambiguous`.
- **[W2] RESOLVED** — `test_resolve_single_candidate_manifest_fails_closed_without_file` (line 3104) pins the Manifest `file_path: None`/`""` single-candidate → `NoMatch` branch, and `test_resolve_multi_candidate_self_type_survivor_span_excludes` (line 3133) pins the reachable multi→1 reduction whose lone survivor's span excludes the atom → `NoMatch` via `validate_single_candidate`.
- **[W3] RESOLVED** — `test_resolve_multi_candidate_self_type_fallback_then_span` (line 3160) pins the positive fallback case: no self-type match keeps all same-file candidates, span then selects a `Match`.
- **[Info 1] RESOLVED** — `test_enrich_no_match_preserves_heuristic_rqn` (line 3214) drives a multi-candidate `NoMatch` through `enrich_atoms` and asserts the heuristic RQN survives.
- **[Info 2] RESOLVED** — `test_resolve_multi_candidate_mixed_span_availability` (line 3186) pins one usable overlapping span + one span-less survivor → `Match` of the overlapping one.
