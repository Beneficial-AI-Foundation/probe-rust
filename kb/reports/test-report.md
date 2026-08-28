---
auditor: test-quality-auditor
date: 2026-08-11
status: resolved (was: 3 must-fix, 5 nice-to-have)
---

Scope: the uncommitted changeset on `fix/scip-staleness-source-facts` — SCIP cache staleness (P14), complete `cfg` predicates and `mod_chain` walk (P18), and the conservative unmounted/foreign/trait facts (P19). All 206 lib tests pass.

## Coverage summary

| Property / concern | Tests | Coverage | Notes |
|-------------------|-------|----------|-------|
| P14 — staleness detection | `scip_cache`: `test_missing_cache_is_stale`, `test_cache_newer_than_sources_is_fresh`, `test_source_newer_than_cache_is_stale`, `test_manifest_newer_than_cache_is_stale` | Full (unit) | Both trigger classes (`*.rs`, `Cargo.toml`) plus the missing-cache case; deterministic via explicit `set_file_mtime` offsets (no sleeps, race-safe) |
| P14 — prune rules | `test_target_git_and_cache_dirs_ignored`, `test_nested_data_dir_is_not_ignored` | Full (unit) | Nested-`data/` test correctly pins that only the top-level cache dir is exempt. Decoy-mtime weakness: see Warning 3 |
| P14 — "span misses warned, never silent" | (none) | **None** | The `span_misses` counter + `eprintln!` in `convert_to_atoms_with_lines_internal` has no test and is not observable (see Warning 2) |
| P14 — extract wiring | (none; `get_scip_json` gates on `is_cache_stale`) | Indirect | Needs external `scip` tool; unit coverage of `is_cache_stale` is the load-bearing part. New `generation_reason` branch untested (Info 6) |
| P18 — same-file gates (own/impl/mod/trait/extern) | `rust_parser`: pre-existing cfg tests + `test_cfg_gated_extern_block_gates_members`, cfg assert inside `test_foreign_and_trait_required_facts` | Full (unit) | Extern-block gate and member-own gate both covered |
| P18 — mod-chain file gate | `mod_chain`: `gated_and_unmounted_files`, `nested_dirs_and_inline_mods`, `path_override_and_mod_rs`, `path_mounted_file_owns_its_directory`, `mutually_exclusive_path_mounts_produce_no_gate`, `gate_free_chain_wins_over_gated_remount`, `lib_path_override_in_manifest` | Full (unit) | Directory-ownership rules (crate root, `mod.rs`, `#[path]` owner), inline-mod gates, `any(...)` over chains, gate-free-chain suppression all pinned |
| P18 — fold into atom `cfg` + `file-cfg` | `lib.rs`: `test_convert_with_chain_facts` | Full (integration) | `all(file, own)` fold, file-only, own-only-absent, and `file-cfg` provenance all asserted on real temp-dir crate through `convert_to_atoms_with_parsed_spans` |
| P19 — unmounted (positive) | `gated_and_unmounted_files`, `bin_only_package_walks_from_main`, `workspace_members_analyzed_independently`, e2e `dead` atom | Full (unit + integration) | lib-rooted, bin-rooted, and per-workspace-member cases |
| P19 — unmounted valves (conservative direction) | `unresolvable_mod_disables_unmounted_inference`, `include_macro_disables_unmounted_inference`, `unknown_macro_mentioning_mod_disables_unmounted_inference`, `unparsable_file_disables_unmounted_inference` | Partial | 4 of 6 documented valves tested. Mount-cycle and `MAX_CHAINS_PER_FILE` valves untested (Info 1). Cross-package mounts untested and likely a real hole (Warning 1) |
| P19 — cfg_if under-gating ("undecidable stays ungated") | `cfg_if_mounts_are_seen_ungated` | Full (unit) | Branch predicate dropped, mount recorded — both halves asserted |
| P19 — is-foreign / trait-required | `rust_parser::test_foreign_and_trait_required_facts`, e2e | Full (unit + integration) | Extern member vs bodyless trait sig vs defaulted vs impl vs free fn — all five distinctions asserted; multi-line extern decl span asserted |
| Schema — new wire fields (`file-cfg`, `is-unmounted`, `is-foreign`, `trait-required`) | (none at JSON level) | **None** | e2e asserts Rust struct fields only; kebab-case rename + omitted-when-false/None contract from SCHEMA.md untested (Warning 3) |

