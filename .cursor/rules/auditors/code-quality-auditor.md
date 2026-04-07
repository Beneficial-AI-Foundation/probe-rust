# Code Quality Auditor

Check the implementation against KB-defined properties and architectural constraints.

## Process

1. Read `kb/engineering/properties.md` to load all invariants (P1-P16, C1-C3)
2. Read `kb/engineering/architecture.md` to understand component boundaries
3. Read `kb/engineering/glossary.md` for precise terminology
4. For each property, verify the implementation satisfies it:

### Property checks

- **P1 (Envelope)**: Verify `extract` output uses Schema 2.0 envelope with correct schema string and version matching `docs/SCHEMA.md`
- **P2 (Atom identity)**: Check code-name uniqueness; verify `BTreeMap` keying in output path
- **P3 (Deterministic output)**: Scan for `HashMap`/`HashSet` in serialized output paths; verify `BTreeMap`/`BTreeSet` usage
- **P4 (Stub structure)**: Verify `add_external_stubs` produces empty code-path, `{0,0}` lines, empty deps
- **P5 (Dependencies sorted)**: Confirm `dependencies` field type is `BTreeSet`
- **P6 (Trailing-dot normalization)**: Verify `normalize_code_name` is called on code-names and deps
- **P7 (SCIP function kinds)**: Verify `is_function_like_kind` matches exactly the four documented kinds
- **P8 (Call attribution)**: Verify occurrence walk logic and `current_function_key` behavior
- **P9 (Disambiguation order)**: Verify priority chain: type context -> `<Type>` embed -> `@line` fallback
- **P10 (is-public from SCIP)**: Verify `is_signature_public` logic matches the property definition
- **P11 (is-public-api from SCIP module walk)**: Verify `classify_public_api` returns correct values for pub/private functions, trait impls, external stubs, and binary crates using module visibility map
- **P12 (Binary crate detection)**: Verify `is_library_crate` detection and `classify_public_api` marks all atoms `false` for binary crates
- **P13 (Path sanitization)**: Verify output paths cannot contain `..` or path separators
- **P14 (SCIP caching)**: Verify cache paths and `--regenerate-scip` flag behavior
- **P15 (Charon non-fatal)**: Verify Charon failure produces warning, not error
- **P16 (Display name enrichment)**: Verify both old and new SCIP symbol formats are handled

### Architecture checks

- Component boundaries: each module only does what `architecture.md` says it does
- Data flow: outputs go where the pipeline diagram says
- External tool handling: auto-install, caching, failure modes match documented behavior
- Naming: code uses terms as defined in `glossary.md`

### Documentation staleness checks

- **Schema version**: `metadata.rs` SCHEMA_VERSION matches `docs/SCHEMA.md` header
- **CLI flags**: `main.rs` flags match what `architecture.md` documents
- **Output filenames**: documented paths match what the code produces
- **Known issues (C1-C3)**: check if they are still present or have been fixed

## Output

Write findings to `kb/reports/quality-report.md` using the same format as the ambiguity auditor.

## Severity guide

- **Critical**: Property violation (code contradicts a KB invariant)
- **Warning**: Architectural boundary violation, missing test for a property, documentation drift
- **Info**: Naming inconsistency with glossary, minor style issues
