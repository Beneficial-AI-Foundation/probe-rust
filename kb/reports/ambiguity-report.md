---
auditor: ambiguity-auditor
date: 2026-08-11
status: resolved (was: 5 critical, 7 warnings, 6 info)
---

Scope: the uncommitted 0.10.0 changeset (SCIP staleness check + source facts: `cfg` chain fold, `file-cfg`, `is-unmounted`, `is-foreign`, `trait-required`). Serde attributes were checked against the "omitted when absent/false" claims (`skip_serializing_if = Option::is_none` / `std::ops::Not::not`) and are consistent; `file-cfg` is described consistently as "already folded into `cfg`" in SCHEMA.md, `lib.rs`, P18, and CHANGELOG.

## Critical

### [C1] "Complete" `cfg` predicate is over-claimed — documented carve-outs contradict the bolded contract
- **Location**: docs/SCHEMA.md `cfg` row ("The **complete** item-gating `#[cfg(...)]` predicate"); kb/engineering/properties.md P18 ("the **complete** item-gating predicate"); src/mod_chain.rs module doc; src/rust_parser.rs `visit_item_macro`
- **Issue**: Three behaviors make the emitted `cfg` weaker than the true gate, all deliberate but none acknowledged where "complete" is claimed: (1) `cfg_if!` branch predicates are dropped — both by `FunctionSpanVisitor::visit_item_macro` (same-file gates) and by the mod-chain walk (`mod_chain.rs`: "the branch predicates are dropped, so any `mod` found inside is under-gated"); (2) files the module-tree walk never reached (invisible macro mounts, unresolvable targets) get no chain component at all; (3) a span-map miss silently drops the function's own+same-file gates (leaving only `file-cfg`), mitigated only by a stderr warning (P14). A consumer reading "complete" may treat `cfg` as sound *and* complete for compiled-ness decisions; it is only conservative-complete (may under-gate).
- **Recommendation**: Qualify "complete" in SCHEMA.md and P18: state the cfg_if!-branch drop, the unreached-file case, and the span-miss degradation (with its conservative direction) explicitly, or link to the `mod_chain` valve list.

### [C2] `is-unmounted` = "rustc never compiles it under any configuration" is false for test/bench/example-target mounts
- **Location**: docs/SCHEMA.md `is-unmounted` row; kb/engineering/properties.md P19; src/mod_chain.rs (`package_entries` doc comment); CHANGELOG.md 0.10.0 Added
- **Issue**: The walk starts only from lib and bin roots — `package_entries` deliberately excludes test/bench/example targets ("they live outside `src/`") and never scans `tests/`, `benches/`, `examples/`, or `build.rs`. But those targets CAN mount `src/**.rs` files (common pattern: `#[path = "../src/test_util.rs"] mod util;` in `tests/it.rs`), and since those files are never scanned, no cleanliness valve fires. Such a file is labeled `is-unmounted: true` while rustc compiles it for the test target — directly contradicting SCHEMA's "rustc never compiles it under any configuration" and P19's fixed error direction "compiled code must never be labeled unmounted".
- **Recommendation**: Either scope the claim honestly ("no `mod` chain from the package's lib/bin targets; files mounted only from test/bench/example targets or build.rs may still be labeled unmounted") in SCHEMA.md and P19, or add a valve for `#[path]` mounts from non-lib/bin targets.

### [C3] Predicate string format is unspecified and the docs' examples show a spacing the tool does not emit
- **Location**: docs/SCHEMA.md `cfg` row (example `all(test, feature = "serde")`); kb/engineering/properties.md P18; src/rust_parser.rs `cfg_predicates_of` (`list.tokens.to_string()`); src/mod_chain.rs `chains_predicate`; test assertions (`not (feature = "std")`, `all (test , not (feature = "benchmarking"))`, `any(not (test), test)`)
- **Issue**: Gate text is taken from syn token streams, yielding `not (feature = "std")` and `all (test , not (…))` (spaces before parens and around commas), while the synthesized combinators (`combine_cfg_predicates`, `chains_predicate`, the `all({fc}, {oc})` fold in lib.rs) emit tight `all(...)`/`any(...)`. Real output therefore mixes two styles inside one predicate (e.g. `any(not (test), test)`), but every documented example uses canonical rustc spacing. A consumer told to "evaluate it against the build config" who implements a parser (or string comparison) from the documented examples can reject or mis-match actual output. No document defines the grammar or states that whitespace is not normalized.
- **Recommendation**: In SCHEMA.md, specify the format: rustc cfg-expression grammar with non-normalized token-stream whitespace ("parse whitespace-insensitively; never compare strings"), and include one real mixed-spacing example.

