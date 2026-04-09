---
auditor: ambiguity-auditor
pass: second
date: 2026-04-09
status: 0 critical, 0 warnings, 3 info
---

## Critical

_(None.)_

### First-pass items — verified resolved

- **Schema version contradiction (first-pass C1)** — `docs/SCHEMA.md` header, envelope examples, field table, and changelog are aligned on **2.3**. The 2.3 changelog entry includes a **2026-04-09 addendum** for `--with-public-api` as a behavioral option with **no schema change**. This matches `kb/engineering/properties.md` § P1 and `src/metadata.rs` (`SCHEMA_VERSION = "2.3"`).

- **P11 vs optional `cargo-public-api` (first-pass C2)** — `properties.md` § P11 now documents the **default** SCIP module-chain walk (no extra tools) and the **optional** `--with-public-api` override (RQN matching, non-fatal fallback via P17). `glossary.md` **is-public-api** matches. No remaining contradiction between KB and shipped behavior for this feature.

---

## Warnings

_(None.)_

### First-pass warnings — verified resolved

| First-pass ID | Resolution |
|----------------|------------|
| W1 Architecture | Extract pipeline step 7, source map, component boundary “Public API override”, external tools row, and data-flow node for `cargo public-api` are present in `architecture.md`. |
| W2 Missing invariant | **P17** documents public-API override non-fatal behavior (analogous to P15). |
| W3 Glossary gaps | **blanket impl**, **cargo-public-api**, and updated **is-public-api** are defined. |
| W4 P14 | P14 documents `public-api.txt` cache and `--regenerate-scip` coupling. |
| W5 USAGE intro | External tools intro states two **required** tools and optional enrichment. |
| W6 `main.rs` help | `--auto-install` documents `cargo-public-api` and nightly when `--with-public-api`. |
| W7 Stale reports | Superseded by this second-pass report set. |
| I1 “With all options” | Example includes `--with-public-api`. |
| I2 KB `last-updated` | `properties.md`, `architecture.md`, and `glossary.md` show **2026-04-09**. |

---

## Info

### [I1] Root KB index `last-updated` lags engineering files

- **Location**: `kb/index.md` (`2026-04-07`) vs `kb/engineering/*.md` (`2026-04-09`)
- **Note**: Not a contradiction in content; only the top-level KB stamp is older. Bump when convenient for navigational consistency.

### [I2] Illustrative `tool.version` strings still differ across docs

- **Location**: `docs/SCHEMA.md` (`0.4.0` in the envelope example), `docs/USAGE.md` (`0.3.0`), `Cargo.toml` (`0.5.0`)
- **Note**: Examples are non-normative; readers comparing to an installed binary may still be momentarily confused. Optional cleanup: placeholder or sync on release.

### [I3] `USAGE.md` missing-tool wording vs full-run non-fatal

- **Location**: `docs/USAGE.md` § cargo-public-api (opt-in): “override step will **fail** with an actionable error message” when nightly/tool missing without `--auto-install`
- **Note**: Behavior matches **P17** (extract completes; SCIP values kept). The word “fail” applies to the override step, not the process exit code. For parity with the Charon subsection, an explicit “extraction still completes” clause would remove any ambiguity.

---

## Distinction: properties “Known issues” C1–C3

`kb/engineering/properties.md` § **Known issues** C1–C3 are **long-standing** call-attribution / disambiguation limitations (P8/P9). They are **not** the first-pass ambiguity-report C1/C2 (schema / P11), which are **resolved** as above.
