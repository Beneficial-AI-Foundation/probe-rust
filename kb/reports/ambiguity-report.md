---
auditor: ambiguity-auditor
date: 2026-04-18
status: 0 critical, 0 warnings, 3 info
---

## Critical

None

## Warnings

None (previous warnings resolved: glossary entries added for "match key", "multi-crate LLBC", "span disambiguation"; RQN entry updated to describe Charon vs heuristic forms; architecture updated to document multi-crate LLBC handling; `engineering/index.md` property count already correct at P1-P17)

## Info

- **KB `last-updated` staleness** — All KB files are within 30 days: `kb/index.md` and `engineering/index.md`: 2026-04-07; `properties.md`: 2026-04-09; `architecture.md` and `glossary.md`: 2026-04-18 (updated in this cycle). No staleness flag.

- **`docs/SCHEMA.md` and multi-crate** — `rust-qualified-name` is described as heuristic vs "Aeneas-compatible" when `--with-charon` is used; there is no mention of multi-crate LLBC or `--include`. No schema field change is implied by the fixes, but user-visible semantics of enrichment across crates are not documented in user-facing docs. Low priority since `--with-charon` is an internal/advanced flag.

- **LLBC as standalone glossary entry** — "LLBC" appears in architecture and data-flow diagrams but only as an expansion inside the "Charon" glossary line, not as its own entry. Acceptable since the term is always used in the context of Charon; adding a standalone entry would be redundant.
