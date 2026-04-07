# probe-rust Knowledge Base

- **last-updated**: 2026-04-07
- **status**: draft
- **owner**: lacra

The KB is the source of truth for probe-rust. If the code disagrees with the KB, the code is wrong — unless a deliberate decision changes the KB first.

## Sections

| Section | Purpose |
|---------|---------|
| [Engineering](engineering/index.md) | Architecture, properties (invariants), glossary |
| [Reports](../kb/reports/) | Auditor outputs (generated, not hand-authored) |

## Ecosystem context

probe-rust is one tool in the probe ecosystem. Ecosystem-level documentation (product spec, ADRs, cross-tool schema, merge semantics) lives in the [probe KB](https://github.com/Beneficial-AI-Foundation/probe). This KB covers probe-rust internals only.

## Auditor skills

Three auditor skills live in `.cursor/rules/auditors/`:

| Skill | What it checks |
|-------|---------------|
| `ambiguity-auditor.md` | KB gaps, contradictions, undefined terms, staleness |
| `code-quality-auditor.md` | Implementation vs properties and architecture |
| `test-quality-auditor.md` | Test coverage against properties |

## Development Loop (Ralph Loop)

For every non-trivial implementation task:

1. Implement the change
2. Run all three auditor skills
3. Read audit reports in `kb/reports/`
4. Fix every issue found
5. Repeat steps 2-4 until all auditors pass clean
6. Run `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`

For trivial changes (typo fixes, comment updates, dependency bumps), just run the validation suite.
