---
auditor: test-quality-auditor
date: 2026-04-18
status: 0 critical, 0 warnings, 4 info
---

## Coverage summary

| Property / concern | Tests | Coverage | Notes |
|-------------------|-------|----------|-------|
| Single-crate Charon match keys | `test_make_match_key_from_charon` (first four asserts) | Full (unit) | Preserves prior cases: type in braces, `{Trait for Type}`, `{impl Trait}`, and `backend::get_selected_backend` with matching `curve25519_dalek` prefix |
| Multi-crate LLBC match keys (issue #7) | Same test (five new asserts) | Full (unit) | Included-dep impl + free fn (`libsignal_core` vs `signal_crypto`), cross-crate trait impl, bare name, empty string |
| Multi-crate enrichment (issue #7) | `test_enrich_multi_crate_llbc_included_dependency` | Full (integration) | LLBC with `crate_name = "target_crate"` and a `dep_crate` function; asserts both enriched with correct RQN and visibility |
| Single-line span disambiguation (issue #8) | `test_enrich_span_disambiguation_single_line_charon_span` | Full (integration) | Two same-key candidates with single-line spans, atom range contains exactly one; asserts correct candidate chosen by visibility and RQN enrichment |
| Multi-line span disambiguation | `test_enrich_span_disambiguation_carries_visibility` | Full (integration) | Two multi-line span candidates; asserts span overlap picks correct one |
| P15 — Charon non-fatal | `test_charon_failure_is_non_fatal` | Partial | Unit level: `enrich_atoms_with_charon_names` returns `Err`; extract-level subprocess warning test lives in `commands/extract.rs` |

## Critical

None

## Warnings

None (previous warnings resolved: multi-crate enrichment integration test added, empty string edge case covered)

## Info

1. The first four assertions in `test_make_match_key_from_charon` still cover the original single-crate behavior (impl block stripping, trait-object-style impl, inherent-style `{impl …}`, and a normal module path).

2. The five new assertions cover multi-crate scenarios and edge cases (empty string, bare name without `::`) for `make_match_key_from_charon`.

3. `test_charon_failure_is_non_fatal` remains the dedicated P15-oriented test; it does not replace an extract-level subprocess test for `--with-charon` warnings.

4. The two span disambiguation tests (`_carries_visibility` for multi-line, `_single_line_charon_span` for single-line) together cover both branches of the `disambiguate_by_span` overlap computation.