P1–P13, P15–P17, C1–C3: not directly touched by this changeset (only mechanical `AtomWithLines` initializer additions in their test fixtures); existing coverage unchanged and still passing.

## Critical

None.

## Warnings

1. **Cross-package mounts untested — and by code reading, a real P19/P18 hole.** `mod_chain::analyze_package` computes `unmounted` from each package's own walker only; `facts.unmounted` and `facts.file_gate` are merged across packages keyed by project-relative path. A file under `b/src/` mounted *only* via another workspace member's `#[path = "../b/src/shared.rs"]` is (a) inserted into `unmounted` by package b's provably-complete walk even though crate a compiles it — violating P19's fixed error direction ("compiled code must never be labeled unmounted") — and (b) conversely, a cross-package *gated* mount can insert a `file_gate` for a file compiled unconditionally in its own package (over-gating). `workspace_members_analyzed_independently` covers only the no-cross-mount case. Needs a test with a cross-package `#[path]` mount; expected fix direction: a file reachable in *any* package's chains must not be inferred unmounted (e.g. share the reached-set across packages, or disable inference when a chain escapes the package dir).

2. **P14's warning clause is untested and untestable as written.** The property text says span-map misses are "reported with a warning, never silently", but the `span_misses` counter lives only in a local + `eprintln!` inside `convert_to_atoms_with_lines_internal` — no test asserts the count or the degraded fallback (`lines-end == lines-start`, facts dropped) for a deliberately mismatched node. Suggest surfacing the miss count (return value or injected sink) so the counting itself can be pinned; note also the count covers only the phase-1 `lines_end` lookup while phase-3 re-looks-up span info independently (same matcher, so currently consistent — a divergence would be invisible).

3. **New wire fields have no serde-shape test.** The repo's own precedent (`charon_provenance_fields_serde_shape` in `charon_names.rs`) pins kebab-case names and omission for provenance fields; `file-cfg`, `is-unmounted`, `is-foreign`, `trait-required` get no such test. A typo in a `rename` or a dropped `skip_serializing_if` (SCHEMA.md promises "omitted when false") would ship silently — the e2e test reads Rust struct fields, never the JSON.

## Info

1. **Two P19 valves untested**: the mount-cycle guard (`#[path]` loop → `clean = false`) and the `MAX_CHAINS_PER_FILE` cap. Both are explicitly listed in P19/module docs as inference-disabling valves; neither has a test. Cheap to add and they guard the property's core direction.

2. **`test_target_git_and_cache_dirs_ignored` detection power depends on sub-second mtime granularity.** The decoy files (`target/`, `.git/`, `data/`) are written *after* the cache with natural mtimes; on a filesystem with 1-second mtime resolution their mtimes can equal the cache's, and the strict `newest > cache_mtime` comparison would let a broken prune pass unnoticed. Not flaky (never spuriously fails), but consider `set_mtime`-ing the decoys into the future (or the cache into the past) so a prune regression is guaranteed to trip the assert. The other five staleness tests use explicit ±100 s offsets and are fully deterministic.

3. **Equal-mtime boundary undocumented/untested**: `newest > cache_mtime` treats a source modified in the same clock tick as the cache write as *fresh*. For a staleness check the conservative direction would be `>=`; at minimum the chosen semantics deserve a comment/test. Related untested micro-case: a project with a cache but zero source files (`newest_source_mtime` → `None` → fresh).

4. **`mutually_exclusive_path_mounts_produce_no_gate` name contradicts its body** — it asserts the gate *is* produced (`any(not (test), test)`). The behavior asserted is correct per P18; rename (e.g. `..._produce_any_over_chains`).

5. **Predicate-string assertions pin two spelling regimes.** Token-derived gates keep proc-macro2 spacing (`all (test , not (feature = "benchmarking"))`) while synthesized combinators are tight (`any(...)` from `chains_predicate`, `all(...)` from the lib.rs fold) — the tests codify this mixed wire format. Pinning exact strings is right (downstream parses them), but a proc-macro2 upgrade changing `TokenStream::to_string` spacing will break tests *and* wire format together; worth a normalizing helper eventually. Not brittle in the bad sense — the string is the contract.

