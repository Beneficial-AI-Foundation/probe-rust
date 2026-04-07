---
auditor: code-quality-auditor
date: 2026-04-07
status: 0 critical, 4 warnings, 3 info
---

## Critical

_(None.)_

## Warnings

### [W1] P3 — Possible non-determinism in span fallback

**Property**: P3 requires the same SCIP input and source files to yield the same JSON; KB cites `BTreeMap` / `BTreeSet` for the atoms map and dependencies.

**Evidence**: `convert_to_atoms_with_parsed_spans` builds `relative_paths` via `collect::<HashSet<_>>().into_iter().collect()`, so file processing order for `build_function_span_map` is not stable across runs. More importantly, `rust_parser::get_function_end_line` uses exact `HashMap::get` first, then falls back to iterating `span_map.iter()` (a `HashMap`). When multiple parsed spans match the containment rule (same file, same bare name, SCIP start line inside more than one span), which match wins depends on hash iteration order.

**Impact**: Edge cases only; typical projects hit the exact key path. Still a gap vs strict deterministic-output intent.

**Where**: `src/lib.rs` (`convert_to_atoms_with_parsed_spans`), `src/rust_parser.rs` (`get_function_end_line`).

### [W2] User-facing docs still describe `cargo-public-api` and old schema

**Check**: Documentation staleness; no stale nightly / `cargo-public-api` requirement for `is-public-api`.

**Evidence**: `CHANGELOG.md` still documents `cargo public-api`, `public_api` module, three-way `is-public-api`, and schema **2.2**. `README.md` and `docs/USAGE.md` still show envelope example with `"schema-version": "2.1"`. None of these reflect Schema **2.3** or the SCIP module-chain approach in `lib.rs`.

**Where**: `CHANGELOG.md`, `README.md`, `docs/USAGE.md`.

### [W3] Process / KB reports and auditor rules reference removed `public_api`

**Check**: No references to deleted `public_api.rs`; P14 no stale public-api cache.

**Evidence**: `src/commands/mod.rs` and the tree contain **no** `public_api` module or `src/public_api.rs`. `commands/extract.rs` and `scip_cache.rs` do **not** mention `public-api.txt` or a public-api cache — P14 coupling to a second artifact is gone.

**Stale artifacts** (still mention `public_api`, `cargo public-api`, or old P11 behavior): `.cursor/rules/auditors/code-quality-auditor.md`, `.cursor/rules/auditors/test-quality-auditor.md`, `kb/reports/test-report.md`, and the previous content of this file. They mislead future audits and the Ralph loop until updated.

### [W4] Schema doc example `tool.version` behind crate version

**Evidence**: `docs/SCHEMA.md` envelope example uses `"version": "0.3.0"` for `probe-rust`; `Cargo.toml` has `version = "0.4.0"`. `schema-version` **2.3** is consistent with `metadata::SCHEMA_VERSION` and the doc header.

## Info

### [I1] Duplicate `is_signature_public` evaluation per atom

**Evidence**: In `convert_to_atoms_with_lines_internal`, `is_signature_public(&data.node.signature_text)` is called twice when building each `AtomWithLines` (for `is_public` and for `classify_public_api`). Harmless; could bind once for clarity.

**Where**: `src/lib.rs`.

### [I2] CLAUDE.md project structure omits Charon modules

**Evidence**: `kb/engineering/architecture.md` lists `charon_cache.rs` and `charon_names.rs` under `src/`; root `CLAUDE.md` “Project Structure” block does not. Not wrong about `is-public-api`, but slightly out of sync with the file map.

### [I3] “Absent / null” wording vs serde

**Evidence**: P11 / glossary describe external stubs as `is-public-api` absent or `null`. `AtomWithLines` uses `skip_serializing_if = "Option::is_none"`, so JSON omits the field rather than emitting `null`. Consumers are unaffected; wording is mildly imprecise.

---

## Property checklist (focused)

| ID | Result | Notes |
|----|--------|--------|
| **P1** | Pass | `metadata::SCHEMA_VERSION` is `"2.3"`; matches `docs/SCHEMA.md` Version and examples. |
| **P11** | Pass | `classify_public_api`: empty `code_path` → `None`; non-library → `Some(false)`; `is_public && is_module_chain_public` → `Some(true)`; `is_trait_impl_symbol && is_module_chain_public` → `Some(true)`; else `Some(false)`. Aligns with `properties.md` and tests in `lib.rs`. |
| **P12** | Pass | `is_library_crate` reads `[lib]` in `Cargo.toml` or `src/lib.rs`; `extract::cmd_extract` passes `is_library` into `convert_to_atoms_with_parsed_spans`. |
| **P3** | Partial | Serialized atoms: `BTreeMap` keys, `BTreeSet` dependencies — OK. Internal `HashMap`/`HashSet` remain on the pipeline; see [W1]. |
| **P14** | Pass | SCIP cache only (`data/`, `index.scip`, `index.scip.json`); no `public-api` cache in implementation. |

## Architecture

| Check | Result |
|--------|--------|
| Pipeline vs `architecture.md` | Pass — `extract.rs`: validate → SCIP → `parse_scip_json` → `build_call_graph` (with `module_visibility`) → `convert_to_atoms_with_parsed_spans` → dedupe → optional Charon → `add_external_stubs` → `wrap_in_envelope`. |
| Data flow diagram | Pass — matches `build_call_graph` 3-tuple and `classify_public_api` inside conversion. |
| `public_api.rs` | Pass — module removed; no `mod public_api` or imports. |

## Files reviewed

`kb/engineering/properties.md`, `kb/engineering/architecture.md`, `kb/engineering/glossary.md`, `docs/SCHEMA.md`, `src/lib.rs`, `src/constants.rs`, `src/metadata.rs`, `src/commands/extract.rs`, `CLAUDE.md`, plus targeted reads of `src/rust_parser.rs`, `src/scip_cache.rs`, `src/commands/mod.rs`, `Cargo.toml`.
