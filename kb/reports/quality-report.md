---
auditor: code-quality-auditor
date: 2026-08-28
status: resolved (was: 3 warnings, 2 info)
---

Scope: the 0.11.0 changeset — candidate-form passes and impl-descriptor resolution for `--with-public-api` (`src/public_api.rs`, `src/commands/extract.rs`, `extract_bracket_type` visibility in `src/lib.rs`). Audited against P1–P19 and C1–C3. `cargo test` (241 lib + 3 integration) passes; `cargo clippy --all-targets -- -D warnings` is clean. The prior 0.10.0 pass was resolved and is superseded.

## Critical

None.

## Warnings (fixed in this pass)

1. **P11 — the property text no longer matched the code.** The implementation gained three passes (pub-use, trait-default, impl-descriptor resolution) and marks RQN-less atoms, while P11 still said "atoms without an RQN (external stubs) are unaffected". Fixed: P11 now documents the pass order, the per-entry candidate-form set, the uniqueness guards, and states the RQN-less exception explicitly. `docs/SCHEMA.md` (field table, stub-shape list, `--with-public-api` limitations) carried the same stale claim in three places and was corrected to match.
2. **Panic-capable indexing in `resolve_unmatched_to_atoms`.** The blanket check indexed `atoms[key]`. The key provably comes from `atoms.keys()` in the same function, but an `Index` panic is a poor failure mode inside a pass documented as non-fatal (P17). Replaced with `atoms.get(key).is_some_and(…)`.
3. **Glossary terms used before being defined.** `properties.md` and `architecture.md` introduced "candidate form", "entries matched", "inherited default trait method", and "impl-descriptor resolution" with no glossary entries. All four added; the existing **Blanket impl** and **is-public-api** entries were extended (a crate's *own* blanket impl does produce an atom and is resolved, not filtered).

## Info

- **P3 (deterministic output)** holds. The new `HashSet`s (`flat`, `atom_candidate_names`) are membership-only lookups and never iterated into output. Resolution order comes from `BTreeMap`/`BTreeSet` (`PublicNameForms::forms`, the `by_self`/`by_trait` indexes built from `atoms.keys()`), so the marked-atom set is order-independent. Verified empirically: two consecutive dalek extracts produce byte-identical output modulo `timestamp`.
- **P17 (non-fatal)** holds. Every new failure path is graceful: unreadable or unparsable source in `is_blanket_impl_atom` returns `false` (no blanket claim), a missing impl descriptor skips the key, and the whole pass runs inside the existing `Ok(...)` branch of `collect_public_api`.
- **P4 (stub structure)** is untouched — the pass sets only `is-public-api`; stub `code-path`/`code-text`/`dependencies` are unchanged, and stubs belonging to other crates are excluded by the crate guard.
- **Architecture boundaries** respected: all matching logic is in `public_api.rs`; `commands/extract.rs` only wires the passes and prints. The one cross-module change is `extract_bracket_type` becoming `pub(crate)` so key parsing reuses the existing SCIP-descriptor parser instead of duplicating it.
- **Pre-existing contradiction deliberately left alone** (out of scope, unchanged by this work): P11's "Default (no flags)" section describes `classify_public_api` values that the default path does not currently produce.