6. **Untested minor branches**: `generation_reason`'s new "(cached SCIP data older than sources)" arm; the verbose regeneration message in `get_or_generate`; `gate_free_chain_wins_over_gated_remount` only covers the ungated-mount-first ordering (gated-first is handled by the subset check but unasserted); `src/bin/*.rs` entry as its own dir-owner (bin coverage uses `src/main.rs` only).

7. **Missing fact-combination cases (low risk, orthogonal code paths by reading)**: extern block *inside* a cfg-gated inline mod (mod gate + extern gate + member gate `all(...)`-joined with `is_foreign`); trait fn with a default body *and* its own `#[cfg]` (expect `cfg` present, `trait-required` absent); inline `mod foo {}` shadowing an existing `src/foo.rs` (correctly unmounted by rustc's rules — would pin the ownership logic); `#[path]` pointing outside `src/` (gate keyed project-relative, file skipped by the src-only unmounted scan; outside the project root the gate is silently dropped by `relative_key`).

---

## Resolution round (2026-08-11, same day)

Every finding above was addressed in the same working-tree changeset and the
closure verified by a re-audit against the final code:

- File-level `#![cfg(...)]`: folded in both collectors (`rust_parser.rs`
  `parse_file_for_spans` seeds `cfg_stack` from `File::attrs`; `mod_chain.rs`
  `scan_file` returns `inner_gates` joined into the mount chain before
  recording). Tests in both modules, including descendant propagation.
- Cross-package mounts: `mod_chain::analyze` now unions chains across ALL
  packages and infers unmounted only when EVERY package's walk is clean.
  Tests: `cross_package_path_mount_is_not_unmounted`,
  `cross_package_gated_mount_unions_with_own_ungated_mount`.
- `find_span_info` containment fallback made deterministic
  (innermost-candidate rule via `max_by_key`, never map iteration order).
- Staleness check: `>=` comparison, directory mtimes included (deletions/
  renames), docs aligned (P14, SCHEMA, CHANGELOG, doc comments).
- Span-map misses surfaced: internal conversion returns the miss count,
  warning emitted by the public wrapper, count pinned by test.
- Serde shape of the four new fields pinned
  (`source_fact_fields_serde_shape`).
- Dirty-file gate stripping valve added (mounts kept, gates dropped) with
  test; cycle guard, chain-cap, and inline-mod-shadowing tests added.
- Docs: P18 reworded to "as complete as visible, under-gating only" with the
  cfg_if/unreached-file caveats; P19 scoped to lib/bin target entries with the
  test/bench/example exclusion and the known cross-file macro limitation;
  predicate-format note in SCHEMA; glossary entries added (mount chain, file
  gate, target entry, provably complete walk, directory owner, unmounted,
  foreign declaration, trait-required); architecture/index/CLAUDE.md/auditor
  checklists updated to P1-P19.

Final state: `cargo fmt --check` clean, `cargo clippy --all-targets
-- -D warnings` clean, 219 lib + 3 integration tests pass.

## Codex adversarial round (2026-08-11, cross-model review)

An independent Codex review over the full diff surfaced further soundness
gaps, all fixed and re-validated (229 tests, clippy clean; SymCRust and dalek
keep full facts, zero regressions):

- Dirty walks could still emit `file-cfg` (cross-file over-gating): now ANY
  completeness taint suppresses BOTH facts project-wide; per-file gate
  stripping removed as redundant. Taint causes are collected and printed
  (structural: the `mod` IDENT token, so doc comments/strings do not taint).
- Macro valve hardened: `macro_rules!` definition bodies, block-local
  `#[path] mod` (any item's tokens), and `cfg_attr` on `mod` declarations now
  taint the walk.
- `#[path]` resolution fixed inside inline modules (inline segments + dir
  ownership) and for `#[path]` on the inline module itself (directory
  segment override). Decoy-file tests added.
- Chain pruning is now keyed on (gates, dir-ownership); a cap overflow no
  longer records the unwalked chain.
- Package guards: unreadable manifests and entry-less packages with src
  files taint the walk. `collect_rs_files` no longer follows symlinked dirs.
- Staleness: `Cargo.lock` tracked; project root's own mtime excluded (self-
  invalidation loop on coarse-mtime filesystems); cache JSON published
  atomically (temp + rename).
- Ambiguous span containment matches are refused (counted as warned misses)
  instead of guessing scope-changing facts.
