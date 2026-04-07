# Properties and Invariants

- **last-updated**: 2026-04-07

Every property here must hold in the implementation. If a property is violated, it is a bug in the code, not in the KB — unless a deliberate decision changes the KB first.

## Output properties

### P1 — Envelope

The `extract` command wraps output in a Schema 2.0 envelope with `schema: "probe-rust/extract"`. The `schema-version` field must match the version in `docs/SCHEMA.md`. Currently `"2.2"`.

**Where**: `metadata.rs` (`wrap_in_envelope`, `SCHEMA_VERSION`), `commands/extract.rs`.

### P2 — Atom identity

Each atom's identity is its **code-name**. The output JSON is a map keyed by code-name (`BTreeMap<String, AtomWithLines>`). Keys must be unique within a single output file.

**Where**: `lib.rs` (`find_duplicate_code_names`), `commands/extract.rs` (dedup into `BTreeMap`).

### P3 — Deterministic output

Same SCIP input + same source files = same JSON output. Enforced by using `BTreeMap` for the atoms map and `BTreeSet` for dependencies. No `HashMap` or `HashSet` in serialized output paths.

**Where**: `lib.rs` (`AtomWithLines::dependencies` is `BTreeSet`), `commands/extract.rs`.

### P4 — Stub structure

External stubs (functions referenced but not defined in the analyzed project) have:
- `code-path`: empty string
- `code-text`: `{"lines-start": 0, "lines-end": 0}`
- `dependencies`: empty set

**Where**: `lib.rs` (`add_external_stubs`).

### P5 — Dependencies sorted

The `dependencies` field is a `BTreeSet<String>`, guaranteeing lexicographic order in the JSON array.

**Where**: `lib.rs` (`AtomWithLines` struct definition).

### P6 — Trailing-dot normalization

Code-names and dependency references have trailing `.` stripped to prevent SCIP artifacts from creating phantom mismatches. The normalization is embedded in `symbol_to_code_name` and `symbol_to_code_name_full` (suffix handling and `strip_suffix('.')`). A standalone `normalize_code_name` helper exists and is tested but is not called in the main pipeline.

**Where**: `lib.rs` (`symbol_to_code_name`, `symbol_to_code_name_full`, `normalize_code_name`).

## SCIP / call graph properties

### P7 — SCIP function kinds

Only four SCIP symbol kinds produce call-graph nodes:

| Kind | Value | Description |
|------|-------|-------------|
| Method | 6 | Instance methods |
| Function | 17 | Free functions |
| Constructor | 26 | Constructors / trait impl methods |
| Macro | 80 | Macro-generated functions |

All other kinds (structs, modules, variables, etc.) are ignored.

**Where**: `constants.rs` (`is_function_like_kind`).

### P8 — Call attribution

Occurrences in each SCIP document are walked in range order. A `current_function_key` is maintained and updated only when a **function-like definition** occurrence is encountered. All subsequent callee references are attributed to that function until the next function definition.

**Where**: `lib.rs` (`process_occurrences`).

### P9 — Disambiguation order

When multiple SCIP symbols share the same base code-name, disambiguation proceeds in priority order:

1. **Definition type context** — nearby type references within 5 lines above the def
2. **`<Type>` embed** — insert the disambiguating type into the probe URI
3. **`@line` fallback** — append the 1-based line number

**Where**: `lib.rs` (`convert_to_atoms_with_lines_internal`), `constants.rs` (`TYPE_CONTEXT_LOOKBACK_LINES`).

## Visibility properties

### P10 — is-public from SCIP

`is-public` is derived from the SCIP `signature_documentation.text` field. A function is `pub` only if its signature starts with `pub` and the next character is NOT `(` (which would indicate restricted visibility like `pub(crate)`).

Charon can override this value when `--with-charon` is used.

**Where**: `lib.rs` (`is_signature_public`).

### P11 — is-public-api three-way

`is-public-api` uses three-way semantics:

| Value | Meaning |
|-------|---------|
| `true` | Confirmed: RQN found in `cargo public-api` output |
| `false` | Not public API: item is not `pub` (non-public visibility) |
| absent (`null`) | Uncertain: item is `pub` but RQN could not be matched against `cargo public-api` output (re-exports, aliases, macro-generated types) |

Trait impl methods (no `pub` in signature) are checked against the public API set before falling back to `is-public`.

**Where**: `public_api.rs` (`enrich_atoms_with_public_api`).

### P12 — Binary crate skip

Public API detection is skipped entirely for crates without a `[lib]` target (binary-only). All atoms get `is-public-api: null`.

**Where**: `public_api.rs` (`is_library_crate`), `commands/extract.rs` (`enrich_with_public_api`).

## Infrastructure properties

### P13 — Path sanitization

Output paths under `.verilib/probes/` are constructed from package name and version. The filename segment must never contain `..` or path separators.

**Where**: `metadata.rs` (`get_default_output_path`, `test_output_path_does_not_escape`).

### P14 — SCIP caching

Generated SCIP data (`index.scip`, `index.scip.json`) is cached in `<project>/data/`. The `--regenerate-scip` flag forces re-generation of both SCIP data and the `cargo public-api` cache (`data/public-api.txt`). Stale cache does not cause incorrect output — it causes stale output.

**Where**: `scip_cache.rs`, `commands/extract.rs` (passes `regenerate_scip` to `public_api::collect_public_api`).

### P15 — Charon non-fatal

`--with-charon` failure (compilation panic, missing tool) produces a warning and falls back to the heuristic `rust-qualified-name` derived from file path + display name. It never aborts the extract pipeline.

**Where**: `commands/extract.rs`, `charon_cache.rs`.

### P16 — Display name enrichment

`enrich_display_name` handles two SCIP symbol formats for impl methods:

| Format | Example | Extraction |
|--------|---------|------------|
| Old | `Type#Trait<&Type>#method().` | Self type from text before first `#` |
| New | `impl#[Type][Trait]method().` | Self type from first `[...]` bracket |

Lifetime prefixes (`&'a`) and backtick quoting are stripped from the extracted type name.

**Where**: `lib.rs` (`enrich_display_name`, `extract_bracket_type`).

---

## Known issues

### C1 — Call after non-function def

When a non-function definition (const, static, type) appears between two function definitions, subsequent callee references may still be attributed to the previous function. The `current_function_key` is only updated on [function-like definitions](glossary.md#function-like-definition). Constrains [P8](#p8--call-attribution).

**Test**: `test_call_after_non_function_def_not_attributed_to_previous_fn`

### C2 — Calls before first function def

Callee references that appear before the first [function-like definition](glossary.md#function-like-definition) in a SCIP document are silently dropped (no caller to attribute them to). Constrains [P8](#p8--call-attribution).

**Test**: `test_calls_before_first_definition_are_dropped`

### C3 — Disambiguation substring false match

[Type context](glossary.md#type-context) [disambiguation](glossary.md#disambiguation) uses substring matching. A type name that is a substring of another type (e.g., `Point` vs `EdwardsPoint`) may cause incorrect disambiguation in edge cases. Constrains [P9](#p9--disambiguation-order).

**Test**: `test_disambiguation_substring_false_match`
