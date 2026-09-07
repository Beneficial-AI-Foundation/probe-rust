---
auditor: test-quality-auditor
date: 2026-09-02
status: 0 critical, 0 warnings, 3 info
---

Scope (round 3): the working tree on `fix/rqn-ambiguity` (uncommitted changes on top of `db243e7`), all test-relevant changes in `src/charon_names.rs`, plus the **P20** text in `kb/engineering/properties.md` (unchanged since round 2). Since round 2, two existing tests were extended in place to close the two round-2 warnings; no test was added, deleted, or weakened.

`cargo test` (full suite, this working tree): lib 246 passed / 0 failed; `src/main.rs` 0; `tests/extract_check.rs` 3 passed / 1 ignored; doc-tests 0. `cargo test --lib charon_names`: 63 passed / 0 failed (count unchanged from rounds 1–2 — both fixes extended existing tests).

## Round-2 follow-up

| Round-2 item | Status | Evidence |
|---|---|---|
| **W1** — mix of an excluding usable span and a span-less survivor → Ambiguous unpinned | **Resolved** | `test_resolve_multi_candidate_mixed_span_availability` (`src/charon_names.rs:3234`) gained a second case: same two candidates (span-less `T<u8> for Alpha`, `T<u32> for Alpha` at `(10, 20)`), atom `Alpha::conv (100, 110)`, asserts `Ambiguous`. Traced against the code at `src/charon_names.rs:909-929`: overlaps = `[None, Some(-79)]`, `best = None`, `all(is_some)` is false → `Ambiguous`. Loosening the guard to `any(|o| o.is_some())` would return `NoMatch` and fail this assertion. The inline comment states the intent (Ambiguous, not NoMatch, because NoMatch requires every survivor to carry a usable span). |
| **W2** — unrenderable **self** type skipped in `build_trait_impl_type_info_map` untested | **Resolved** | `test_trait_impl_unformattable_generic_is_placeholder` (`src/charon_names.rs:3443`) gained a `def_id: 3` entry whose `types[0]` is `{"Array": [...]}` (self type) and `types[1]` a renderable `Adt`; asserts `!map.contains_key(&3)`. This pins the `None => continue` at `src/charon_names.rs:312` against the "symmetry" edit that would insert a `?` self type. The downstream `{impl Trait}` rendering for an absent map entry is already pinned by `test_format_name_with_trait_impl_no_self_type` (same `format_name` branch, line 182), so the two tests together cover the clause end to end. |
| **I1** — LLBC `line_start` raw value not asserted at parse time | **Still open (Info)** | No change. Correction to round 2: the test whose fixture uses `beg.line 10 / end.line 20` is `test_build_fun_span_map_extracts_visibility` (`src/charon_names.rs:1591`), not `test_parse_llbc_names_carries_visibility` (whose fixture uses `1/5` and `10/15`). Either would do; the former already holds `map.get(&0)`. Carried as Info 1. |
| **I2** — `?` ceiling (two unrenderable args on one self type) unpinned | **Still open (Info, acknowledged)** | Documented ceiling; `ponytail:` comment at `src/charon_names.rs:320`. Carried as Info 2. |
| **I3** — `in_atom_file` covered only via callers | **Still open (Info)** | No change; helper at `src/charon_names.rs:682`. Carried as Info 3. |

## Coverage summary

