---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB, working tree on fix/impl-header-candidate-filter)
scope: round 2. Uncommitted working tree on top of 97c5f19. It adds `CrateNames` and `crate_names`, keyed by package name and version. An unknown crate empties `paths_in`. A qualified path drops the header.
status: 0 critical, 0 warnings, 1 info
---

Validation: `cargo fmt --all -- --check` is clean. `cargo clippy --all-targets -- -D warnings` is clean. `cargo test` gives 271 passed and 1 ignored.

KB revision: the impl-header text in P20 is an amendment on this branch. The code and the amended P20 are in the same pull request, so they must merge together. The P20 results below hold against the branch text. I re-judged only P20, and P3 for determinism.

## Info

### [I1] Two docs omit the two-bins cause of an unknown crate
- **Location**: docs/SCHEMA.md:174, CHANGELOG.md:11 (also kb/engineering/glossary.md:69)
- **Issue**: A package without a lib can build two bins with different names. The code then treats its crate as unknown. P20 states this case. The SCHEMA row and the CHANGELOG say only "the package builds lib and bin crates with different names". A reader of these two texts can miss the bin-only case. The behavior is safe, because an unknown crate only removes evidence. See ambiguity-report.md I1.
- **Evidence**: `crate_names_from_metadata` (src/lib.rs:764) gives `None` for any target set with more than one crate name. The test asserts `name("two-bins", "1.0.0") == None`.
- **Recommendation**: Say "the lib and bin targets build more than one crate name".

## Round-1 fix verified

- **I1 of round 1 ("normalizes to the associated type")**: fixed. The new wording is at src/rust_parser.rs:47, 445 and 949. The glossary, the CHANGELOG and P20 use it too. It says that Charon prints the type that the impl binds to `Item`. P20 and the rustdoc of `impl_header_hides_types` name the binding `type Item = Input`.

## Verified clean

- **P20, unknown crate**: `crate_names_from_metadata` collects lib-kind and `bin` target names into a `BTreeSet`, with `-` read as `_`. One name gives `Some`, more than one gives `None`, and none skips the package. `scip_type_symbol_to_path` returns `Some(None)` for a `None` crate. `paths_in` (src/lib.rs:584) collects into `Option<Vec<_>>`, so one unknown reference inside the ranges empties the result. References outside the ranges do not count. With empty paths, `impl_header_complete` is false (src/lib.rs:1613). `header_evidence` (src/charon_names.rs:811) then gives no evidence, and a lone candidate needs the span. This only removes evidence, so it cannot stamp a wrong `charon-def-id`.
- **P20, version keys**: `CrateNames::crate_name` looks up `(package, version)` first. Next it looks up the package name. Last, it reads the package with `-` as `_`. If two versions disagree, or one version is `None`, the name entry is `None`. This matches P20.
- **P20, single entry point**: `build_impl_header_symbols` has one production caller (src/commands/extract.rs:56). It runs only for Charon enrichment. No other path builds header paths, so no path bypasses the unknown-crate rule.
- **P20, qualified path**: `impl_header_hides_types` (src/rust_parser.rs:448) overrides `visit_qself` and visits only the trait path and the self type. syn reaches `QSelf` from nested type arguments too. A qualified path in a const expression is not a simple expression, so `visit_expr` flags it. `visit_item_impl` uses `ImplHeaderFrame::default()` for such an impl, as for a const expression. The test covers a qualified path in the trait argument and in the self type.
- **P20, docs**: all names in the **Where** list exist. kb/engineering, docs/, README.md, CHANGELOG.md and src/ hold no `lib_crate_names` or `impl_header_has_const_expression`. kb/engineering/architecture.md does not list drop conditions, so it needs no change.
- **P3**: `CrateNames` uses `HashMap` for lookup only. No output comes from iterating it. Target names go through a `BTreeSet`.
- P1, P2, P4-P19, C1-C3: this change does not touch them.
