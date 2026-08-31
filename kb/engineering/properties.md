# Properties and Invariants

- **last-updated**: 2026-08-28

Every property here must hold in the implementation. If a property is violated, it is a bug in the code, not in the KB — unless a deliberate decision changes the KB first.

## Output properties

### P1 — Envelope

The `extract` command wraps output in a Schema 3.0 envelope with `schema: "probe-rust/extract"`. The `schema-version` field must match the version in `docs/SCHEMA.md`. Currently `"3.0"`.

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

### P11 — is-public-api from SCIP module walk

**Default (no flags):** `is-public-api` is binary (`true` / `false`) for all internal atoms. Derived from SCIP data at call-graph build time — no external tools required.

| Value | Meaning |
|-------|---------|
| `true` | Function is reachable from the crate root: either a direct `pub` function with all ancestor modules `pub`, or a trait impl method whose implementing type is in a public module chain |
| `false` | Not public API: function is non-public, or at least one ancestor module is non-public |
| absent (`null`) | External stubs only (no code-path to analyze) |

**Optional override (`--with-public-api`):** When the flag is set, `is-public-api` is overridden for all atoms that have a `rust-qualified-name` (RQN). The RQN is looked up in the set of qualified names parsed from `cargo public-api -sss` output. On failure (missing tools, nightly, or `cargo public-api` error), the override is skipped and SCIP-walk values are preserved (non-fatal, see [P17](#p17--public-api-override-non-fatal)).

`cargo public-api` names items by their public *use* path; atom RQNs use the *definition* path, and some public functions have no atom carrying their name at all. Reconciliation therefore runs in passes over a per-entry candidate-form set (`PublicNameForms`: entry → the names it may match under, always including itself). Every pass is additive — an entry can gain candidate forms but never lose one — no pass resolves an ambiguity (zero or several candidates never resolve, and a `pub use` alias claimed for two definitions resolves to neither), and the resolution passes (2 and 4) act only on entries still unmatched:

1. **pub-use**: rewrite an entry through the crate-root `pub use` alias map to its definition form.
2. **trait-default**: an entry `path::Type::method` gains the RQN of the trait atom providing `method` when the atom keys show impl evidence for exactly one trait providing it, no atom defines `Type::method` itself, exactly one atom answers to `Trait::method`, and that atom is itself public API. The inherited default body is what a public caller invokes, so it is the entry's only implementation. Impl evidence is read only from the analyzed crate's own atom keys — a dependency's impls neither prove the `Type: Trait` link nor veto with a phantom override. The `Trait::method` atom lookup by RQN needs no such scoping, because the atom-is-itself-public-API guard already rejects any RQN absent from this crate's `cargo public-api` surface.
3. **name matching**: `enrich_atoms_with_public_api` sets `true`/`false` for every atom with an RQN, from the flattened candidate forms.
4. **impl-descriptor resolution**: for an entry that *no* name matched, the atom implementing it is marked `true` directly — resolved through the SCIP impl descriptor embedded in the atom key, restricted to the analyzed crate's own atoms. A qualified entry's module segments (between crate and type/trait) must equal the atom key's module path, so a lone `bar::Table::new` atom never answers for a public `foo::Table::new`; bare `T::method` entries carry no module path and rely on the blanket verification instead. `path::Type::method` resolves to the crate's single `impl#[Type][…]method()` atom; a bare `T::method` (printed by `cargo public-api` for a blanket impl) and a trait-level `path::Trait::method` resolve to the single `impl#[…][Trait]method()` atom only when `syn` verifies the impl self type is one of the impl's own generic parameters (`impl<T> Trait for T`) — the key alone cannot tell a generic parameter named `T` from a concrete type named `T`.

Pass 4 is the one exception to "atoms without an RQN are unaffected": a macro-generated impl leaves a bodyless impl-evidence atom with no RQN, and a `cargo public-api` entry resolving uniquely to it proves it public. Atoms of other crates are never marked from this crate's public API.

Extract prints both metrics, which are different numbers: `is-public-api: N true, M false` counts *atoms*, and `public-api entries matched: N/M` counts *entries* (several entries can share one atom, and an entry can have several candidate forms). This is the canonical description of the pass sequence; `docs/PUBLIC_API_LIMITATIONS.md` catalogues what remains unmatched on the reference crate (`curve25519-dalek` 4.1.3) and why.

**Where**: `lib.rs` (`build_module_visibility_map`, `classify_public_api`, `is_module_chain_public`, `is_trait_impl_symbol`), `public_api.rs` (`PublicNameForms`, `enrich_atoms_with_public_api`, `atom_candidate_names`, `parse_impl_key`, `is_blanket_impl_atom`), `commands/extract.rs` (`enrich_with_public_api`).

### P12 — Binary crate detection

For crates without a `[lib]` target (binary-only), all atoms are marked `is-public-api: false` since binaries have no public API surface.

**Where**: `lib.rs` (`is_library_crate`, `classify_public_api`).

## Infrastructure properties

### P13 — Path sanitization

Output paths under `.verilib/probes/` are constructed from package name and version. The filename segment must never contain `..` or path separators.

**Where**: `metadata.rs` (`get_default_output_path`, `test_output_path_does_not_escape`).

### P14 — SCIP caching with staleness check

Generated SCIP data (`index.scip`, `index.scip.json`) is cached in `<project>/data/`. The cache is reused only when it is **fresh**: no `*.rs` file, `Cargo.toml`, or directory under the project (outside `target/`, `.git/`, `.verilib/`, and the cache dir itself) is newer than the cached JSON. Directories are included because a file deletion or rename bumps only its parent directory's mtime. A stale cache is regenerated automatically; the `--regenerate-scip` flag forces re-generation regardless.

A stale index is not merely stale output: its line numbers no longer match the fresh `syn` parse of the sources, which silently breaks the span-map join (`cfg` and `lines-end` are dropped, see P18). This is why staleness forces regeneration rather than a warning. Any span-map misses that remain after a fresh index are reported with a warning, never silently.

When `--with-public-api` is used, the `cargo public-api` output is also cached in `<project>/data/public-api.txt`. The `--regenerate-scip` flag also forces regeneration of this cache (no automatic staleness check).

**Where**: `scip_cache.rs` (`is_cache_stale`, `newest_source_mtime`), `lib.rs` (span-miss warning in `convert_to_atoms_with_lines_internal`), `public_api.rs` (`collect_public_api`, `PUBLIC_API_CACHE_FILE`), `commands/extract.rs`.

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

### P17 — Public-API override non-fatal

`--with-public-api` failure (missing nightly toolchain, missing `cargo-public-api`, or `cargo public-api` execution error) produces a warning and preserves the SCIP-walk-derived `is-public-api` values. It never aborts the extract pipeline. Analogous to [P15](#p15--charon-non-fatal) for Charon.

**Where**: `commands/extract.rs` (`enrich_with_public_api`), `public_api.rs` (`ensure_nightly_toolchain`, `ensure_cargo_public_api`, `collect_public_api`).

## Source-fact properties

### P18 — `cfg` predicate: as complete as visible, under-gating only

The `cfg` field is the item-gating predicate for a function, as completely as static analysis can see it: its own `#[cfg]`, a file-level `#![cfg(...)]`, every enclosing same-file `impl`/`mod`/`trait`/`extern` gate, and the gates on the `mod` declaration chain mounting its file from its package's lib/bin target entries, `all(...)`-joined. The parent-file component is also emitted alone as `file-cfg` (provenance for consumers reporting *why* a function is gated). probe-rust never evaluates these predicates — it reports configuration-independent facts; consumers (probe-aeneas) evaluate them against their build's feature set.

The permitted error direction is **under-gating only**: the predicate may claim a function is compiled when the build would exclude it, never the reverse. Known deliberate under-gating: `cfg_if!` branch predicates are dropped (the branch items are still walked), `#[cfg_attr(cond, cfg(...))]` is ignored, files the module-tree walk cannot reach contribute no chain component, and a walk that was not provably complete contributes no chain components at all (see P19 — an invisible ungated mount would otherwise let a chain gate over-state). A file-level `#![cfg(...)]` reaches its own functions exactly once (through the same-file component) and its mounted descendants through the chain component. A span-map miss loses the same-file component entirely — that case is warned, never silent (P14); an ambiguous span match (several same-name spans containing the line) is refused and counted as a miss rather than guessed.

A file mounted through several chains (e.g. mutually exclusive `#[path]` remounts, or mounts from different targets/workspace members) gets `any(...)` over the per-chain `all(...)` conjunctions (sorted, deduplicated); a file with at least one gate-free chain has no file gate. `#[cfg_attr(...)]` never contributes (it is not a scope gate). Predicate strings are not whitespace-normalized (source-copied parts keep syn token spacing; synthesized combinators are tight) — consumers must parse whitespace-insensitively.

**Where**: `rust_parser.rs` (`FunctionSpanVisitor`, `cfg_predicates_of`, `parse_file_for_spans` file-inner attrs), `mod_chain.rs` (`analyze`, `chains_predicate`), `lib.rs` (fold in `convert_to_atoms_with_lines_internal`).

### P19 — Conservative unmounted/foreign/trait facts

`is-unmounted`, `is-foreign`, and `trait-required` are configuration-independent declaration facts, emitted only when `true`:

- `is-unmounted`: the file is reached by no `mod` chain from **any analyzed package's lib or bin target entries** (`[lib]`/`src/lib.rs`, `[[bin]]`/`src/main.rs`/`src/bin/*`) — it is not part of any lib or bin build. Test/bench/example targets are deliberately not scanned: a file mounted only from them is still flagged, which is the policy-aligned outcome (those targets are outside the verified build) and exactly what the field claims — no more. Inferred **only from a provably complete walk across every package**: any unparsable file, unresolvable `mod` target, `include!`, `cfg_attr` on a `mod` declaration (it can inject `path` and select a different file per configuration), any item / `macro_rules!` body / unrecognized macro invocation whose tokens mention `mod` (block-local `#[path] mod`, macro-generated mounts), unreadable or entry-less-with-src package manifest, mount cycle, or chain-cap overflow anywhere disables the inference project-wide (a `#[path]` mount may cross package boundaries). **The same completeness requirement gates `file-cfg` emission** — an invisible mount could be ungated, making any gate emitted beside it over-state. The error direction is fixed: lib/bin-compiled code must never be labeled unmounted or over-gated; dead code may be missed.
- `is-foreign`: declared inside an `extern { … }` block (no Rust body). Judged on the AST block, so bodyless trait signatures are NOT foreign.
- `trait-required`: a bodyless trait method signature. Trait methods with a default body are ordinary functions.

**Where**: `mod_chain.rs` (walk valves, `analyze`), `rust_parser.rs` (`visit_foreign_item_fn`, `visit_trait_item_fn`), `lib.rs` (emission).

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