P20 is broken out by clause because that is where the change lands; P1–P19 and C1–C3 are untouched by this changeset and their tests are unchanged (spot-checked in round 2; the lib count of 246 passing is unchanged). P15's Manifest sentence remains pinned at the parser level by `from_translation_json_errors_on_bad_path_and_bad_json`; the extract-level "warn and continue" in `commands/extract.rs` is not unit-tested (same status as prior reports).

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P20 — file filter (normalized path; elsewhere / file-less → NoMatch under multi-candidate keys) | `test_resolve_multi_candidate_file_filter` | Full | |
| P20 — self-type filter narrows | `test_resolve_multi_candidate_self_type_and_ambiguity` (first half), `_eq_collision`, `_try_from_split_by_span` | Full | |
| P20 — self type is a narrowing signal only (no match → all same-file proceed) | `test_resolve_multi_candidate_self_type_fallback_then_span` | Full | |
| P20 — dedup on `(def_id, qualified_name)` | `test_resolve_multi_candidate_dedup_same_function` | Full | Manifest source, same `def_id` |
| P20 — strict-max positive span overlap; tie → Ambiguous | `test_resolve_multi_candidate_try_from_split_by_span`, `_span_tie_is_ambiguous` | Full | |
| P20 — trait type args are not a signal; same-self-type impls with unusable spans → Ambiguous | `test_resolve_multi_candidate_self_type_and_ambiguity` (second half), `_try_from_split_by_span` | Full | |
| P20 — lone survivor re-validated (file + span) | `test_resolve_multi_candidate_self_type_survivor_span_excludes`, `test_resolve_single_candidate_cross_file_rejected`, `_same_file_span_mismatch_rejected`, `_same_file_span_match` | Full | |
| P20 — lone file-less LLBC candidate accepted on match key alone; Manifest fails closed | `test_resolve_single_candidate_no_span_preserved`, `test_resolve_single_candidate_manifest_fails_closed_without_file`, `manifest_single_candidate_without_source_is_rejected` | Full | |
| P20 — every survivor has a usable span and none overlaps → NoMatch | `test_resolve_multi_candidate_all_spans_elsewhere_is_no_match`, `_self_type_survivor_span_excludes`, `test_enrich_no_match_preserves_heuristic_rqn` | Full | |
| P20 — mix of an excluding usable span and a span-less survivor → Ambiguous | `test_resolve_multi_candidate_mixed_span_availability` (second case) | Full | Was Partial (round-2 W1) |
| P20 — 1-based line base, compared directly | `from_translation_json_builds_functions_only_records` (`line_start == Some(5)`), `test_enrich_span_disambiguation_single_line_charon_span`, `test_resolve_single_candidate_same_file_span_match` | Partial | Manifest side asserts the parsed value; LLBC side pinned only through resolution — Info 1 |
| P20 — inclusive overlap formula: one-line atom in multi-line span = 1; single-line Charon span in atom = 1; adjacent = 0 → NoMatch | `test_span_overlap_inclusive_one_line_atom`, `test_enrich_span_disambiguation_single_line_charon_span`, `_all_spans_elsewhere_is_no_match` | Full | |
| P20 — missing lines on **either** side yield `None`, not `0` | `test_span_overlap_inclusive_one_line_atom` (atom side), `test_resolve_single_candidate_no_span_preserved`, `_real_file_zero_lines_accepted`, `test_resolve_multi_candidate_mixed_span_availability`, `_eq_collision` | Full | |
| P20 — Match: LLBC overrides RQN and `is-public` (when carried) and stamps def-id/version; Manifest stamps only def-id/version | `manifest_enrich_stamps_def_id_and_keeps_scip_rqn`, `test_enrich_does_not_clobber_is_public_with_none`, `test_enrich_clears_stale_provenance_when_version_missing`, `resolve_enrichment_prefers_manifest_over_llbc` | Full | |
| P20 — Ambiguous: no def-id on either source; LLBC clears RQN, Manifest keeps it | `test_enrich_ambiguous_clears_rqn_and_stamps_nothing` | Full | |
| P20 — NoMatch keeps heuristic RQN | `test_enrich_no_match_preserves_heuristic_rqn`, `test_enrich_clears_stale_provenance_on_no_match`, `manifest_re_enrich_clears_stale_provenance_on_no_match` | Full | |
| P20 — no first-match-wins fallback | (the Ambiguous tests above) | Full | |
| P20 — literal object forms `{"UInt"}`/`{"Int"}`/`{"Float"}` render; unknown encodings → `None` | `test_format_type_literal_object_forms` | Full | |
| P20 — unrenderable trait generic → `?`, keeps names distinct, reaches the rendered qualified name | `test_trait_impl_unformattable_generic_is_placeholder` | Full | `Array` is the only shape; all shapes share the one `unwrap_or_else("?")` branch (line 328) |
| P20 — `?` ceiling (two unrenderable args on one self type → identical RQN, span decides) | (none) | Untested, acknowledged | Info 2 |
| P20 — unrenderable **self** type → type-less `{impl Trait}` segment, no self-type signal | `test_trait_impl_unformattable_generic_is_placeholder` (`def_id: 3` → no map entry), `test_format_name_with_trait_impl_no_self_type` (absent entry → `{impl Trait}`), `test_self_type_from_qualified_name` (`{impl Trait}` carries no self type) | Full | Was Partial (round-2 W2); map-build, format, and self-type-extraction stages each pinned |
| P20 — `self_type_from_qualified_name` / `self_type_from_display_name` | `test_self_type_from_qualified_name`, `test_self_type_from_display_name` | Full | |
| P1–P19, C1–C3 | unchanged since 2026-08-11 report | — | Not touched by this changeset |

Every P20 clause now has Full coverage except the 1-based LLBC parse value (Partial, Info 1) and the acknowledged `?` ceiling (Info 2). No pre-existing test was deleted or weakened in this round; the two touched tests only gained assertions.

## Critical

None.

## Warnings

None.

## Info

1. **LLBC 1-based line base is pinned only through resolution.** `from_translation_json_builds_functions_only_records` asserts the Manifest parser copies `"line": 5` to `line_start == Some(5)`. The LLBC side (`beg`/`end` extraction at `src/charon_names.rs:538-568`) has no equivalent value assertion; `test_enrich_span_disambiguation_single_line_charon_span` (Charon `50` inside atom `[50, 60]`) would still pass if the parser added `1`. One line closes it: `assert_eq!(pub_fn.line_start, 10)` (and `line_end == 20`) in `test_build_fun_span_map_extracts_visibility`, whose fixture already uses `beg.line 10 / end.line 20` and already binds `pub_fn = map.get(&0)`.

2. **`?` ceiling unpinned — acceptable.** Two unrenderable trait args on one self type produce identical RQNs and fall to span alone; documented in P20 and with the `ponytail:` comment at `src/charon_names.rs:320`. No test needed until a real collision shows.

3. **`in_atom_file` tested only through callers** (carried from rounds 1–2). The helper at `src/charon_names.rs:682` is the single file-proof point for both `resolve_charon_candidate` and `validate_single_candidate`; `""`/`None`/cross-file/`/src/`-normalized inputs are all exercised via `test_resolve_multi_candidate_file_filter` and the `_single_candidate_*` tests. No P20 gap; noted for completeness.

---

Summary: 0 critical, 0 warnings, 3 info. Round-2 W1 and W2 are both resolved by in-place test extensions (verified against the code paths they guard); I1–I3 remain open as Info. All P20 clauses are Full except the LLBC raw `line_start` value (Partial, one-line fix in Info 1) and the deliberately untested `?` ceiling. Full suite: 246 lib + 3 integration passed, 0 failed, 1 ignored.
