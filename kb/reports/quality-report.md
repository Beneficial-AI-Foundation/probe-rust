---
auditor: code-quality-auditor
date: 2026-08-11
status: resolved (was: 1 critical, 4 warnings, 10 info)
---

Scope: working-tree changeset on `fix/scip-staleness-source-facts` (SCIP staleness check, `mod_chain` module-tree walk, foreign/trait-required span facts, new atom fields `file-cfg`/`is-unmounted`/`is-foreign`/`trait-required`, 0.10.0 bump). Audited against P1–P19 and C1–C3. `cargo test` (206 lib + 3 integration) passes; `cargo clippy --all-targets -- -D warnings` is clean.

## Critical

1. **P18 (Complete `cfg` predicate) — file-level inner `#![cfg(...)]` attributes are invisible to both gate collectors.** `rust_parser.rs:329-330` (`parse_file_for_spans` runs `FunctionSpanVisitor` which never folds `syn::File::attrs` into `cfg_stack` — gates are only pushed for `impl`/`trait`/`mod`/`extern` items, `rust_parser.rs:226-242`) and `mod_chain.rs:283-285` (`scan_file` iterates `ast.items` only, ignoring `ast.attrs`). A file whose whole contents are gated by an inner attribute header (`#![cfg(feature = "std")]`, `#![cfg(unix)]` — a common real-world pattern) produces functions whose `cfg` omits that gate entirely, and any `mod` declarations inside such a file are walked ungated. The emitted predicate is therefore not the "complete item-gating predicate" P18 promises, and consumers will treat never-compiled functions as compiled. Fix direction: treat `File::attrs` as an enclosing gate in `FunctionSpanVisitor` and as a chain gate on the file's own mount in `mod_chain`.

## Warnings

1. **P19 (unmounted error direction) — internal tension for sources mounted only from excluded targets.** `mod_chain.rs:151-155` deliberately excludes test/bench/example targets from `package_entries`, and those files are never scanned, so no valve fires for them. A `src/**.rs` file mounted *only* from such a target (e.g. `tests/common.rs` doing `#[path = "../src/util.rs"] mod util;`, or a package whose only targets are integration tests) is compiled by cargo yet gets `is-unmounted: true` whenever every lib/bin walk is clean. P19's definition ("reached by no `mod` chain from its package's target entries (lib and bin roots)") is satisfied, but its own guarantee sentence ("compiled code must never be labeled unmounted") is not. Either the KB should narrow the guarantee to lib/bin compilation, or excluded-target files should be scanned as an additional unmounted-inference valve.

2. **P3 (Deterministic output) — `find_span_info` containment fallback iterates a `HashMap`.** `rust_parser.rs:579-588`: when the exact `(path, name, start_line)` key misses and more than one same-name span in the same file contains the SCIP start line (nested same-name functions; same-named functions from `cfg_if!` both-branch expansion with overlapping spans), the chosen `SpanInfo` depends on `HashMap` iteration order. Pre-existing mechanism, but this changeset widens its blast radius: it now decides `is-foreign` and `trait-required` (and `file-cfg`-independent `cfg`) in addition to `lines-end`. A `BTreeMap` span map or a deterministic tie-break (smallest containing span, then lowest start line) would close it.

3. **Architecture doc drift — `mod_chain.rs` undocumented.** `kb/engineering/architecture.md` (last-updated 2026-04-18) has no entry for `mod_chain.rs` in the source-file map, no mod-chain step in the extract pipeline or data-flow diagram, no component-boundary section, and pipeline step 2 does not mention the new P14 staleness check. `CLAUDE.md`'s project-structure listing also lacks `mod_chain.rs` and `public_api.rs`/`charon_*.rs`. Per the KB rule the code/doc mismatch must be resolved by updating the doc deliberately.

4. **Glossary drift — new domain terms undefined.** `kb/engineering/glossary.md` ("Every domain term used in the KB must be defined here") defines none of the terms P18/P19/SCHEMA.md now rely on: *mount chain*, *file gate* / `file-cfg`, *unmounted*, *foreign (extern-block member)*, *trait-required*, *target entry*, *directory ownership*.

## Info

1. **P14 text understates the implemented check.** `newest_source_mtime` (`scip_cache.rs:268+`) now also includes **directory** mtimes so deletions/renames invalidate the cache (`test_file_deletion_is_stale`) — strictly safer than the KB/CHANGELOG wording ("no `*.rs` file or `Cargo.toml` ... is newer"). Worth folding into P14's text so the KB stays the source of truth.

