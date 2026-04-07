---
auditor: test-quality-auditor
date: 2026-04-07
status: 1 critical, 9 warnings, 4 info
---

## Coverage Summary

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1 | `metadata::tests::test_wrap_in_envelope_roundtrip`, `tests/extract_check.rs` (`example_json_structural_check` via probe-extract-check) | Partial | `schema` and tool fields asserted; **`schema-version` is not asserted against `SCHEMA_VERSION` / `docs/SCHEMA.md` (2.3)**. `test_unwrap_envelope_with_envelope` uses legacy `"2.0"` in fixture JSON. |
| P2 | `lib::tests::test_find_duplicate_code_names_*`, `tests/extract_check::example_json_atom_keys_have_probe_prefix` | Full | Duplicate detection + example atom keys; extract pipeline dedup is exercised indirectly via integration. |
| P3 | — | Partial | **No golden-file or two-run equality test.** Ordering relies on `BTreeMap` / `BTreeSet` types; not behaviorally locked by a dedicated test. |
| P4 | `lib::tests::test_add_external_stubs_creates_missing` | Partial | Asserts stub `display_name` and empty `code-path`. **Does not assert `code-text` 0–0 or empty `dependencies` (P4).** |
| P5 | — | Partial | `BTreeSet` guarantees lexicographic serialization; **no JSON snapshot asserting dependency array order.** |
| P6 | `lib::tests::test_normalize_code_name_strips_trailing_dot` | Full | Trailing-dot normalization covered. |
| P7 | — | **None** | **`constants::is_function_like_kind` has no unit tests**; `constants.rs` has no `#[cfg(test)]` module. Four allowed kinds vs ignored kinds not explicitly asserted. |
| P8 | `lib::tests::test_call_after_non_function_def_not_attributed_to_previous_fn`, `test_calls_before_first_definition_are_dropped` | Partial | See **C1** / **C2** below. |
| P9 | `lib::tests::test_disambiguation_substring_false_match` | Partial | C3 regression asserts `mul` deps `<= 1`; full disambiguation order (type context → embed → `@line`) not isolated in separate tests. |
| P10 | `lib::tests::test_is_signature_public_*` | Full | `pub`, restricted `pub(...)`, qualifiers, whitespace. Charon override path not covered in `lib` tests (see charon tests). |
| P11 | `lib::tests::test_classify_*` (public API / module chain suite) | Full | Stubs `None`, module chains, trait impl rules covered; `is_module_chain_public` exercised **indirectly** via `classify_public_api` (no direct unit tests; acceptable). |
| P12 | `lib::tests::test_classify_binary_crate_always_false` | Partial | **Effect** of `is_library: false` on `classify_public_api` is tested. **`is_library_crate` (Cargo.toml `[lib]` / `src/lib.rs`) has no unit tests** after removal of `public_api.rs`. |
| P13 | `metadata::tests::test_output_path_does_not_escape` | Full | Traversal-safe output path. |
| P14 | `scip_cache::tests::test_scip_cache_paths`, `test_scip_error_display`, `test_scip_cache_auto_install` | Partial | Paths and flags; **`--regenerate-scip` / cache invalidation behavior not unit-tested.** |
| P15 | `charon_names::tests::test_charon_failure_is_non_fatal` | Full | Non-fatal Charon failure path. |
| P16 | `lib::tests::test_enrich_*`, `test_extract_function_name_*` | Full | Old/new impl formats, lifetimes, free fns. |

## Known issues (C1–C3)

| Issue | Named test | Regression strength |
|-------|------------|---------------------|
| C1 | `test_call_after_non_function_def_not_attributed_to_previous_fn` | **Weak.** Test documents C1; **does not fail** if the call at line 20 is still attributed to `fn_a` (only warns via `eprintln!`). Does not enforce post-fix behavior. |
| C2 | `test_calls_before_first_definition_are_dropped` | **Documents current behavior** (dropped / not attributed to later fn). Matches KB “silently dropped.” |
| C3 | `test_disambiguation_substring_false_match` | **Strong.** Asserts at most one `mul` dependency after disambiguation. |

## New functionality (post–public_api removal)

