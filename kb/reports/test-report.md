---
auditor: test-quality-auditor
date: 2026-08-28
status: resolved (was: 2 must-fix)
---

Scope: the 0.11.0 changeset — candidate-form passes and impl-descriptor resolution for `--with-public-api`. All 241 lib tests and 3 integration tests pass. The prior 0.10.0 pass was resolved and is superseded.

## Coverage summary

| Concern | Tests | Coverage |
|---|---|---|
| Trait-default pass, positive | `test_trait_default_inherited_by_concrete_type` | yes |
| Trait-default guards | `test_trait_default_skipped_when_override_exists`, `test_trait_default_ambiguous_traits_skipped`, `test_trait_default_requires_public_trait_method` | yes — override evidence, ambiguous providing trait, non-public default body |
| Entry-level metric | `test_matched_count_reporting` | yes — counts entries, not atoms; expansion never adds or drops entries |
| Impl-descriptor resolution, positive | `test_resolve_marks_impl_atom_without_rqn` | yes — the RQN-less atom is the one that gets tagged |
| Resolution guards | `test_resolve_skips_ambiguous_impl_atoms`, `test_resolve_ignores_other_crates_atoms` | yes — non-unique descriptor, foreign-crate atom |
| Blanket verification | `test_resolve_blanket_impl_generic_self_matches`, `test_resolve_blanket_impl_concrete_self_type_skipped`, `test_resolve_trait_level_entry_via_blanket_impl` | yes — real sources via `write_crate`; the negative uses an identical atom key over a concrete `struct T`, which is the false positive the `syn` check exists to prevent |
| Key descriptor parsing | `test_parse_impl_key_descriptor_forms` | yes (added this pass) |
| P3 determinism with the new pass | none (unit) | verified manually: two dalek extracts byte-identical modulo `timestamp` |

## Must-fix (fixed in this pass)

1. **`parse_impl_key` had no direct test.** It is the pass's parsing core and was exercised only end-to-end. Added `test_parse_impl_key_descriptor_forms`, covering trait and inherent descriptors, backtick/lifetime self types, receiver-prefixed keys, and the two negatives (trait-side declarations, free functions).
2. **The blanket negative did not isolate the risk.** Rewritten so positive and negative share the same atom key and differ only in the source (`impl<T> IsIdentity for T` vs `pub struct T; impl IsIdentity for T`), which is what pins the `syn` check rather than the key heuristic.

## Nice-to-have (not done)

- No fixture-level regression for the new passes: `tests/extract_check.rs` runs against a golden fixture with no `--with-public-api` run, and adding one would require a nightly toolchain plus `cargo-public-api` in CI. The dalek verification stays a documented manual run (`docs/PUBLIC_API_LIMITATIONS.md` carries the numbers).
- No test for a crate whose blanket impl lives inside an inline `mod` (`collect_impls` recurses into `Item::Mod` bodies, currently unexercised).
