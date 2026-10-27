---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB), P20 as amended in the working tree
scope: round 2. Uncommitted working tree on top of 97c5f19, branch fix/impl-header-candidate-filter. It adds `CrateNames` and `crate_names`, keyed by package name and version. An unknown crate empties `paths_in`. `impl_header_hides_types` also drops headers with a qualified path. I re-judged only the touched properties.
status: 0 critical, 0 warnings, 0 info
---

`cargo test`: 271 passed (268 unit, 3 integration), 1 ignored.

The P20 text and its **Where** list are amended in the same uncommitted change. I judged the tests against the amended text. The code and the KB edit must land in the same pull request.

## Round-1 finding verified

### [W1 of round 1] Unknown-crate reference outside the header: fixed
- **Location**: src/lib.rs:2803-2822 (end of `test_impl_header_symbols_paths_in`)
- **Check**: the new `trait_only` case uses the `spqr` lib/bin metadata. It queries only the `core::convert::From` reference (line 138, columns 5..9). It asserts `["core::convert::From"]`. The unknown `spqr` references stay outside the range.
- **Mutation**: on a scratch copy in /tmp, I added `if refs.iter().any(|r| r.path.is_none()) { return Vec::new(); }` before the range filter in `paths_in`. `test_impl_header_symbols_paths_in` now fails at src/lib.rs:2819. In round 1 the same mutation passed all tests.

The round-1 mutations of the other new rules are unchanged. The code under them did not change since round 1, so I did not rerun them.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not re-audited | Untouched |
| P12 | n/a | n/a | Retired |
| P13-P17 | existing suites | Not re-audited | Untouched |
| P18 | test_impl_header_dropped_for_hidden_types (cfg of `f`) | Not re-audited | Early-return branch unchanged apart from the rename |
| P19 | existing suites | Not re-audited | Untouched |
| P20 | test_crate_names_from_metadata, test_scip_type_symbol_to_path, test_impl_header_symbols_paths_in, test_impl_header_dropped_for_hidden_types, existing P20 tests | Full for the new rules | Each new rule has a positive and a negative case, and every round-1 mutation fails a test |
| C1-C3 | existing | Not re-audited | Untouched |

## Verified clean

- Crate names from lib and bin targets: a renamed lib, a bin-only package, and a lib and bin with the same crate name. Test and build-script targets are ignored. Two crate names (two bins, or a lib and a bin that differ) give an unknown crate.
- Version keys: two versions with different crate names each resolve to their own name. If a version is missing from the metadata and all versions agree, it uses their name. If they disagree, the crate is unknown. A package missing from the metadata, or metadata JSON that does not parse, gives the package name with `-` as `_`.
- Unknown crate in a header: a header with one unknown reference beside a known one gives no paths. If the file names the unknown package only outside the header, the header keeps its paths.
- Qualified paths drop the header in the trait arguments and in the self type. All const-expression cases still pass.
- `crate_names` itself (the `cargo metadata` runner with the `--offline` and `--no-deps` fallback) has no test. This was true before the rename, and I did not re-judge it.
