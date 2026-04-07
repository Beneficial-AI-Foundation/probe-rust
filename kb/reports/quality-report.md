---
auditor: code-quality-auditor
date: 2026-04-07
status: 0 critical, 0 warnings, 2 info
re_audit: true
previous_critical_resolved:
  - CRI-1 (P3 non-deterministic dependencies-with-locations)
---

## Executive summary

Re-audit after documented fixes. **All previous critical findings are resolved.** Implementation matches KB properties P1–P16 for the spots checked; `SCHEMA_VERSION` matches `docs/SCHEMA.md`. No new critical or warning issues against `kb/engineering/properties.md` or `architecture.md`.

## Verified fixes (this re-audit)

### CRI-1 / P3 — `dependencies-with-locations` determinism

- **Status**: Resolved.
- **Evidence**: In `lib.rs`, after building `dependencies_with_locations`, the vector is sorted with `.sort_by(|a, b| a.line.cmp(&b.line).then(a.code_name.cmp(&b.code_name)))` (approximately lines 1210–1211). `dependencies` remains `BTreeSet<String>` (P5).

### WRN-1 / P14 — `--regenerate-scip` and public-API cache

- **Status**: Documented and implemented consistently.
- **Evidence**: `properties.md` P14 states that `--regenerate-scip` forces SCIP regeneration and invalidates use of the cached `cargo public-api` output (`data/public-api.txt`). `commands/extract.rs` passes `regenerate_scip` into `public_api::collect_public_api`, which skips the cache when `regenerate` is `true` and re-runs `cargo public-api`, then overwrites the cache. `architecture.md` step 2 notes the same coupling.

### WRN-2 / P6 — Trailing-dot normalization call sites

- **Status**: KB updated; code matches.
- **Evidence**: `properties.md` P6 documents normalization in `symbol_to_code_name` and `symbol_to_code_name_full` (plus standalone `normalize_code_name` for tests). `lib.rs` uses those paths for code-name construction and callee resolution (e.g. `symbol_to_code_name` / `symbol_to_code_name_full` at disambiguation and dependency edges).

### WRN-5 — `SCHEMA_VERSION` constant

- **Status**: Resolved.
- **Evidence**: `metadata.rs` defines `pub const SCHEMA_VERSION: &str = "2.2"` and `wrap_in_envelope` sets `schema_version: SCHEMA_VERSION.to_string()`. `docs/SCHEMA.md` header is `Version: 2.2`.

## Property checklist (P1–P16)

| ID | Result | Notes |
|----|--------|--------|
| P1 | Pass | `wrap_in_envelope("probe-rust/extract", ...)`; schema version from `SCHEMA_VERSION` matches docs. |
| P2 | Pass | Duplicates via `find_duplicate_code_names`; output `BTreeMap<String, AtomWithLines>`. |
| P3 | Pass | Atoms map `BTreeSet` deps; `dependencies-with-locations` explicitly sorted (fixes prior CRI). |
| P4 | Pass | `add_external_stubs`: empty `code_path`, `{0,0}` lines, empty deps and locations. |
| P5 | Pass | `AtomWithLines::dependencies: BTreeSet<String>`. |
| P6 | Pass | Normalization embedded per KB in `symbol_to_code_name` / `symbol_to_code_name_full`. |
| P7 | Pass | `constants.rs` `is_function_like_kind`: kinds 6, 17, 26, 80 only. |
| P8 | Pass | `populate_call_relationships`: occurrences sorted by range; `current_function_key` from `symbol_line_to_key` (function-like defs only). C1/C2 remain documented limitations. |
| P9 | Pass | Disambiguation pipeline present (`make_unique_key`, type context, line fallback); C3 documented. |
| P10 | Pass | `is_signature_public`: `pub` not followed by `(`. |
| P11 | Pass | `enrich_atoms_with_public_api`: public set match → `true`; `is_public == Some(true)` and no match → uncertain; else `false`; stubs skipped. |
| P12 | Pass | `is_library_crate`; `enrich_with_public_api` returns early for binary-only crates. |
| P13 | Pass | `sanitize_for_filename` strips `..` and path separators; tests include `test_output_path_does_not_escape`. |
| P14 | Pass | SCIP under `<project>/data/`; regenerate flag wired to public API collection as above. |
| P15 | Not deep-scanned | Charon path: failures logged as warnings in `extract.rs` (spot-check). |
| P16 | Pass | `enrich_display_name` / `extract_bracket_type`: old `#` and new `impl#[Type]` formats. |

## Architecture & glossary

- **architecture.md**: Extract pipeline steps and file map align with `commands/extract.rs`, `lib.rs`, `scip_cache.rs`, `public_api.rs`, `metadata.rs`.
- **glossary.md**: Terms used in reviewed sections are consistent with code (no contradiction found in this pass).

## Known issues (C1–C3)

Still present by design; documented in `properties.md` with associated tests named in KB. Not regressions.

## Info

### [I1] P3 “Where” could mention `dependencies-with-locations` sort

- **Location**: `kb/engineering/properties.md` P3
- **Issue**: P3 points at `BTreeMap`/`BTreeSet` but does not cite the explicit `sort_by(line, code_name)` on `dependencies-with-locations`.
- **Recommendation**: Optionally extend the **Where** line to include `lib.rs` (`convert_to_atoms_with_lines_internal`, sort on `dependencies_with_locations`) for readers tracing determinism.

### [I2] Code-quality auditor skill P6 step vs KB

- **Location**: `.cursor/rules/auditors/code-quality-auditor.md` (P6 bullet)
- **Issue**: Skill text still says to verify `normalize_code_name` is called on code-names and deps; KB P6 states normalization is embedded in `symbol_to_code_name` / `symbol_to_code_name_full`.
- **Recommendation**: Update the auditor skill P6 bullet to match KB wording to avoid false positives on future audits.

## Critical

*(none)*

## Warnings

*(none)*
