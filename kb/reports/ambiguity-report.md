---
auditor: ambiguity-auditor
date: 2026-04-07
status: 2 critical, 7 warnings, 4 info
---

## Critical

### [C1] Glossary charter violated — terms in `properties.md` without definitions

- **Location**: `kb/engineering/glossary.md` (charter, line 5); `kb/engineering/properties.md` § P8, known issues C1–C2
- **Issue**: The glossary states that every domain term used in the KB must be defined there. `properties.md` uses **SCIP document** (P8: “in a SCIP document”) and **callee references** (“callee references are attributed…”, C1/C2). Neither appears as a glossary entry. **Dependencies** describes the output edge set, not an occurrence-level “callee reference” during SCIP walking, so it does not subsume the term.
- **Recommendation**: Add glossary entries (e.g. **SCIP document** — a document entry in the SCIP index with its own occurrence stream; **Callee reference** — an occurrence that references a called symbol, attributed via P8). Link them from P8 and from **Occurrence** / **Call attribution**.

### [C2] Broken engineering index link — `PUBLIC_API_LIMITATIONS.md` missing

- **Location**: `kb/engineering/index.md` → “Public API limitations” → `../../docs/PUBLIC_API_LIMITATIONS.md`
- **Issue**: That path does not exist under `docs/` (only `SCHEMA.md` and `USAGE.md`). The link is dead and the title still suggests a standalone public-API doc, which is easy to misread after the `cargo-public-api` removal.
- **Recommendation**: Update `engineering/index.md` to point at `docs/SCHEMA.md` (e.g. § “Limitations: `is-public-api`”) or restore/rename a short limitations doc aligned with the SCIP module walk. Remove the broken path.

## Warnings

### [W1] `is-public` described imprecisely in SCHEMA vs P10

- **Location**: `docs/SCHEMA.md` (field reference for `is-public`); `kb/engineering/properties.md` § P10
- **Issue**: SCHEMA says “`true` if the function is declared `pub`”, which readers may interpret as any `pub` qualifier (e.g. `pub(crate)`). P10 restricts to signature starting with `pub` where the next character is not `(`, excluding restricted visibility forms.
- **Recommendation**: Align SCHEMA wording with P10 (unrestricted / “plain” `pub` vs `pub(restricted)`), or add a single sentence pointing to the exact rule in the KB.

### [W2] P11 mixes “absent” and “null” for external stubs

- **Location**: `kb/engineering/properties.md` § P11; `docs/SCHEMA.md` (external stubs + field reference)
- **Issue**: P11’s table uses “absent (`null`)” for stubs. SCHEMA describes omitted fields for stubs. In JSON these are different representations; downstream guidance should pick one or explicitly state “omitted or null” only if both are valid.
- **Recommendation**: Match the serialized form (`serde` typically omits `Option::None`) in P11 and point to SCHEMA; remove “null” if the wire format never emits it.

### [W3] Vague toolchain requirement for Charon

- **Location**: `kb/engineering/architecture.md` § “Charon (optional, external)”
- **Issue**: Phrase “nightly toolchain compatible with Charon’s upstream requirements” does not specify how to verify compatibility (pinned version, repo link, or probe-rust flag docs).
- **Recommendation**: Replace with a concrete pointer (e.g. Charon install doc URL or “same nightly as `charon --version` in CI”) or defer to `USAGE.md` with a link.

### [W4] Missing cross-references — `is-public-api` limitations

- **Location**: `kb/engineering/properties.md` § P11–P12; `kb/engineering/architecture.md` extract pipeline
- **Issue**: `docs/SCHEMA.md` documents re-export and trait-impl heuristic limitations for `is-public-api`. The KB properties state the happy-path rule but do not link to those limitations, so readers may treat P11 as complete.
- **Recommendation**: Add a short note under P11 (or a “See also”) linking to `docs/SCHEMA.md` § “Limitations: `is-public-api`”.

### [W5] Property coverage gap — `dependencies-with-locations`

- **Location**: `docs/SCHEMA.md` § `probe-rust/extract`; `kb/engineering/properties.md` (P1–P16)
- **Issue**: SCHEMA defines `dependencies-with-locations` and `--with-locations`; no invariant names sorting, presence rules, or determinism for that structure.
- **Recommendation**: Add a small property (or extend P5) for location-augmented output: ordering, required fields, and when the array is present.

### [W6] Stale auditor outputs under `kb/reports/`

- **Location**: `kb/reports/quality-report.md`, `kb/reports/test-report.md`
- **Issue**: Reports still reference `src/public_api.rs`, `cargo public-api`, three-way `is-public-api`, and `enrich_atoms_with_public_api`, which conflict with the current SCIP module walk and deleted module. They read as current truth but are outdated relative to P11/P12 and `architecture.md`.
- **Recommendation**: Regenerate reports after updating `.cursor/rules/auditors/*.md`, or archive/delete stale reports with a note until re-run.

### [W7] Stale `.cursor` auditor rules contradict the KB

- **Location**: `.cursor/rules/auditors/code-quality-auditor.md` (P6, P11); `.cursor/rules/auditors/test-quality-auditor.md` (test paths)
- **Issue**: Code-quality auditor asks to verify `normalize_code_name` is used in the main pipeline; `properties.md` P6 states it is **not** called there. It still describes P11 as “three-way” / `enrich_atoms_with_public_api`. Test-quality auditor lists `src/public_api.rs` as a test location.
- **Recommendation**: Update auditor markdown to match P6, P11 (binary SCIP classification + P12), and actual test modules so future audits do not reintroduce wrong expectations.

## Info

### [I1] Changelog dates cluster on one day

- **Location**: `docs/SCHEMA.md` changelog (2.2 and 2.3 both `2026-04-07`)
- **Issue**: Historically accurate or not, same-day minor bumps can confuse readers about ordering.
- **Recommendation**: If 2.2 shipped earlier, use its real date; otherwise a one-line note that both landed in the same release train is enough.

### [I2] `code-path` only implied, not glossed

- **Location**: `kb/engineering/properties.md` P4; `kb/engineering/glossary.md` (**External stub**)
- **Issue**: Non-stub atoms’ `code-path` semantics (relative path rules) are not defined in the glossary; only the empty stub case is mentioned.
- **Recommendation**: Optional one-line glossary entry or a pointer to SCHEMA field reference.

### [I3] “Module visibility map” / module-chain walk unnamed in glossary

- **Location**: `kb/engineering/architecture.md` (pipeline, data flow); `kb/engineering/glossary.md` (**is-public-api**)
- **Issue**: Architecture names `module_visibility` / “module chain” as a pipeline artifact; glossary explains outcome on atoms but not the named internal map/concept.
- **Recommendation**: Add **Module visibility map** (or alias) under **is-public-api** or as its own entry, linking to P11 and SCHEMA limitations.

### [I4] `kb/index.md` status still “draft”

- **Location**: `kb/index.md` front matter
- **Issue**: KB content is detailed and referenced as source of truth; “draft” may understate maturity for external readers.
- **Recommendation**: Bump status when the team agrees the KB is stable, or clarify what “draft” means (e.g. reports subdirectory only).
