---
auditor: code-quality-auditor
date: 2026-04-18
status: 0 critical, 0 warnings, 7 info
---

## Critical

None

## Warnings

None (previous warnings resolved: module-level comment aligned with implementation, multi-crate enrichment integration test added)

## Info

1. **P3 (Deterministic output)**: Unchanged. `enrich_atoms_with_charon_names` still mutates a `BTreeMap` in sorted key order; `make_match_key_from_charon` is pure string logic with no randomness. `disambiguate_by_span` is deterministic (iterates candidates in fixed order, strict `>` for tie-breaking).

2. **P10 / enrichment**: `enrich_atoms_with_charon_names` is unchanged in structure; successful matches still assign `rust_qualified_name` and `is_public` from the chosen `CharonFunInfo`. The new single-line span handling in `disambiguate_by_span` increases the number of successful matches but does not change the enrichment contract.

3. **P15 (Charon non-fatal)**: No new `unwrap`/`expect` on enrichment paths. `disambiguate_by_span` returns `Option`; callers handle `None` gracefully. LLBC parse failures remain `Result::Err` from existing callers; pipeline non-fatal behavior lives in `commands/extract.rs` / `charon_cache.rs` as before.

4. **Component boundaries**: Edits are confined to `charon_names.rs` (Charon optional enrichment, architecture step 5). No SCIP cache, envelope, or public-api code touched.

5. **Docs / C1–C3**: `docs/SCHEMA.md`, `docs/USAGE.md`, and `README.md` have no references to `make_match_key_from_charon` or `disambiguate_by_span`; no user-facing doc updates required. C1–C3 concern SCIP attribution/disambiguation in `lib.rs`; these changes do not interact with those code paths.

6. **Tests**: New assertions cover multi-crate LLBC match keys (Issue #7), single-line span disambiguation (Issue #8), and a full integration test for multi-crate enrichment. 145 tests pass.

7. **Match-key empty `crate_name` edge**: `make_match_key_from_charon` no longer consults LLBC `translated.crate_name`. For a hypothetical LLBC with no `crate_name` but paths like `my_crate::module::fn`, match keys would change. Real Charon outputs always set `crate_name`; low-probability compatibility footnote.
