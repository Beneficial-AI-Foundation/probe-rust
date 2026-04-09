---
auditor: test-quality-auditor
pass: second
date: 2026-04-09
status: 0 critical, 6 warnings, 6 info
---

## Critical

_(None.)_ Properties **P1–P17** each have at least one of: dedicated tests, type-level guarantees, or documented integration/structural checks. No property is wholly untested **except** that **P17** has no isolated test for the extract-layer warn-and-continue path (see Info).

---

## Warnings

### [W1] C1 call-attribution test does not regress on the known bug

`test_call_after_non_function_def_not_attributed_to_previous_fn` documents C1 but does not assert that the call at line 20 is **not** attributed to `fn_a`. When the buggy condition holds, it only prints to stderr. KB C1 implies this test should fail or assert correct attribution once behavior is fixed.

### [W2] P17 — non-fatal public-API override untested at extract layer

`enrich_with_public_api` warns and preserves SCIP-derived `is-public-api` on toolchain/tool/`collect_public_api` failure. There is no test that drives this path from `cmd_extract` and asserts stderr or unchanged atoms. **P17** is now explicit in the KB; the gap is unchanged from the first pass aside from property numbering.

### [W3] P3 / P5 — no golden or snapshot test for serialized ordering

Determinism (P3) and lexicographic `dependencies` (P5) rely on `BTreeMap` / `BTreeSet`; no canonical JSON snapshot asserts key or array order.

### [W4] P4 external stub shape is only partially asserted

`test_add_external_stubs_creates_missing` checks empty `code_path` and stub count but not full P4 (`code-text` 0–0, empty `dependencies`, absent `is-public-api`).

### [W5] P1 — `schema-version` not asserted against `SCHEMA_VERSION` in roundtrip test

`metadata::tests::test_wrap_in_envelope_roundtrip` does not assert `envelope.schema_version == SCHEMA_VERSION`. `test_unwrap_envelope_with_envelope` still uses `"schema-version": "2.0"` in the fixture.

### [W6] P14 — cache regeneration semantics

`scip_cache` tests cover paths and flags; no automated test proves `--regenerate-scip` forces refresh of SCIP and `public-api.txt` together.

---

## Info

### [I1] P17 property added — first-pass suggestion closed

P17 now pins public-API override non-fatal behavior. Test mapping: **no dedicated test** for the extract orchestration path; behavior is implied by code review and parity with Charon’s pattern.

### [I2] `public_api` module — unit coverage

Fifteen tests in `src/public_api.rs` cover parsing, blanket filtering, RQN enrichment, and edge cases. Subprocess/cache/`ensure_*` paths depend on environment; not fully exercised in CI.

### [I3] Blanket impl list vs detection tests

`BLANKET_IMPL_TRAITS` and `is_blanket_impl` are covered by synthetic lines; optional hardening: one assertion per listed trait.

### [I4] Integration tests omit `--with-public-api`

`live_extract_structural_check` remains `#[ignore]` with `with_public_api: false`. An additional ignored test could cover end-to-end override when nightly + tool are present (**accepted gap** for default CI).

### [I5] P8 positive-path call attribution

C1/C2/C3 tests focus on edge cases; no small synthetic SCIP test that asserts callee sets for a simple def/ref sequence in isolation.

### [I6] Structural example JSON schema-version

`examples/rust_curve25519-dalek_4.1.3.json` uses `schema-version` **2.2**; structural tests still pass. Regenerate example when updating interchange samples.

---

## Coverage summary

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1 | `metadata::tests::test_wrap_in_envelope_roundtrip`, `test_unwrap_envelope_*` | Partial | `schema-version` not asserted vs `SCHEMA_VERSION`; fixture still `2.0` in one test |
| P2 | `lib::tests::test_find_duplicate_code_names_*` | Full | Dedup in extract exercised |
| P3 | — | Partial | `BTreeMap` / no `HashMap` on output; no golden JSON |
| P4 | `lib::tests::test_add_external_stubs_creates_missing` | Partial | Not all stub fields asserted |
| P5 | Type-level `BTreeSet` on `AtomWithLines` | Partial | No JSON order snapshot |
| P6 | `lib::tests::test_normalize_code_name_strips_trailing_dot` | Full | |
| P7 | `lib::tests::test_is_function_like_kind_*` | Full | |
| P8 | `lib::tests::test_call_after_non_function_def_*`, `test_calls_before_first_definition_*`, `test_disambiguation_substring_false_match` | Partial | C1 test weak; no isolated happy-path attribution test |
| P9 | `lib::tests::test_disambiguation_substring_false_match` | Full | |
| C1 | `lib::tests::test_call_after_non_function_def_not_attributed_to_previous_fn` | Partial | Documents issue; weak regression signal |
| C2 | `lib::tests::test_calls_before_first_definition_are_dropped` | Full | |
| C3 | `lib::tests::test_disambiguation_substring_false_match` | Full | |
| P10 | `lib::tests::test_is_signature_public_*` | Full | |
| P11 | `lib::tests::test_build_module_visibility_map_*`, `test_classify_*`, trait impl cases; `public_api::tests::*` (override path) | Full (default + override logic) | Default: SCIP walk; override: unit-tested in `public_api.rs`, not end-to-end in `tests/` |
| P12 | `lib::tests::test_is_library_crate_*`, `test_classify_binary_crate_always_false` | Full | |
| P13 | `metadata::tests::test_output_path_does_not_escape` | Full | |
| P14 | `scip_cache::tests::*`; `public_api` cache via code paths | Partial | No test that `--regenerate-scip` clears both caches |
| P15 | `charon_names::tests::test_charon_failure_is_non_fatal`, charon cache tests | Partial | Extract-level warn-and-continue not fully unit-tested |
| P16 | `lib::tests::test_enrich_impl_*`, `test_enrich_free_function_unchanged` | Full | |
| **P17** | _(none dedicated)_ | **Info only** | Non-fatal override documented in KB; extract path relies on analogy to P15 / code review; **no fixture test** (would need controlled failure + nightly/tool story) |
| **`public_api` module** | `public_api::tests::*` | Strong unit | Parse, blanket filter, `enrich_atoms_with_public_api`, RQN in/out, stubs unchanged |
| Structural integration | `tests/extract_check.rs::example_json_*`, `live_extract_structural_check` (ignored) | Partial | Example JSON; live extract optional; `with_public_api: false` |

---

## Recent context

Second pass follows KB/doc updates for `--with-public-api`, **P17**, and SCHEMA **2.3** consistency. Implementation under `src/public_api.rs` and `commands/extract.rs` matches the updated engineering KB and `docs/SCHEMA.md`.
