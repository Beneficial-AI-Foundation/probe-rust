---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB, branch fix/impl-header-candidate-filter)
scope: round 4. Commits 313fc16 and fc9afb8. A type alias defined in the workspace drops the impl header. Two packages with one name and version but different crate names give an unknown crate. fc9afb8 changes only doc comments and one test in src/lib.rs.
status: 0 critical, 0 warnings, 0 info
---

Validation: `cargo fmt --all -- --check` is clean. `cargo clippy --all-targets -- -D warnings` is clean. `cargo test` gives 272 passed and 1 ignored.

KB revision: P20 is amended on this branch. The code and the amended P20 are in the same pull request (#40), so they must merge together. The results below hold against the branch text. I judged only P20 and P3 again.

## Critical

None.

## Warnings

None.

## Info

None.

## Round-3 finding verified

- **I1 (`impl_header_paths` doc)**: fixed. The doc (src/lib.rs:299-309) now lists every case of an empty field: a repeated method name, a non-literal const expression, a qualified path, a type alias defined in the workspace, and an unknown crate. It points to P20.

## Verified clean

- P20, alias rule: `build_impl_header_symbols` (src/lib.rs:622 onward) maps an alias reference to `path: None`. `paths_in` (src/lib.rs:588) collects the references inside the header ranges into `Option<Vec<_>>`, so one `None` empties the header. With an empty header, `impl_header_complete` is `false` (src/lib.rs:1630), and the span check decides.
- P20, conflicting package versions: `crate_names_from_metadata` (src/lib.rs:741) merges `by_version` and `by_name` with one `merge` closure. Entries that disagree give `None`. Entries that agree keep the name.
- P3, deterministic output: the merge result does not depend on the package order in `cargo metadata`.
- Documentation accuracy: I compared P20, the glossary, docs/SCHEMA.md:174, CHANGELOG.md:11 and the doc comments in src/lib.rs with the code. All give the same alias scope (the workspace) and the same known limit (a dependency alias). All list the same three unknown-crate causes.
- Architecture: if Charon enrichment does not run, header symbols are not built (src/commands/extract.rs:56). The alias check stays in lib.rs because it needs SCIP symbol kinds.
- Other properties (P1, P2, P4-P19) and known issues C1-C3: not touched by these commits. Not judged again.
