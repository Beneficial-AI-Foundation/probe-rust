---
auditor: ambiguity-auditor
date: 2026-04-07
status: 0 critical, 3 warnings, 1 info
---

## Re-audit scope

Follow-up after fixes to P11/glossary, Charon tooling table, SCHEMA cross-references, glossary completeness, architecture property citations, and known-issue cross-links. Sources reviewed: `kb/index.md`, `kb/engineering/index.md`, `kb/engineering/properties.md`, `kb/engineering/architecture.md`, `kb/engineering/glossary.md`, `docs/SCHEMA.md` (spot-check: `docs/PUBLIC_API_LIMITATIONS.md` for `is-public-api` wording).

## Resolution of previous findings

| Prior ID | Topic | Verdict |
|----------|--------|---------|
| C1 | P11 `is-public-api: false` meant “not in public API” | **Resolved.** P11 table: `false` = not `pub`; uncertain = absent/`null`. Glossary matches. Aligns with SCHEMA §1 field reference (three-way semantics). |
| W1 | Charon nightly missing from architecture tools table | **Resolved.** External tools table includes Charon with nightly; dedicated nightly row present. |
| W2 | Code-name glossary should reference SCHEMA | **Resolved.** Code-name entry links to SCHEMA Code-Name Format. |
| W3 | `is-public-api` glossary vs P11 | **Resolved.** |
| W4 / W6 | Missing glossary entries (incl. binary/library crate) | **Resolved.** FunctionNode, ScipIndex, BFS, syn, base code-name, disambiguation, function-like definition, library/binary crate entries present. |
| W5 | Architecture pipeline should cite property IDs | **Resolved.** Extract pipeline annotates steps with P1–P16 (including grouped P3/P5/P6, P7–P10/P16, etc.). |
| W7 | Cross-references (properties ↔ glossary, C1–C3 ↔ P8/P9) | **Resolved.** C1–C3 link to glossary anchors and P8/P9. |
| I2 | Charon nightly wording too vague | **Resolved.** Charon subsection states a nightly toolchain compatible with Charon’s upstream requirements. |

**All previous critical and warning items from that list are resolved.**

## Critical

(none)

## Warnings

### [W1] `is-public-api: null` wording vs omitted JSON field

- **Location**: `kb/engineering/properties.md` (P12: “All atoms get `is-public-api: null`”), `docs/PUBLIC_API_LIMITATIONS.md` (match statistics row), informally in P11 table (“absent (`null`)”).
- **Issue**: `AtomWithLines` uses `Option<bool>` with `serde` `skip_serializing_if = "Option::is_none"`, so uncertain / skipped states produce **no** `is-public-api` key in JSON, not a literal `null` value. SCHEMA binary-only text correctly says “absent”; the field reference uses “absent/null” for consumers, which is easy to misread as requiring a JSON null.
- **Recommendation**: In P12 and other KB text, prefer “field omitted” or “absent from JSON” as the canonical on-wire description; keep “logical null / `None`” only when talking about the Rust type or consumer normalization. Optionally align PUBLIC_API_LIMITATIONS statistics wording with the same convention.

### [W2] SCHEMA `is-public` description vs P10

- **Location**: `docs/SCHEMA.md` §1 field reference (`is-public`), `kb/engineering/properties.md` (P10).
- **Issue**: SCHEMA says `true` if the function is “declared `pub`”. P10 is stricter: signature must start with `pub` and the next character must not be `(`, excluding `pub(crate)` and similar from counting as unrestricted `pub`.
- **Recommendation**: Add one sentence to SCHEMA cross-referencing P10 / heuristic, or inline the `pub` / `pub(` nuance so implementers are not misled.

### [W3] “Lexical order” vs “range order” for occurrence walks

- **Location**: `kb/engineering/glossary.md` (**Call attribution**), `kb/engineering/properties.md` (P8).
- **Issue**: Glossary says occurrences are walked in “lexical order”; P8 says “range order”. `lib.rs` (`process_occurrences`) sorts occurrences by SCIP `range` (line, column), which is not the same wording and could be read as a contradiction.
- **Recommendation**: Change the glossary to “range order” (or “ascending SCIP range order”) to match P8 and the implementation.

## Info

### [I1] Section sign in glossary visible text

- **Location**: `kb/engineering/glossary.md`, **Code-name** entry (link label to SCHEMA).
- **Issue**: The label uses `§`, which some viewers render poorly.
- **Recommendation**: Replace with plain text, e.g. “Code-Name Format (SCHEMA.md)”.
