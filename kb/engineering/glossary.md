# Glossary

- **last-updated**: 2026-08-31

Every domain term used in the KB must be defined here. Terms are listed alphabetically.

---

**Atom** — A single function or method in the call graph output. Represented as `AtomWithLines` in Rust. Each atom has a unique [code-name](#code-name) (key), [display name](#display-name), [dependencies](#dependencies), source location, and optional metadata (visibility, qualified name, kind). See [Schema](../../docs/SCHEMA.md).

**Base code-name** — The shared stem of a [code-name](#code-name) before [disambiguation](#disambiguation). When multiple SCIP symbols produce the same base code-name, they are disambiguated via [type context](#type-context), `<Type>` embed, or `@line` fallback. See [P9](properties.md).

**BFS (Breadth-First Search)** — The traversal strategy used by the `callee-crates` command to walk the call graph from a starting function and group callees by crate.

**Blanket impl** — A standard library trait implementation automatically provided for all types (e.g. `Into`, `TryFrom`, `Borrow`). `cargo public-api` output includes blanket impl entries that have no corresponding [atoms](#atom) in the call graph. These are filtered out during `--with-public-api` processing. See [P11](properties.md), `public_api.rs` (`BLANKET_IMPL_TRAITS`).

A crate's *own* blanket impl (`impl<T> Trait for T`) does produce an atom, resolved by [impl-descriptor resolution](#impl-descriptor-resolution), not filtered.

**Binary-only crate** — A Cargo package that has no `[lib]` target (only `[[bin]]` targets). All atoms are marked `is-public-api: false` since binaries have no public API surface. See [P12](properties.md), [library crate](#library-crate).

**Candidate form** — One of the names a [cargo-public-api](#cargo-public-api) entry may be matched under during `--with-public-api` processing; an entry is matched when any of its forms names a real atom. Held per entry in `PublicNameForms`. See [P11](properties.md).

**cargo-public-api** — External tool that lists a crate's public API surface by running `rustdoc` and parsing the output. Invoked via `cargo public-api -sss -p <pkg>`. Requires a nightly Rust toolchain. Used by `--with-public-api` to override [is-public-api](#is-public-api) values via [RQN](#rqn-rust-qualified-name) matching. See [P11](properties.md), [P17](properties.md), `public_api.rs`.

**Call attribution** — The process of assigning [callee references](#callee-reference) to their enclosing function. Done by walking SCIP [occurrences](#occurrence) in lexical order and tracking the current [function-like definition](#function-like-definition). See [P8](properties.md).

**Callee reference** — An [occurrence](#occurrence) that references a called symbol (as opposed to a definition). During [call attribution](#call-attribution), callee references are assigned to the enclosing [function-like definition](#function-like-definition). See [P8](properties.md).

**Charon** — External tool that compiles Rust into LLBC (Low-Level Borrow Calculus) and produces precise qualified names and visibility information. Optional in probe-rust (`--with-charon`). Failure is non-fatal ([P15](properties.md)). Requires a nightly toolchain.

**Code-name** — The unique identifier for an [atom](#atom). A probe URI of the form `probe:<crate>/<version>/<module-path>/<symbol>()`. Used as the JSON object key in the output map. Full grammar and examples: [`docs/SCHEMA.md` § Code-Name Format](../../docs/SCHEMA.md). See [P2](properties.md).

**Code-text** — The source location of a function: `{"lines-start": N, "lines-end": M}` with 1-based inclusive line numbers. [External stubs](#external-stub) use `{0, 0}`. See [P4](properties.md).

**DeclKind** — The declaration kind of an [atom](#atom). Currently only `exec` (executable Rust function). Marked `#[non_exhaustive]` for future extension (e.g., `spec`, `proof`).

**Dependencies** — The set of functions called by an [atom](#atom). Stored as a `BTreeSet<String>` of [code-names](#code-name), guaranteeing sorted output. See [P5](properties.md).

**Directory owner** — A file whose `mod` children resolve as siblings in its own directory: a crate root ([target entry](#target-entry)), a `mod.rs`, or a `#[path]`-mounted file. A conventionally mounted `foo.rs` is not one — its children live under `foo/`. rustc's module-resolution rule, honored by the [mount chain](#mount-chain) walk. See `mod_chain.rs` (`resolve_mod_file`).

**Disambiguation** — The process of making [code-names](#code-name) unique when multiple SCIP symbols share the same [base code-name](#base-code-name). Uses a priority chain: [type context](#type-context) → `<Type>` embed → `@line` fallback. See [P9](properties.md).

**Display name** — The human-readable name shown for an [atom](#atom). For impl methods, enriched to `Type::method` form via `enrich_display_name`. See [P16](properties.md).

**Envelope** — The Schema 3.0 metadata wrapper around the atoms map. Contains `schema`, `schema-version`, `tool`, `source`, `timestamp`, and `data` fields. See [P1](properties.md).

**External stub** — An [atom](#atom) representing a function that is referenced (called) but not defined in the analyzed project. Has empty code-path, `{0,0}` lines, and empty [dependencies](#dependencies). See [P4](properties.md).

**File gate** — The `file-cfg` atom field: the cfg predicate under which a file is mounted at all, derived from the gates on its [mount chains](#mount-chain) (`any(...)` over per-chain `all(...)` conjunctions). Absent when any chain is gate-free or the walk never reached the file. Already folded into the atom's `cfg`. See [P18](properties.md#p18--cfg-predicate-as-complete-as-visible-under-gating-only).

**Foreign declaration** — A function declared inside an `extern { … }` block: a binding whose implementation lives outside Rust, flagged `is-foreign`. Bodyless trait signatures are NOT foreign (see [trait-required](#trait-required)). See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Function-like definition** — An [occurrence](#occurrence) of a [function-like kind](#function-like-kind) with the definition bit set in [symbol roles](#symbol-roles). Updates the `current_function_key` during [call attribution](#call-attribution). See [P8](properties.md).

**Function-like kind** — A SCIP symbol kind that produces a call-graph node: method (6), function (17), constructor (26), macro (80). All other kinds are ignored. See [P7](properties.md).

**FunctionNode** — Internal Rust type representing a node in the call graph. Contains symbol, display name, signature text, callees (`HashSet<CalleeInfo>`), source range, and type context. Not serialized directly; converted to [AtomWithLines](#atom) for output.

**Entries matched (vs atoms marked)** — The two distinct `--with-public-api` metrics: *entries matched* counts [cargo-public-api](#cargo-public-api) entries backed by some atom (`public-api entries matched: N/M`); *atoms marked* counts atoms whose `is-public-api` is `true` (`is-public-api: N true, M false`). Several entries can share one atom, so they move independently. See [P11](properties.md).

**Impl-descriptor resolution** — The last `--with-public-api` pass: for an entry no atom *name* matched, the atom that implements it is found through the SCIP impl descriptor embedded in the atom key (`…impl#[SelfType][Trait]method()`) and marked `is-public-api: true` directly. This is what reaches macro-generated impl-evidence atoms, which have no [RQN](#rqn-rust-qualified-name) to match on. See [P11](properties.md), `public_api.rs` (`resolve_unmatched_to_atoms`).

**Inherited default trait method** — A trait method with a default body that an implementing type does not override. The body exists once, in the trait; SCIP creates no per-type symbol, so `cargo public-api`'s per-type entry is matched to the trait's atom by the trait-default pass. See [P11](properties.md).

**is-public** — Boolean field on [atoms](#atom) indicating whether the function's SCIP signature starts with an unrestricted `pub` prefix. Derived from `signature_documentation.text`. Does not indicate public API membership. See [P10](properties.md).

**is-public-api** — Boolean field indicating whether a function is reachable from the crate root. By default, derived from SCIP module-chain visibility walk: `true` = direct `pub` function with all ancestor modules `pub`, or trait impl method whose implementing type is in a public module chain; `false` = non-public function or non-public ancestor module. Absent/`null` for [external stubs](#external-stub) and for analyzed-crate atoms with no SCIP definition occurrence (bodyless stubs, unresolved macro-generated impl evidence). When `--with-public-api` is used, overridden for atoms with a [RQN](#rqn-rust-qualified-name) by matching against [cargo-public-api](#cargo-public-api) output, and set on an RQN-less atom that [impl-descriptor resolution](#impl-descriptor-resolution) proves public. See [P11](properties.md), [P17](properties.md).

**Module visibility map** — A `HashMap<String, bool>` built from SCIP module symbols (kind 29) during `build_call_graph`. Maps module path descriptors (e.g. `"edwards/"`) to whether the module is unrestricted `pub`. Used by `classify_public_api` to walk ancestor module chains. See [P11](properties.md).

**Library crate** — A Cargo package that has a `[lib]` target. Functions in library crates can be public API; [binary-only crates](#binary-only-crate) always have `is-public-api: false`. See [P12](properties.md).

**Match key** — A normalized string used to correlate [Charon](#charon) LLBC function entries with SCIP-derived [atoms](#atom). Built as `module::bare_function_name`. From the atom side: module derived from `code_path` or `code_module`, bare function name from `display_name`. From the Charon side: strip the first `::` segment (always the crate name, which may differ from `translated.crate_name` for dependency crates included via `--include`) and remove `{...}` impl blocks. When multiple Charon candidates share the same match key, [span disambiguation](#span-disambiguation) selects the best one. See `charon_names.rs`.

**Mount chain** — The sequence of `mod` declarations (with their `#[cfg]` gates) through which a file is reached from a [target entry](#target-entry), including a file-level `#![cfg]`. One file can have several chains (mutually exclusive `#[path]` remounts, mounts from different targets or packages). Walked by `mod_chain.rs`.

**Multi-crate LLBC** — A [Charon](#charon) LLBC file that contains functions from more than one crate. Produced when Charon is invoked with `--include` to pull in dependency crate functions alongside the target crate. The LLBC's `translated.crate_name` reflects only the target crate; included dependency functions have qualified names prefixed by their own crate name. [Match key](#match-key) construction handles this by stripping the first path segment unconditionally rather than matching against `crate_name`.

**Occurrence** — A SCIP data element representing a reference to or definition of a symbol at a specific source location. Has `range`, `symbol`, and `symbol_roles` fields.

**Probe URI** — See [Code-name](#code-name).

**Provably complete walk** — A module-tree walk during which no valve tripped: every file read and parsed, every `mod` target resolved, no `include!`, no mod-mentioning unrecognized macro invocation, no mount cycle, no chain-cap overflow. Only such a walk (across ALL packages) may infer [unmounted](#unmounted) files. See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Ralph Loop** — The development quality loop: implement, audit (three auditor skills), fix, repeat until clean, then run tests. See [kb/index.md](../index.md).

**RQN (Rust Qualified Name)** — The `rust-qualified-name` field on [atoms](#atom). A `::` separated path like `crate_name::module::{Type}::method`. Derived heuristically from file path + [display name](#display-name), or precisely from [Charon](#charon) LLBC when `--with-charon` is used. The heuristic form uses bare `Type::method`; the Charon form includes `{Type}::method` with nested impl segments and may have the crate prefix from any crate in a [multi-crate LLBC](#multi-crate-llbc). Matching between Charon RQNs and atoms uses a normalized [match key](#match-key), not raw string equality.

**Span disambiguation** — When multiple [Charon](#charon) candidates share the same [match key](#match-key), the candidate whose LLBC span best overlaps the [atom's](#atom) source line range is selected. For multi-line Charon spans, overlap is `min(atom_end, c_end) - max(atom_start, c_start)`. For single-line spans (`line_start == line_end`, common for dependency crates in [multi-crate LLBCs](#multi-crate-llbc)), a containment check is used instead: the span must fall within `[atom_start, atom_end]`. See `charon_names.rs` (`disambiguate_by_span`).

**SCIP document** — A document entry in the [SCIP](#scip-source-code-intelligence-protocol) index, corresponding to a single source file. Contains [occurrences](#occurrence) and [symbol](#scip-source-code-intelligence-protocol) definitions. Each document has its own occurrence stream walked during [call attribution](#call-attribution).

**SCIP (Source Code Intelligence Protocol)** — The intermediate representation generated by rust-analyzer. A binary format (`index.scip`) converted to JSON (`index.scip.json`) by the `scip` CLI tool. Contains documents, symbol definitions, [occurrences](#occurrence), and metadata.

**ScipIndex** — Internal Rust type representing the parsed SCIP JSON structure. Contains a list of documents, each with [occurrences](#occurrence) and symbol information. Entry point for the call graph pipeline.

**Symbol roles** — A bitmask on SCIP [occurrences](#occurrence). Bit 0 (`& 1`) indicates a definition (as opposed to a reference). Used to identify [function-like definitions](#function-like-definition) vs callee references.

**syn** — Rust parser library used to parse source files for function body spans. SCIP only provides function name locations; syn finds the actual end line of function bodies. See [architecture](architecture.md).

**Target entry** — A package's lib or bin crate-root file: `[lib] path`/`src/lib.rs`, `[[bin]] path`, `src/main.rs`, `src/bin/*.rs`, `src/bin/*/main.rs`. The roots of the [mount chain](#mount-chain) walk. Test/bench/example targets are deliberately excluded. See `mod_chain.rs` (`package_entries`).

**Trait-required** — The `trait-required` atom field: a bodyless trait method signature, whose proof obligations live on the implementations. Trait methods with a default body are ordinary functions. See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Type context** — Nearby type references (within 5 lines above a definition) used for [disambiguation](#disambiguation) when multiple SCIP symbols share the same [base code-name](#base-code-name). See [P9](properties.md).

**.verilib** — The output directory structure: `.verilib/probes/` holds extracted [atom](#atom) files. Convention shared across the probe ecosystem.


**Unmounted** — The `is-unmounted` atom field: the file is reached by no [mount chain](#mount-chain) from any analyzed package's [target entries](#target-entry), so it is not part of any lib or bin build. Inferred only from a [provably complete walk](#provably-complete-walk). See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).