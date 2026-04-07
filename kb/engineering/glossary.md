# Glossary

- **last-updated**: 2026-04-07

Every domain term used in the KB must be defined here. Terms are listed alphabetically.

---

**Atom** — A single function or method in the call graph output. Represented as `AtomWithLines` in Rust. Each atom has a unique [code-name](#code-name) (key), [display name](#display-name), [dependencies](#dependencies), source location, and optional metadata (visibility, qualified name, kind). See [Schema](../../docs/SCHEMA.md).

**Base code-name** — The shared stem of a [code-name](#code-name) before [disambiguation](#disambiguation). When multiple SCIP symbols produce the same base code-name, they are disambiguated via [type context](#type-context), `<Type>` embed, or `@line` fallback. See [P9](properties.md).

**BFS (Breadth-First Search)** — The traversal strategy used by the `callee-crates` command to walk the call graph from a starting function and group callees by crate.

**Binary-only crate** — A Cargo package that has no `[lib]` target (only `[[bin]]` targets). Public API detection is skipped for binary-only crates since `cargo public-api` only works on libraries. See [P12](properties.md), [library crate](#library-crate).

**Call attribution** — The process of assigning callee references to their enclosing function. Done by walking SCIP [occurrences](#occurrence) in lexical order and tracking the current [function-like definition](#function-like-definition). See [P8](properties.md).

**Cargo public-api** — External tool (`cargo-public-api`) that reports the public API surface of a Rust [library crate](#library-crate) by analyzing rustdoc JSON. Requires a nightly toolchain.

**Charon** — External tool that compiles Rust into LLBC (Low-Level Borrow Calculus) and produces precise qualified names and visibility information. Optional in probe-rust (`--with-charon`). Failure is non-fatal ([P15](properties.md)). Requires a nightly toolchain.

**Code-name** — The unique identifier for an [atom](#atom). A probe URI of the form `probe:<crate>/<version>/<module-path>/<symbol>()`. Used as the JSON object key in the output map. Full grammar and examples: [`docs/SCHEMA.md` § Code-Name Format](../../docs/SCHEMA.md). See [P2](properties.md).

**Code-text** — The source location of a function: `{"lines-start": N, "lines-end": M}` with 1-based inclusive line numbers. [External stubs](#external-stub) use `{0, 0}`. See [P4](properties.md).

**DeclKind** — The declaration kind of an [atom](#atom). Currently only `exec` (executable Rust function). Marked `#[non_exhaustive]` for future extension (e.g., `spec`, `proof`).

**Dependencies** — The set of functions called by an [atom](#atom). Stored as a `BTreeSet<String>` of [code-names](#code-name), guaranteeing sorted output. See [P5](properties.md).

**Disambiguation** — The process of making [code-names](#code-name) unique when multiple SCIP symbols share the same [base code-name](#base-code-name). Uses a priority chain: [type context](#type-context) → `<Type>` embed → `@line` fallback. See [P9](properties.md).

**Display name** — The human-readable name shown for an [atom](#atom). For impl methods, enriched to `Type::method` form via `enrich_display_name`. See [P16](properties.md).

**Envelope** — The Schema 2.0 metadata wrapper around the atoms map. Contains `schema`, `schema-version`, `tool`, `source`, `timestamp`, and `data` fields. See [P1](properties.md).

**External stub** — An [atom](#atom) representing a function that is referenced (called) but not defined in the analyzed project. Has empty code-path, `{0,0}` lines, and empty [dependencies](#dependencies). See [P4](properties.md).

**Function-like definition** — An [occurrence](#occurrence) of a [function-like kind](#function-like-kind) with the definition bit set in [symbol roles](#symbol-roles). Updates the `current_function_key` during [call attribution](#call-attribution). See [P8](properties.md).

**Function-like kind** — A SCIP symbol kind that produces a call-graph node: method (6), function (17), constructor (26), macro (80). All other kinds are ignored. See [P7](properties.md).

**FunctionNode** — Internal Rust type representing a node in the call graph. Contains symbol, display name, signature text, callees (`HashSet<CalleeInfo>`), source range, and type context. Not serialized directly; converted to [AtomWithLines](#atom) for output.

**is-public** — Boolean field on [atoms](#atom) indicating whether the function's SCIP signature starts with an unrestricted `pub` prefix. Derived from `signature_documentation.text`. Does not indicate public API membership. See [P10](properties.md).

**is-public-api** — Optional boolean field with three-way semantics: `true` = confirmed in public API ([RQN](#rqn-rust-qualified-name) matched in `cargo public-api` output), `false` = not public API (item has non-public visibility), absent/`null` = uncertain (`pub` item whose RQN could not be matched due to re-exports, aliases, or macro-generated types). See [P11](properties.md).

**Library crate** — A Cargo package that has a `[lib]` target. Required for [cargo public-api](#cargo-public-api) to function. See [P12](properties.md), [binary-only crate](#binary-only-crate).

**Occurrence** — A SCIP data element representing a reference to or definition of a symbol at a specific source location. Has `range`, `symbol`, and `symbol_roles` fields.

**Probe URI** — See [Code-name](#code-name).

**Ralph Loop** — The development quality loop: implement, audit (three auditor skills), fix, repeat until clean, then run tests. See [kb/index.md](../index.md).

**RQN (Rust Qualified Name)** — The `rust-qualified-name` field on [atoms](#atom). A `::` separated path like `crate_name::module::Type::method`. Derived heuristically from file path + [display name](#display-name), or precisely from [Charon](#charon) LLBC when `--with-charon` is used.

**SCIP (Source Code Intelligence Protocol)** — The intermediate representation generated by rust-analyzer. A binary format (`index.scip`) converted to JSON (`index.scip.json`) by the `scip` CLI tool. Contains documents, symbol definitions, [occurrences](#occurrence), and metadata.

**ScipIndex** — Internal Rust type representing the parsed SCIP JSON structure. Contains a list of documents, each with [occurrences](#occurrence) and symbol information. Entry point for the call graph pipeline.

**Symbol roles** — A bitmask on SCIP [occurrences](#occurrence). Bit 0 (`& 1`) indicates a definition (as opposed to a reference). Used to identify [function-like definitions](#function-like-definition) vs callee references.

**syn** — Rust parser library used to parse source files for function body spans. SCIP only provides function name locations; syn finds the actual end line of function bodies. See [architecture](architecture.md).

**Type context** — Nearby type references (within 5 lines above a definition) used for [disambiguation](#disambiguation) when multiple SCIP symbols share the same [base code-name](#base-code-name). See [P9](properties.md).

**.verilib** — The output directory structure: `.verilib/probes/` holds extracted [atom](#atom) files. Convention shared across the probe ecosystem.