2. **`is_cache_stale` residual edges** (`scip_cache.rs:103-114`): strict `newest > cache_mtime` means an edit landing in the same mtime-granularity tick as the cache write passes as fresh; unreadable source mtimes are skipped (acknowledged in the doc comment, mildly anti-conservative); `Cargo.lock`/toolchain changes never invalidate (consistent with P14 as written). Also the staleness walk runs twice on the fresh path (`commands/extract.rs:248` and again inside `get_or_generate`) — cost only.

3. **CHANGELOG link definitions**: no `[0.10.0]` link at the bottom; the `[Unreleased]` definition is now dangling (its section was renamed) and stale (`v0.8.1...HEAD`); `[0.9.0]` was already missing (pre-existing).

4. **Auditor checklist stale**: `.cursor/rules/auditors/code-quality-auditor.md` still says "P1-P16, C1-C3" and has no check items for P17–P19.

5. **Mixed predicate rendering**: source-derived gates carry token-stream spacing (`all (test , not (feature = "x"))`, see `mod_chain.rs` tests) while synthesized combinators are compact (`all({}, {})` in `lib.rs:1437-1441`, `any(...)`/`all(...)` in `chains_predicate`). Deterministic, so no P3 issue, but `docs/SCHEMA.md`'s example implies the compact style; consumers must parse `cfg` as cfg syntax, never compare strings. A note in SCHEMA.md would prevent surprises.

6. **`find_package_dirs` prunes any directory named `data` at any depth** (`mod_chain.rs:138-149`): a workspace member rooted under some `data/` directory silently gets no mod-chain facts (cfg incomplete for it; never unmounted — safe direction). Inconsistent with `newest_source_mtime`, which exempts only the top-level cache dir (`test_nested_data_dir_is_not_ignored`).

7. **`cfg_if!` branch predicates are dropped by both walkers** (`rust_parser.rs:244-251`, `mod_chain.rs:345-355`). Conservative (under-gating) and documented in `mod_chain`'s module docs, but P18 and SCHEMA.md present `cfg` as complete without noting this exception.

8. **`file_cfg` doc comment incomplete** (`lib.rs`, `AtomWithLines::file_cfg`): says "Omitted when the file is unconditionally mounted"; SCHEMA.md correctly adds "or was not reachable by the module-tree walk".

9. **Canonicalized keys vs SCIP textual paths**: `relative_key` (`mod_chain.rs:229-232`) strips the canonicalized root from canonicalized file paths; if SCIP's `relative_path` traverses a symlink the `file_gate`/`unmounted` lookups in `lib.rs:1432-1443` silently miss (no gate, no unmounted — conservative direction, same failure mode as the span map's canonicalization guard).

10. **Verified clean**: P1 (schema-version `"3.0"` consistent, `metadata.rs:13` ↔ `docs/SCHEMA.md`); P2/P5–P13/P15–P17 unchanged and spot-checked; **P4** — `add_external_stubs` defaults all four new fields to `None`/`false` so stubs stay `{empty path, {0,0}, no deps}` with the new fields omitted; **serde/schema consistency** — wire names `file-cfg`/`is-unmounted`/`is-foreign`/`trait-required` and skip rules (`Option::is_none`, `Not::not` ⇒ omitted when absent/false) match SCHEMA.md exactly, and `default` attrs keep old JSON deserializable; **cfg folding** — no double-count possible (file component comes only from parent-file `mod` chains via `mod_chain`, own component only from same-file gates via `rust_parser`; disjoint by construction), fold order `all(file, own)` is fixed and `chains_predicate` sorts+dedups alternatives, so output is deterministic; **cross-package soundness** — `analyze` (`mod_chain.rs:73-136`) unions chains across all packages and requires *every* walk clean before any unmounted inference, closing the cross-package `#[path]`-mount and nested-package holes, and making `file_gate` order-independent; **P14 routing** — staleness enforced both in `commands/extract.rs:248` (`get_scip_json`) and in `ScipCache::get_or_generate` (library path), `--regenerate-scip` still forces, `generation_reason` covers the new case; span-map misses now warn (`lib.rs:1185-1192`), never silent; version bump coherent (Cargo.toml = Cargo.lock = CHANGELOG `0.10.0`); C1–C3 unchanged with their regression tests still present.

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