| API / behavior | Tested? | Tests |
|----------------|---------|--------|
| `build_module_visibility_map` | Yes | `test_build_module_visibility_map_pub_modules`, `test_build_module_visibility_map_skips_extern_crate`, `test_build_module_visibility_map_nested` |
| `classify_public_api` | Yes | `test_classify_*` suite (see edge-case checklist below) |
| `is_trait_impl_symbol` | Yes | `test_is_trait_impl_symbol_new_format`, `_old_format`, `_inherent_method`, `_free_function` |
| `is_library_crate` | **No** | Only **indirect** coverage via `classify_public_api(..., is_library: false)`; no tempdir + `Cargo.toml` / `src/main.rs` tests |
| `is_module_chain_public` | Indirect | Covered by `classify_public_api` tests (not public / not tested in isolation) |

### `is-public-api` edge-case checklist (user-requested)

| Scenario | Covered? | Test |
|----------|----------|------|
| External stubs → `None` | Yes | `test_classify_external_stub_returns_none` |
| Binary crate → `false` | Yes | `test_classify_binary_crate_always_false` (`is_library: false`) |
| `pub fn` at crate root → `true` | Yes | `test_classify_pub_fn_in_root` |
| `pub fn` under public module chain → `true` | Yes | `test_classify_pub_fn_in_pub_module` |
| `pub fn` under private module → `false` | Yes | `test_classify_pub_fn_in_private_module` |
| Non-`pub` fn in public module → `false` | Yes | `test_classify_private_fn_in_pub_module` |
| Trait impl in public module → `true` | Yes | `test_classify_trait_impl_in_pub_module` |
| Trait impl in private module → `false` | Yes | `test_classify_trait_impl_in_private_module` |
| Nested public chain | Yes | `test_classify_nested_pub_chain` |
| Nested broken chain (private parent) | Yes | `test_classify_nested_broken_chain` |

## Other files reviewed

- **`src/constants.rs`**: No tests (feeds **P7** gap).
- **`src/error.rs`**: `test_error_display`, `test_error_from_json` — not mapped to P1–P16; good hygiene.
- **`src/charon_names.rs`**: Rich tests for Charon name matching, visibility propagation, non-fatal failure — align with **P15** / **P10** (Charon override) / enrichment paths.
- **`src/commands/callee_crates.rs`**: BFS / grouping / resolution tests — not KB P-properties; adequate for that command.

## Critical

1. **P7 — SCIP function kinds** — No test asserts that only kinds 6 / 17 / 26 / 80 are treated as function-like, or that representative other kinds are ignored. **`is_function_like_kind` should have a small unit test table** (or a `build_call_graph` fixture with a non-function-like definition occurrence).

## Warnings

1. **P1** — Assert `envelope.schema_version == SCHEMA_VERSION` in `test_wrap_in_envelope_roundtrip` (and align unwrap fixture with 2.3 if intended as canonical).
2. **P3** — Add optional golden or “run convert twice → equal JSON” test on a minimal SCIP fixture if determinism must be locked beyond types.
3. **P4** — Extend `test_add_external_stubs_creates_missing` to assert stub `dependencies` empty and `code-text` lines 0–0.
4. **P5** — Optional: serialize an atom with multiple deps and assert JSON key order (or rely on integration snapshot).
5. **P8 + C1** — Strengthen `test_call_after_non_function_def_not_attributed_to_previous_fn` to **fail** if line-20 call is attributed to `fn_a` once C1 is fixed (today it only prints and asserts line 8).
6. **P9** — Optional focused tests for each disambiguation tier (type context, embed, `@line`).
7. **P12** — Restore **`is_library_crate` filesystem tests** (temp project with only `src/main.rs` vs `src/lib.rs` / `[lib]`) — lost with `public_api.rs` removal.
8. **P14** — Unit or integration test for regenerate / cache hit behavior if risk warrants it.
9. **P12 / extract wiring** — `test_classify_binary_crate_always_false` does not prove `extract` passes `is_library_crate(project)` correctly; only the classification branch.

## Info

1. **`symbol_to_code_name` / `symbol_to_code_name_full`** (P6 pipeline) — Normalization is embedded in the pipeline; **`normalize_code_name` tested in isolation**; optional integration assert on a symbol with trailing `.`.
2. **`tests/extract_check::live_extract_structural_check`** — `#[ignore]`; good for manual/CI-with-tools validation, not default coverage.
3. **`path_utils`, `rust_parser`, `tool_manager`** — Have local tests; outside P1–P16 table but support extract quality.
4. **Integration example** — `examples/rust_curve25519-dalek_4.1.3.json` + probe-extract-check provides broad structural smoke; does not replace property-level unit tests above.
