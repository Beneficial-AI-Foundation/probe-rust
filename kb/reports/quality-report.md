---
auditor: code-quality-auditor
pass: second
date: 2026-04-09
status: 0 critical, 0 warnings, 7 info
---

## Critical

_(None.)_

`metadata::SCHEMA_VERSION`, `docs/SCHEMA.md` header and examples, and P1 all agree on **2.3**. The implementation matches the KB for the default SCIP `is-public-api` path, optional `--with-public-api` enrichment (`src/public_api.rs`, `commands/extract.rs`), caching (**P14**), and non-fatal failure (**P17**).

---

## Warnings

_(None.)_

### First-pass warnings — verified resolved

| ID | Topic | Verification |
|----|--------|--------------|
| W1 | Schema drift in SCHEMA.md | Changelog no longer asserts a 2.4 bump; addendum under 2.3 states no schema change. |
| W2 | P11 omitting override | P11 covers default + `--with-public-api`; **Where** includes `public_api.rs` and `extract.rs`. |
| W3 | Glossary **is-public-api** | Entry documents default walk and optional override; links P11/P17. |
| W4 | Architecture | Pipeline step 7, `public_api.rs` in source map, component section, tools table, data-flow node. |
| W5 | P14 / `public-api.txt` | P14 documents cache path and `--regenerate-scip` invalidation. |
| I4 | `main.rs` `--auto-install` help | Line 38 mentions `cargo-public-api` and nightly with `--with-public-api`. |
| I5–I6 | USAGE examples / cargo-public-api section | “With all options” includes `--with-public-api`; dedicated opt-in subsection documents resolution, nightly, cache, and continuation on errors. |

---

## Info

### [I1] Integration test gap for `--with-public-api` (accepted)

- **Location**: `src/public_api.rs` unit tests; `tests/extract_check.rs` (`live_extract_structural_check` uses `with_public_api: false` only)
- **Note**: **Accepted gap** — full extract + `cargo public-api` needs nightly and the external tool; CI-friendly coverage is the `public_api` unit suite plus manual or ignored runs. No code defect.

### [I2] P3 / `HashSet` in `public_api`

- **Location**: `public_api.rs` membership sets during enrichment
- **Note**: Sets are not serialized; atom map remains `BTreeMap`. Satisfies P3 for output paths.

### [I3] P15-style symmetry with P17

- **Location**: `commands/extract.rs` `enrich_with_public_api`
- **Note**: Missing nightly, missing `cargo-public-api`, or `collect_public_api` error warns and returns or falls through without aborting extract; aligns with P17 and Charon-style optional enrichment.

### [I4] Known issues C1–C3

- **Location**: `properties.md` Known issues
- **Note**: Unchanged by this feature; no action required for this audit.

### [I5] Enrichment order: stubs then public API

- **Location**: `extract.rs` — `add_external_stubs` then `enrich_with_public_api`
- **Note**: Stubs without RQN are skipped by design; consistent with `docs/SCHEMA.md`.

### [I6] `USAGE.md` vs P17 wording

- **Location**: `docs/USAGE.md` § cargo-public-api — “override step will fail” when tools missing
- **Note**: Extract still succeeds; optional clarity improvement in docs only (see ambiguity report I3).

### [I7] Example / fixture schema-version lag

- **Location**: `examples/rust_curve25519-dalek_4.1.3.json` still shows `schema-version` **2.2** (historical sample)
- **Note**: Does not affect code; update example when regenerating artifacts if strict doc-example parity is desired.
