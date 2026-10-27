---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: branch fix/impl-header-candidate-filter (a105d5a, dc33019, 716a431, 53e9d6e) vs main, fourth round
status: 0 critical, 0 warnings, 1 info
---

KB revision: audited on the feature branch. The P20 and glossary edits ship in
the same pull request as the code, so there is no cross-PR merge-order
constraint. Third-round W1 and I1 are resolved by 53e9d6e.

## Info

### [I1] Glossary still scopes the lib rename to workspace packages
- **Location**: kb/engineering/glossary.md:69
- **Issue**: The glossary says "A workspace package that renames its `[lib]` is printed under the lib name". Since 53e9d6e, P20 (properties.md:198), the `lib_crate_names` rustdoc and the CHANGELOG cover dependencies too. The sentence is not false, but it is narrower than the rule.
- **Recommendation**: Write "A package, workspace or dependency, that renames its `[lib]` is printed under the lib name (see P20 for the `cargo metadata` fallback)".

## Verified clean

- P20 states the `--offline` read and the `--no-deps` fallback (workspace packages only), matching `lib_crate_names` (src/lib.rs:676-699).
- Glossary known limits now list the text-keyed `cfg`-split limit, matching P20.
- CHANGELOG uses the `md-5` / `md5` example consistently with the rustdoc.
- `build_impl_header_symbols` rustdoc ("each package whose lib target is renamed") matches the code.
- P1-P19, C1-C3: not touched.
