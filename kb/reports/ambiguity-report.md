---
auditor: ambiguity-auditor
date: 2026-08-28
status: resolved (was: 3 critical, 2 warnings, 1 info)
---

Scope: the 0.11.0 changeset's documentation surface — `kb/engineering/{properties,glossary,architecture}.md`, `docs/SCHEMA.md`, `docs/PUBLIC_API_LIMITATIONS.md`, `CHANGELOG.md`. The prior 0.10.0 pass was resolved and is superseded.

## Critical (fixed in this pass)

### [C1] `is-public-api` semantics contradicted between P11, SCHEMA.md, and the code
- **Location**: kb/engineering/properties.md P11; docs/SCHEMA.md field table, stub-shape list, `--with-public-api` limitations
- **Issue**: both said atoms without an RQN are unaffected and stay absent; the new resolution pass marks same-crate RQN-less atoms `true`.
- **Resolution**: P11 documents the pass and names the exception; SCHEMA.md corrected in all three places, distinguishing the analyzed crate's definition-less atoms (markable) from other crates' stubs (never marked).

### [C2] One name, two metrics, silently conflated
- **Location**: docs/PUBLIC_API_LIMITATIONS.md ≤ v2.2 statistics table; new `entries matched` output line
- **Issue**: "130 of 169 matched, 46 unmatched" mixed an atom count with an entry count (130 + 46 = 176 ≠ 169), so no reader could tell which number a change moved.
- **Resolution**: both metrics defined explicitly in the doc, P11, SCHEMA.md, and the glossary, with the reason they move independently. The trait-default pass is documented as entry-count-only on this crate.

### [C3] Factual errors in the limitations doc drove a mitigation that would fix nothing
- **Location**: docs/PUBLIC_API_LIMITATIONS.md ≤ v2.2 Categories B and C
- **Issue**: the type-alias direction was inverted (`pub type EdwardsBasepointTableRadix16 = EdwardsBasepointTable`, `src/edwards.rs:1112` — the alias is the *reported* name and the macro-generated struct is what SCIP indexes), `Vartime{Edwards,Ristretto}Precomputation` were called aliases though they are `pub struct` newtypes, and `BasepointTable::{create, basepoint, mul_base}` were called default methods though only `mul_base_clamped` has a default body. The proposed `pub type` mapping pass would have resolved zero entries.
- **Resolution**: doc rewritten (v3.0) with each correction sourced to a line in pristine crates.io 4.1.3, and the dead mitigation replaced by what the categories actually need.

## Warnings (fixed in this pass)

1. **Undefined terms.** "candidate form", "entries matched", "inherited default trait method", "impl-descriptor resolution" appeared in properties.md/architecture.md with no glossary entries. All added; **Blanket impl** and **is-public-api** extended.
2. **Architecture omitted a pipeline stage.** Step 7 of the diagram and the "Public API override" section described RQN matching only. Both now name the pass sequence.

## Info

- `last-updated` refreshed on properties.md, glossary.md, architecture.md (2026-08-28). `kb/engineering/index.md` is unchanged by this work and stays at 2026-08-11 (17 days, within the 30-day staleness bar).
- P11's pre-existing "Default (no flags)" contradiction (describing `classify_public_api` output the default path does not produce) is **not** addressed here — deliberately out of scope, flagged in the quality report.