### [C4] New domain terms used in properties and SCHEMA have no glossary entries
- **Location**: kb/engineering/glossary.md (no entries); used in kb/engineering/properties.md P18/P19, docs/SCHEMA.md, src/mod_chain.rs
- **Issue**: The glossary's own rule is "Every domain term used in the KB must be defined here." P18/P19 and the new SCHEMA rows use, undefined: *mount chain* / *mod chain*, *(file) gate*, *file gate*, *target entries*, *provably complete walk*, *directory owner(ship)*, *unmounted*, *foreign* (extern-block member), *trait-required* / *bodyless trait signature*, and *cfg predicate* itself. "Provably complete" is defined only via a parenthetical valve list that differs slightly between SCHEMA.md, P19, CHANGELOG, and mod_chain.rs ("chain explosion" appears only in mod_chain.rs).
- **Recommendation**: Add glossary entries for mount chain, file gate, target entry, provably complete walk, directory ownership, unmounted, foreign, trait-required, and cfg predicate; make the valve list canonical in one place and link the others to it.

### [C5] "Both facts err on the side of *tracked*" is not true of file gates under an incomplete walk
- **Location**: src/mod_chain.rs module doc ("Both facts err on the side of *tracked*… Gate facts for chains the walk did follow are unaffected"); contradicts docs/SCHEMA.md `cfg`/`file-cfg` contract
- **Issue**: When a *gate-free* mount is invisible (hidden behind an unresolvable `mod` target or a mod-mentioning macro) or cut off by `MAX_CHAINS_PER_FILE`, only the gated chains are recorded, so `file-cfg`/`cfg` come out *stricter* than reality — the missing chain would have removed the gate entirely (cf. `gate_free_chain_wins_over_gated_remount`). `clean = false` disables only unmounted inference, never gate emission. Downstream evaluation then classifies compiled code as not-compiled — erring toward *untracked*, the direction the doc promises never happens. "Those gates were actually seen" is true per chain but misleads about the disjunction's completeness.
- **Recommendation**: Restrict the claim to `is-unmounted`; for file gates, state that an incomplete walk can over-gate (a missed gate-free chain is not represented), or suppress `file_gate` for packages whose walk was unclean.

## Warnings

### [W1] Staleness trigger documented as "*.rs or Cargo.toml newer" but directory mtimes also count
- **Location**: kb/engineering/properties.md P14; CHANGELOG.md 0.10.0 Fixed; src/scip_cache.rs `is_cache_stale` doc vs `newest_source_mtime` doc/code
- **Issue**: `newest_source_mtime` includes **every non-excluded directory's** mtime (deliberately, to catch deletions/renames), so e.g. adding `README.md` to `src/` marks the cache stale. P14, the CHANGELOG, and even the `is_cache_stale` doc comment describe the condition as "some `*.rs` file or `Cargo.toml` … newer" — only the `newest_source_mtime` comment mentions directories. The same file contradicts itself, and P14 reads as a full characterization but is only a necessary condition.
- **Recommendation**: Add "or any containing directory" to P14, the CHANGELOG entry, and the `is_cache_stale` doc.

### [W2] `AtomWithLines::file_cfg` doc omits the not-reachable case
- **Location**: src/lib.rs `file_cfg` doc ("Omitted when the file is unconditionally mounted."); docs/SCHEMA.md `file-cfg` row
- **Issue**: SCHEMA (correctly, matching the code) says omitted "when the file is unconditionally mounted **or was not reachable by the module-tree walk**"; the Rust doc states only the first condition, so a reader of the API doc infers absence ⇒ unconditionally mounted, which is wrong for unreached/unmounted files.
- **Recommendation**: Append "or was not reached by the module-tree walk" to the lib.rs doc comment.

### [W3] "crate root" vs "target entries", and undefined cross-target merge semantics
- **Location**: docs/SCHEMA.md `cfg` row ("mounting its file from the crate root") vs `is-unmounted` row ("target entries (lib and bin roots)"); kb/engineering/properties.md P18 vs P19; src/lib.rs `cfg` doc
- **Issue**: The `cfg`/P18 text speaks of "the crate root" (singular) while the walk starts from *all* target entries and merges chains from different crates (lib + each bin) into one `any(...)` per file. Which "crate root" governs a file mounted by both a lib and a bin — and that the disjunction spans different compilation units — is nowhere stated; a consumer evaluating `cfg` "to decide whether the function is compiled" cannot tell compiled-in-which-target.
- **Recommendation**: Use "target entries (lib and bin roots)" consistently and add one sentence: chains from all of a package's targets are merged, so the predicate means "compiled in at least one target".

### [W4] CHANGELOG wording implies `cfg_if!` gates are folded into `cfg`
- **Location**: CHANGELOG.md 0.10.0 Added ("resolves how each file is mounted … (`#[path]` overrides, inline mods, `cfg_if!`) and folds the `mod`-declaration gates … into each function's `cfg`")
- **Issue**: Listing `cfg_if!` among the resolved mount mechanisms right before "folds the gates" reads as "cfg_if! branch predicates are folded". They are deliberately dropped (mounts recorded ungated). Two reasonable readings, opposite consumer expectations.
- **Recommendation**: Say "`cfg_if!` bodies (mounts followed; branch predicates conservatively dropped)".

