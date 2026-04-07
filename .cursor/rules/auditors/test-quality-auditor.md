# Test Quality Auditor

Verify test coverage against KB properties and identify testing gaps.

## Process

1. Read `kb/engineering/properties.md` to load all invariants (P1-P16, C1-C3)
2. For each property, check if there are tests that exercise it:

### Coverage matrix

Find tests in:
- `src/lib.rs` (inline `#[cfg(test)]` module)
- `src/public_api.rs` (inline tests)
- `src/metadata.rs` (inline tests)
- `src/charon_names.rs` (inline tests)
- `src/charon_cache.rs` (inline tests)
- `src/scip_cache.rs` (inline tests)
- `src/tool_manager.rs` (inline tests)
- `src/rust_parser.rs` (inline tests)
- `src/path_utils.rs` (inline tests)
- `src/error.rs` (inline tests)
- `src/commands/callee_crates.rs` (inline tests)

### What to check

- **Every property (P1-P16) has a corresponding test** — or is explicitly flagged as untested
- **Known issues (C1-C3) have regression tests** — tests should document the current behavior and expected fix
- **Edge cases from SCHEMA.md are tested** — stubs, empty projects, binary-only crates, workspace projects
- **Visibility logic tested** — `is_signature_public` edge cases, three-way `is-public-api` logic, trait impl methods
- **Disambiguation tested** — type context matching, `@line` fallback, substring false-match risk (C3)
- **SCIP format compatibility** — both old (`Type#Trait`) and new (`impl#[Type]`) formats tested for `enrich_display_name`
- **Parser edge cases** — lifetime prefixes, bare `&` references, generic parameters in `extract_fn_qualified_name`

### Impact analysis

For recent changes (check `git log --oneline -20`):
- Identify which properties are affected by recent changes
- Check if those properties have test coverage for the new behavior
- Flag any property-affecting changes without corresponding test additions

## Output

Write findings to `kb/reports/test-report.md` using the standard auditor format.

### Coverage summary table

Include a table:

```markdown
| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1 | metadata::tests::test_wrap_in_envelope_roundtrip | Full | |
| P2 | tests::test_add_external_stubs_creates_missing | Partial | No explicit uniqueness test |
| P7 | (constants verified by kind values) | Indirect | |
| C1 | tests::test_call_after_non_function_def_not_attributed_to_previous_fn | Full | Documents known issue |
```

## Severity guide

- **Critical**: Property with no test coverage at all
- **Warning**: Property with partial coverage, or known issue without regression test
- **Info**: Property-based testing opportunity, or test that could be more precise
