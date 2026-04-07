# Ambiguity Auditor

Examine the KB for gaps, contradictions, undefined terms, and vague language. Ambiguity in the KB becomes bugs in the code.

## Process

Examine all files in `kb/` and identify:

1. **Undefined or inconsistently used terms** — compare against `kb/engineering/glossary.md`. Every domain term used in the KB must be defined there.

2. **Vague requirements** — flag phrases like "should be fast", "handle errors gracefully", "as appropriate". These need quantification or concrete criteria.

3. **Contradictions between files** — e.g., `properties.md` says X but `architecture.md` says Y, or a property contradicts `docs/SCHEMA.md`.

4. **Missing cross-references** — concepts mentioned but not linked to their KB file or to the relevant source file.

5. **Stale content** — `last-updated` older than 30 days, or references to removed components/fields.

6. **Property coverage gaps** — invariants in `properties.md` not referenced by `architecture.md` or `glossary.md`, or architectural components not covered by any property.

7. **Glossary completeness** — terms used in `properties.md` or `architecture.md` that have no glossary entry.

## Output

Write findings to `kb/reports/ambiguity-report.md` using this format:

```markdown
---
auditor: ambiguity-auditor
date: YYYY-MM-DD
status: N critical, N warnings, N info
---

## Critical

### [C1] Title
- **Location**: kb/file.md, section name
- **Issue**: description
- **Recommendation**: what to do

## Warnings

### [W1] Title
...

## Info

### [I1] Title
...
```

## Severity guide

- **Critical**: Contradictions, undefined terms used in properties, vague requirements
- **Warning**: Missing cross-references, stale dates, incomplete sections
- **Info**: Style inconsistencies, minor phrasing improvements
