---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: round 4. Commits 313fc16 and fc9afb8 on branch fix/impl-header-candidate-filter. A type alias defined in the workspace drops the impl header. Two packages with one name and version but different crate names give an unknown crate. fc9afb8 fixes the wording from round 3. P20 is amended on this branch, not on main. The code and the amendment ship in one PR (#40), so they must merge together.
status: 0 critical, 0 warnings, 0 info
---

## Critical

None.

## Warnings

None.

## Info

None.

## Round-3 findings verified

- **W1 (alias scope)**: fixed. P20 (properties.md:198) and the glossary (glossary.md:69) now say "a type alias defined in the indexed workspace". docs/SCHEMA.md:174, CHANGELOG.md:11 and the doc comments in src/lib.rs say "defined in the workspace". The known limit now names "an alias from a dependency outside the workspace". This matches the code: `aliases` (src/lib.rs:622) holds every alias with a definition in the SCIP index, and `rust-analyzer scip .` indexes the workspace. No "crate-local" or "another crate" remains in the KB, SCHEMA, CHANGELOG or src/lib.rs for this rule.
- **I1 (completeness clause)**: fixed. P20, the glossary and the `impl_header_complete` doc no longer list aliases as something the completeness count guards. The CHANGELOG and SCHEMA exactness conditions now say "no primitive or type parameter".

## Verified clean

- P20, alias rule: P20, the glossary, SCHEMA and CHANGELOG give the same rule and the same reason (Charon expands the alias, and the expansion can drop its arguments).
- P20, a dependency alias: the texts say it reads as a contradiction. That is still true. SCIP renders it as a path such as `dep::Alias`, and the Charon segment does not hold it. Such a header is not exact either, so removing "alias" from the exactness clauses does not allow a wrong match.
- P20, unknown-crate causes: the four texts list the same three causes as `crate_names_from_metadata` (src/lib.rs:741).
- P20 **Where** list: the named functions exist.
- Phrasing: P20 and the glossary say "indexed workspace", and SCHEMA, CHANGELOG and the code say "workspace". In this context both mean the same set. I do not count this as a finding.
- Other properties (P1-P19, C1-C3): not touched by these commits. Not judged again.
