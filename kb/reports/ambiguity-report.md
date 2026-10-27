---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: round 2. Uncommitted working tree on top of 97c5f19, branch fix/impl-header-candidate-filter. It adds `CrateNames` and `crate_names` and drops headers with a qualified path (`impl_header_hides_types`). It touches P20, the glossary, docs/SCHEMA.md and CHANGELOG.md. P20 is amended in the same tree. The code and the amendment ship in one PR, so they must merge together.
status: 0 critical, 0 warnings, 1 info
---

## Critical

None.

## Warnings

None.

## Info

### [I1] Secondary docs name two of the three unknown-crate causes
- **Location**: kb/engineering/glossary.md:69, docs/SCHEMA.md:174, CHANGELOG.md:11
- **Issue**: P20 (properties.md:198) lists three causes. The first is a lib and a bin with different crate names. The second is a package without a lib that builds two bins with different names. The third is a missing SCIP version whose other versions disagree. The glossary, the SCHEMA row and the CHANGELOG name only the first and the third. A bin-only package builds no lib, so "lib and bin crates with different names" does not clearly cover the second cause.
- **Evidence**: `crate_names_from_metadata` gives `None` for any lib/bin crate set with more than one name (`_ => None`). The test asserts `name("two-bins", "1.0.0") == None`.
- **Recommendation**: In the three texts, say "the lib and bin targets build more than one crate name". The glossary already points at P20, so this is minor.

## Round-1 fixes verified

- **(a) Unknown-crate causes**: P20 (properties.md:198) now lists all three causes. It also says what happens if the versions disagree: the crate is unknown. The secondary docs keep a small gap (I1).
- **(b) `Input` and "normalizes"**: P20 now defines the binding: "where `impl HasType for Marker` sets `type Item = Input`". It states the claim with a condition: "when it can resolve the projection". The glossary, the CHANGELOG and both rustdoc comments in src/rust_parser.rs use the same form. No "normalizes" wording is left for this rule.
- **(c) Circular wording**: the glossary and the CHANGELOG now make the method the subject. This agrees with P20.
- **(d) Missing test**: the new `trait_only` case in src/lib.rs closes it. See test-report.md.

## Verified clean

- **P20** (properties.md:196-212): every new claim matches the code. The lookup is per package version (`CrateNames::crate_name`). If the version is missing, the lookup falls back to `by_name`. If the versions disagree, `by_name` holds `None`. A package absent from the metadata is read with `-` as `_`. `crate_names` tries `--offline` first and then `--no-deps`. If a reference in range has an unknown crate, `paths_in` returns nothing and the span check decides. `impl_header_hides_types` visits only the trait path and the self type. All names in the **Where** list exist.
- **Stale names**: kb/engineering, docs/, README.md, CHANGELOG.md and src/ hold no `lib_crate_names`, `lib_crate_names_from_metadata` or `impl_header_has_const_expression`.
- **Cross-file consistency**: P20, the glossary, the SCHEMA row and the CHANGELOG agree on the qualified-path rule and the crate-name rule. I1 is the only gap.
- **Glossary terms**: P20 and the glossary use "exact", "compatible", "contradicted" and "complete" with the same meaning.
- **Stamps**: properties.md and glossary.md show `last-updated: 2026-10-27`.