### [W5] KB not updated for the new surface: index count, architecture coverage
- **Location**: kb/engineering/index.md ("Numbered invariants (P1-P17)"); kb/engineering/architecture.md (no `mod_chain` component, source-file map lacks `mod_chain.rs`, extract pipeline shows neither the staleness check nor the chain-facts step, P18/P19 referenced nowhere)
- **Issue**: Properties now run P1–P19 but the index still says P1–P17. The architecture doc has no component boundary, pipeline step, or data-flow node for module-chain analysis or cache-staleness — a property coverage gap (P18/P19 unanchored in architecture) per auditor rules 5–6.
- **Recommendation**: Bump the index to P1-P19; add a `mod_chain` component section, `mod_chain.rs` in the source map, and the staleness check + `analyze()` in the pipeline/data-flow diagrams.

### [W6] Stale `last-updated` stamps and stale CLAUDE.md property range
- **Location**: kb/engineering/architecture.md and kb/engineering/glossary.md (2026-04-18), kb/index.md and kb/engineering/index.md (2026-04-07) — all >30 days old on 2026-08-11; CLAUDE.md ("numbered invariants P1-P16")
- **Issue**: Only properties.md was re-stamped in this change. CLAUDE.md's KB summary is two property-generations behind (P16 → now P19).
- **Recommendation**: Refresh stamps when W5/C4 edits land; fix CLAUDE.md's range.

### [W7] `is_cache_stale` "conservative" claim misdescribes the unreadable-source-mtime direction
- **Location**: src/scip_cache.rs `is_cache_stale` doc ("Conservative on missing information: unreadable mtimes on source files are ignored (they cannot prove freshness)")
- **Issue**: Ignoring an unreadable source mtime *favors reuse* (the file can no longer prove staleness) — the non-conservative direction — and the parenthetical says the opposite of what ignoring achieves. Only the unreadable-cache-mtime branch is conservative. Same ambiguity for `newest_source_mtime` returning `None` ⇒ fresh.
- **Recommendation**: Reword: "an unreadable cache mtime counts as stale (conservative); unreadable source mtimes are skipped and so cannot mark the cache stale (best-effort)".

## Info

### [I1] Span-miss warning understates what is lost
- **Location**: src/lib.rs warning text ("their lines-end/cfg are degraded")
- **Issue**: A miss also loses `is-foreign` and `trait-required` (they default to false/omitted), not just lines-end and the own-cfg component.
- **Recommendation**: Mention declaration-kind facts in the warning or in the adjacent comment.

### [I2] Excluded-directory definitions differ between the two new walks
- **Location**: src/scip_cache.rs `newest_source_mtime` (skips `target`/`.git` at any depth, `data/` top-level only — `test_nested_data_dir_is_not_ignored`) vs src/mod_chain.rs `find_package_dirs` (skips any dir named `data` at any depth)
- **Issue**: A package under a nested `data/` directory counts as source for staleness but is invisible to chain analysis (its files silently get no `file-cfg`/`is-unmounted`). Neither SCHEMA nor the KB records either exclusion set.
- **Recommendation**: Align the two exclusion rules or document the difference in P14/P18.

### [I3] External Stubs section not updated for the new fields
- **Location**: docs/SCHEMA.md § External Stubs
- **Issue**: The section enumerates absent fields (`rust-qualified-name`, `is-public`, …) but not `cfg`/`file-cfg`/`is-unmounted`/`is-foreign`/`trait-required` (all omitted for stubs via omit-when-absent/false).
- **Recommendation**: Add the new fields to the stub absence list for completeness.

### [I4] `file-cfg` combinator shape looser than described
- **Location**: docs/SCHEMA.md `file-cfg` row ("gets `any(...)` over the per-chain `all(...)` conjunctions") vs src/mod_chain.rs `chains_predicate`
- **Issue**: Single-gate chains are emitted bare (no `all()` wrapper), alternatives are sorted and deduped, and a single surviving alternative drops the `any()` — a consumer expecting literal `any(all(…), all(…))` nesting won't always see it. Harmless for evaluators, surprising for pattern-matchers; overlaps C3.
- **Recommendation**: Add "(trivial arities collapse; alternatives sorted)" to the row.

### [I5] "Downstream scope policy" is an undefined reference
- **Location**: src/mod_chain.rs `package_entries` doc ("downstream scope policy already treats them as non-library targets")
- **Issue**: Which policy, in which tool (probe-aeneas? VeriLib?), is unnamed and unlinked — the justification for excluding test/bench/example targets (see C2) rests on it.
- **Recommendation**: Name the consumer and behavior, or drop the justification and state the exclusion plainly.

### [I6] Absent `is-unmounted` conflates "mounted" with "walk incomplete"
- **Location**: docs/SCHEMA.md `is-unmounted` row; kb/engineering/properties.md P19
- **Issue**: Omission means either verified-mounted or inference-disabled; a consumer cannot distinguish, and no per-package "walk was clean" signal is emitted. This is the intended conservative design, but the docs never say the negative is uninformative.
- **Recommendation**: One sentence in SCHEMA: "absence carries no information — it does not assert the file is mounted."

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
