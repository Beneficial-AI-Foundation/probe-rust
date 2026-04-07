# Test quality audit — probe-rust

**Date:** 2026-04-07  
**Auditor:** test-quality-auditor (re-audit after P2/P15 fixes)  
**Inputs:** `.cursor/rules/auditors/test-quality-auditor.md`, `kb/engineering/properties.md`, `rg '#\[test\]' src/`

## Executive summary

| Severity  | Count |
|-----------|-------|
| Critical  | **0** |
| Warning   | **5** |
| Info      | **4** |

**Previous criticals (resolved):**

- **P2 (atom / code-name identity):** Confirmed — `src/lib.rs` contains `test_find_duplicate_code_names_none`, `test_find_duplicate_code_names_detects_duplicates`, and `test_find_duplicate_code_names_multiple_groups`, all exercising `find_duplicate_code_names`.
- **P15 (Charon non-fatal):** Confirmed — `src/charon_names.rs` contains `test_charon_failure_is_non_fatal`, documenting that a bad LLBC path yields `Err` while heuristic `rust-qualified-name` on atoms is preserved.

**Test inventory:** `cargo test --lib` reports **116** passing tests; the workspace also runs **3** integration tests (1 ignored) on the binary target. Full suite green at audit time.

## Coverage table (P1–P16, C1–C3)

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| **P1** | `metadata::tests::test_wrap_in_envelope_roundtrip`, `test_unwrap_envelope_*` | Partial | Asserts `schema: probe-rust/extract` and data roundtrip; does not assert `schema-version` matches `metadata::SCHEMA_VERSION` / `docs/SCHEMA.md`. |
| **P2** | `lib::tests::test_find_duplicate_code_names_none`, `test_find_duplicate_code_names_detects_duplicates`, `test_find_duplicate_code_names_multiple_groups` | **Full** | Direct coverage of `find_duplicate_code_names`; re-audit fix verified. |
| **P3** | (types: `BTreeMap` / `BTreeSet` on output paths) | Indirect | No golden-file or hash-stability test for identical inputs → identical JSON. |
| **P4** | `lib::tests::test_add_external_stubs_creates_missing`, `test_external_function_detected` | Partial | Stubs: `code-path` empty asserted; `code-text` 0–0 and empty `dependencies` not asserted in stub test. |
| **P5** | `AtomWithLines::dependencies` as `BTreeSet` | Indirect | Ordering guarantee is by type; no explicit JSON array order snapshot. |
| **P6** | `lib::tests::test_normalize_code_name_strips_trailing_dot` | Full | |
| **P7** | `lib` call-graph tests (`SCIP_KIND_FUNCTION`, `build_call_graph`) | Partial | `constants::is_function_like_kind` not table-tested for Method/Constructor/Macro vs non-function kinds. |
| **P8** | `build_call_graph` via C1/C2-style scenarios | Partial | Core attribution exercised mainly through known-issue and edge-case tests, not a dedicated “happy path” matrix. |
| **P9** | `lib::tests::test_disambiguation_substring_false_match` | Partial | C3 regression covered; full priority order (type context → embed → `@line`) not isolated in small unit tests. |
| **P10** | `lib::tests::test_is_signature_public_*` | Full | |
| **P11** | `public_api::tests::test_enrich_three_way`, parse/extract helpers | Full | |
| **P12** | `public_api::tests::test_is_library_crate_*`, `test_is_not_library_crate_binary_only` | Full | |
| **P13** | `metadata::tests::test_output_path_does_not_escape` | Full | |
| **P14** | `scip_cache::tests::*` | Partial | Paths, error display, auto-install; `--regenerate-scip` and coordinated `public-api.txt` refresh not covered at unit level. |
| **P15** | `charon_names::tests::test_charon_failure_is_non_fatal` | **Full** (unit) | Covers `enrich_atoms_with_charon_names` failure + preserved RQN; see Info for optional CLI-level extract test. |
| **P16** | `lib::tests::test_enrich_impl_method_old_format`, `test_enrich_impl_method_new_format`, inherent/lifetime variants | Full | Old vs new SCIP impl formats for `enrich_display_name`. |
| **C1** | `lib::tests::test_call_after_non_function_def_not_attributed_to_previous_fn` | Full | As documented in KB; constrains P8. |
| **C2** | `lib::tests::test_calls_before_first_definition_are_dropped` | Full | |
| **C3** | `lib::tests::test_disambiguation_substring_false_match` | Full | |

## Findings by severity

### Critical

None.

### Warning

1. **P1 — Envelope version** — Add an assertion that serialized `schema-version` equals `metadata::SCHEMA_VERSION` (and stays aligned with `docs/SCHEMA.md`).
2. **P3 — Deterministic output** — Consider a small fixture-based test: fixed SCIP + fixed sources → snapshot or stable `serde_json::Value` comparison.
3. **P4 — Stub shape** — Extend `test_add_external_stubs_creates_missing` (or add a sibling test) to assert stub `code-text` lines 0–0 and empty `dependencies`.
4. **P7 — SCIP function kinds** — Add a focused test on `is_function_like_kind` (accept 6/17/26/80, reject representative other kinds).
5. **P14 — Cache invalidation** — Unit or integration coverage for `--regenerate-scip` (and public-api cache refresh) behavior if feasible without heavy I/O.

### Info

1. **P5** — Optional snapshot of `dependencies` array order for an atom with multiple deps (redundant with `BTreeSet` but documents JSON contract).
2. **P8** — Optional positive-path SCIP document test: multiple functions, expected callee sets without invoking C1/C2 failure modes.
3. **P9** — Optional tests that pin each disambiguation stage separately (type-context vs `@line` fallback).
4. **P15** — Optional end-to-end test: `extract` with `--with-charon` when Charon/LLBC is missing, expecting warning + successful extract with heuristic names (heavier than current unit test).

## Recent changes (impact sketch)

From `git log --oneline -15`: Charon visibility, disambiguation, path hardening, schema rename, library API exposure. Charon- and disambiguation-related code has substantial coverage in `charon_names` and `lib` tests; **P14** and **CLI/setup** paths remain comparatively light in automated tests.

## Verification notes (re-audit checklist)

- [x] `src/lib.rs`: `test_find_duplicate_code_names_none`, `test_find_duplicate_code_names_detects_duplicates`, `test_find_duplicate_code_names_multiple_groups` present and call `find_duplicate_code_names`.
- [x] `src/charon_names.rs`: `test_charon_failure_is_non_fatal` present; asserts `Err` on missing LLBC and preserved heuristic `rust_qualified_name`.

---

*End of report.*
