# Glossary

- **last-updated**: 2026-09-01

Every domain term used in the KB must be defined here. Terms are listed alphabetically.

---

**Atom** — A single function or method in the call graph output. Represented as `AtomWithLines` in Rust. Each atom has a unique [code-name](#code-name) (key), [display name](#display-name), [dependencies](#dependencies), source location, and optional metadata (visibility, qualified name, kind). See [Schema](../../docs/SCHEMA.md).

**Base code-name** — The shared stem of a [code-name](#code-name) before [disambiguation](#disambiguation). When multiple SCIP symbols produce the same base code-name, they are disambiguated via [type context](#type-context), `<Type>` embed, or `@line` fallback. See [P9](properties.md).

**BFS (Breadth-First Search)** — The traversal strategy used by the `callee-crates` command to walk the call graph from a starting function and group callees by crate.

**Blanket impl** — A standard library trait implementation automatically provided for all types (e.g. `Into`, `TryFrom`, `Borrow`). `cargo public-api` output includes blanket impl entries that have no corresponding [atoms](#atom) in the call graph. These are filtered out during `--with-public-api` processing. See [P11](properties.md), `public_api.rs` (`BLANKET_IMPL_TRAITS`).

A crate's *own* blanket impl (`impl<T> Trait for T`) does produce an atom, resolved by [impl-descriptor resolution](#impl-descriptor-resolution), not filtered.

**Binary-only crate** — A Cargo package that has no `[lib]` target (only `[[bin]]` targets). Gets [is-public-api](#is-public-api) like any crate (`null` without `--with-public-api`); `is_library_crate` only labels the extract summary. See [P12](properties.md) (retired), [library crate](#library-crate).

**Candidate form** — One of the names a [cargo-public-api](#cargo-public-api) entry may be matched under during `--with-public-api` processing; an entry is matched when any of its forms names a real atom. Held per entry in `PublicNameForms`. See [P11](properties.md).

**cargo-public-api** — External tool that lists a crate's public API surface by running `rustdoc` and parsing the output. Invoked via `cargo public-api -sss -p <pkg>`. Requires a nightly Rust toolchain. Used by `--with-public-api` to override [is-public-api](#is-public-api) values via [RQN](#rqn-rust-qualified-name) matching. See [P11](properties.md), [P17](properties.md), `public_api.rs`.

**Call attribution** — The process of assigning [callee references](#callee-reference) to their enclosing function. Done by walking SCIP [occurrences](#occurrence) in lexical order and tracking the current [function-like definition](#function-like-definition). See [P8](properties.md).

**Callee reference** — An [occurrence](#occurrence) that references a called symbol (as opposed to a definition). During [call attribution](#call-attribution), callee references are assigned to the enclosing [function-like definition](#function-like-definition). See [P8](properties.md).

**Candidate resolution** — The process of matching one [atom](#atom) against the Charon entries sharing its [match key](#match-key). Multi-candidate keys are narrowed by successive filters: same normalized file → [self-type](#self-type) match → dedup on `(def_id, qualified_name)` → [span disambiguation](#span-disambiguation). Three outcomes: **Match** (enrich; the LLBC source overrides the RQN, the [Manifest source](#manifest-enrichment-source) stamps only [charon-def-id](#charon-def-id)), **Ambiguous** (distinct same-file candidates no signal can split: the atom's [RQN](#rqn-rust-qualified-name) is cleared and nothing is stamped), **NoMatch** (no candidate confirmable to the atom's file, or every usable span excludes it: the heuristic RQN is kept). Never an arbitrary pick. See [P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily), `charon_names.rs` (`resolve_charon_candidate`).

**Charon** — External tool that compiles Rust into LLBC (Low-Level Borrow Calculus) and produces precise qualified names and visibility information. Optional in probe-rust (`--with-charon`). Failure is non-fatal ([P15](properties.md)). Requires a nightly toolchain.

**charon-def-id** — The `charon-def-id` atom field: Charon's `FunDeclId` for the matched function, the integer key probe-aeneas joins on against Aeneas's `translation.json`. Always emitted together with `charon-version` (the coupling invariant — a def-id is meaningful only relative to the Charon run that produced it) or not at all. Join contract and field spec: [`docs/SCHEMA.md`](../../docs/SCHEMA.md). See [P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily).

**Code-name** — The unique identifier for an [atom](#atom). A probe URI of the form `probe:<crate>/<version>/<module-path>/<symbol>()`. Used as the JSON object key in the output map. Full grammar and examples: [`docs/SCHEMA.md` § Code-Name Format](../../docs/SCHEMA.md). See [P2](properties.md).

**Code-text** — The source location of a function: `{"lines-start": N, "lines-end": M}` with 1-based inclusive line numbers. [External stubs](#external-stub) use `{0, 0}`. See [P4](properties.md).

**DeclKind** — The declaration kind of an [atom](#atom). Currently only `exec` (executable Rust function). Marked `#[non_exhaustive]` for future extension (e.g., `spec`, `proof`).

**Dependencies** — The set of functions called by an [atom](#atom). Stored as a `BTreeSet<String>` of [code-names](#code-name), guaranteeing sorted output. See [P5](properties.md).

**Directory owner** — A file whose `mod` children resolve as siblings in its own directory: a crate root ([target entry](#target-entry)), a `mod.rs`, or a `#[path]`-mounted file. A conventionally mounted `foo.rs` is not one — its children live under `foo/`. rustc's module-resolution rule, honored by the [mount chain](#mount-chain) walk. See `mod_chain.rs` (`resolve_mod_file`).

**Disambiguation** — The process of making [code-names](#code-name) unique when multiple SCIP symbols share the same [base code-name](#base-code-name). Uses a priority chain: [type context](#type-context) → `<Type>` embed → `@line` fallback. See [P9](properties.md).

**Display name** — The human-readable name shown for an [atom](#atom). For impl methods, enriched to `Type::method` form via `enrich_display_name`. See [P16](properties.md).

**Entries matched (vs atoms marked)** — The two distinct `--with-public-api` metrics: *entries matched* counts [cargo-public-api](#cargo-public-api) entries backed by some atom (`public-api entries matched: N/M`); *atoms marked* counts atoms whose `is-public-api` is `true` (`is-public-api: N true, M false`). Several entries can share one atom, so they move independently. See [P11](properties.md).

**Envelope** — The Schema 3.0 metadata wrapper around the atoms map. Contains `schema`, `schema-version`, `tool`, `source`, `timestamp`, and `data` fields. See [P1](properties.md).

**External stub** — An [atom](#atom) representing a function that is referenced (called) but not defined in the analyzed project. Has empty code-path, `{0,0}` lines, and empty [dependencies](#dependencies). See [P4](properties.md).

**File gate** — The `file-cfg` atom field: the cfg predicate under which a file is mounted at all, derived from the gates on its [mount chains](#mount-chain) (`any(...)` over per-chain `all(...)` conjunctions). Absent when any chain is gate-free or the walk never reached the file. Already folded into the atom's `cfg`. See [P18](properties.md#p18--cfg-predicate-as-complete-as-visible-under-gating-only).

**Foreign declaration** — A function declared inside an `extern { … }` block: a binding whose implementation lives outside Rust, flagged `is-foreign`. Bodyless trait signatures are NOT foreign (see [trait-required](#trait-required)). See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Function-like definition** — An [occurrence](#occurrence) of a [function-like kind](#function-like-kind) with the definition bit set in [symbol roles](#symbol-roles). Updates the `current_function_key` during [call attribution](#call-attribution). See [P8](properties.md).

**Function-like kind** — A SCIP symbol kind that produces a call-graph node: method (6), function (17), constructor (26), macro (80). All other kinds are ignored. See [P7](properties.md).

**FunctionNode** — Internal Rust type representing a node in the call graph. Contains symbol, display name, signature text, callees (`HashSet<CalleeInfo>`), source range, and type context. Not serialized directly; converted to [AtomWithLines](#atom) for output.

**Impl-descriptor resolution** — The last `--with-public-api` pass: for an entry no atom *name* matched, the atom that implements it is found through the SCIP impl descriptor embedded in the atom key (`…impl#[SelfType][Trait]method()`) and marked `is-public-api: true` directly. This is what reaches macro-generated impl-evidence atoms, which have no [RQN](#rqn-rust-qualified-name) to match on. See [P11](properties.md), `public_api.rs` (`resolve_unmatched_to_atoms`).

**Inherited default trait method** — A trait method with a default body that an implementing type does not override. The body exists once, in the trait; SCIP creates no per-type symbol, so `cargo public-api`'s per-type entry is matched to the trait's atom by the trait-default pass. See [P11](properties.md).

**is-public** — Boolean field on [atoms](#atom) indicating whether the function's SCIP signature starts with an unrestricted `pub` prefix. Derived from `signature_documentation.text`. Does not indicate public API membership. See [P10](properties.md).

**is-public-api** — Boolean field set **only** by `--with-public-api`, by matching atoms against [cargo-public-api](#cargo-public-api) output: `true` when a public entry proves the atom public (RQN match after the candidate-form passes, or [impl-descriptor resolution](#impl-descriptor-resolution)); `false` when the atom has an [RQN](#rqn-rust-qualified-name) that no public entry matches. Absent/`null` without the flag (for every atom), and with it for [external stubs](#external-stub) and analyzed-crate atoms with no SCIP definition occurrence (bodyless stubs, unresolved macro-generated impl evidence). Not derived from any SCIP walk — that mechanism was removed (`43cdc96`, deleted 0.11.0); contrast [is-public](#is-public). See [P11](properties.md), [P17](properties.md).

**Library crate** — A Cargo package that has a `[lib]` target. Functions in library crates can be public API; [binary-only crates](#binary-only-crate) always have `is-public-api: false`. See [P12](properties.md).

**Manifest (enrichment source)** — The `--translation <translation.json>` enrichment path: Charon already ran once inside Aeneas, so probe-rust reads the Aeneas manifest's `functions[]` (`def_id`, `rust_name`, source span) instead of running Charon again. Stamps only [charon-def-id](#charon-def-id)/`charon-version` — it never overrides the atom's heuristic [RQN](#rqn-rust-qualified-name). Fails closed: a lone candidate without file proof is rejected rather than stamped ([P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily)). The alternative source is the legacy Charon LLBC (`--with-charon`), which does override RQNs. See `charon_names.rs` (`EnrichmentSource`, `Enrichment::from_translation_json`).

**Match key** — A normalized string used to correlate [Charon](#charon) LLBC function entries with SCIP-derived [atoms](#atom). Built as `module::bare_function_name`. From the atom side: module derived from `code_path` or `code_module`, bare function name from `display_name`. From the Charon side: strip the first `::` segment (always the crate name, which may differ from `translated.crate_name` for dependency crates included via `--include`) and remove `{...}` impl blocks. When multiple Charon candidates share the same match key, [candidate resolution](#candidate-resolution) narrows them by file, self type, dedup, and span — never an arbitrary pick ([P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily)). See `charon_names.rs`.

**Mount chain** — The sequence of `mod` declarations (with their `#[cfg]` gates) through which a file is reached from a [target entry](#target-entry), including a file-level `#![cfg]`. One file can have several chains (mutually exclusive `#[path]` remounts, mounts from different targets or packages). Walked by `mod_chain.rs`.

**Multi-crate LLBC** — A [Charon](#charon) LLBC file that contains functions from more than one crate. Produced when Charon is invoked with `--include` to pull in dependency crate functions alongside the target crate. The LLBC's `translated.crate_name` reflects only the target crate; included dependency functions have qualified names prefixed by their own crate name. [Match key](#match-key) construction handles this by stripping the first path segment unconditionally rather than matching against `crate_name`.

**Occurrence** — A SCIP data element representing a reference to or definition of a symbol at a specific source location. Has `range`, `symbol`, and `symbol_roles` fields.

**Probe URI** — See [Code-name](#code-name).

**probe-aeneas** — Sibling probe-ecosystem tool that links Rust atoms to their Aeneas-generated Lean translations. Consumes probe-rust output: matches primarily on [RQN](#rqn-rust-qualified-name) (legacy) or joins on [charon-def-id](#charon-def-id) (manifest flow), and evaluates `cfg` predicates against its build's feature set ([P18](properties.md#p18--cfg-predicate-as-complete-as-visible-under-gating-only)). Its matching behavior motivates [P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily)'s clear-on-ambiguity rule.

**Provably complete walk** — A module-tree walk during which no valve tripped: every file read and parsed, every `mod` target resolved, no `include!`, no mod-mentioning unrecognized macro invocation, no mount cycle, no chain-cap overflow. Only such a walk (across ALL packages) may infer [unmounted](#unmounted) files. See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Ralph Loop** — The development quality loop: implement, audit (three auditor skills), fix, repeat until clean, then run tests. See [kb/index.md](../index.md).

**RQN (Rust Qualified Name)** — The `rust-qualified-name` field on [atoms](#atom). A `::` separated path like `crate_name::module::{Type}::method`. Derived heuristically from file path + [display name](#display-name), or precisely from [Charon](#charon) LLBC when `--with-charon` is used. The heuristic form uses bare `Type::method`; the Charon form includes `{Type}::method` with nested impl segments and may have the crate prefix from any crate in a [multi-crate LLBC](#multi-crate-llbc). Matching between Charon RQNs and atoms uses a normalized [match key](#match-key), not raw string equality. May be **absent**: [candidate resolution](#candidate-resolution) clears even the heuristic value when a Charon collision ends Ambiguous ([P20](properties.md#p20--charon-candidate-resolution-never-picks-arbitrarily) — a wrong RQN is worse than none). The [Manifest source](#manifest-enrichment-source) never replaces the heuristic form; only the LLBC source does.

**Span disambiguation** — The span stage of Charon candidate resolution (P20): when multiple [Charon](#charon) candidates share the same [match key](#match-key), survivors of the file and self-type filters are compared by LLBC span overlap with the [atom's](#atom) source line range, and only a strict-maximum positive overlap wins. For multi-line Charon spans, overlap is `min(atom_end, c_end) - max(atom_start, c_start)`. For single-line spans (`line_start == line_end`, common for dependency crates in [multi-crate LLBCs](#multi-crate-llbc)), a containment check is used instead: the span must fall within `[atom_start, atom_end]`. A tie or missing span data leaves the collision ambiguous — the atom's RQN is cleared rather than guessed. See `charon_names.rs` (`span_overlap`, `resolve_charon_candidate`).

**SCIP document** — A document entry in the [SCIP](#scip-source-code-intelligence-protocol) index, corresponding to a single source file. Contains [occurrences](#occurrence) and [symbol](#scip-source-code-intelligence-protocol) definitions. Each document has its own occurrence stream walked during [call attribution](#call-attribution).

**SCIP (Source Code Intelligence Protocol)** — The intermediate representation generated by rust-analyzer. A binary format (`index.scip`) converted to JSON (`index.scip.json`) by the `scip` CLI tool. Contains documents, symbol definitions, [occurrences](#occurrence), and metadata.

**ScipIndex** — Internal Rust type representing the parsed SCIP JSON structure. Contains a list of documents, each with [occurrences](#occurrence) and symbol information. Entry point for the call graph pipeline.

**Self type** — The implementing type of an impl block, as used by [candidate resolution](#candidate-resolution)'s self-type filter. Extracted on the atom side from the [display name](#display-name)'s `Type::` prefix (`SpecificServiceId<KIND>::eq` → `SpecificServiceId`) and on the Charon side from the qualified name's last impl segment (`{Trait<..> for path::SelfType}` or inherent `{path::SelfType}` → `SelfType`); generics, references, and path prefixes are stripped before comparison. See `charon_names.rs` (`self_type_from_display_name`, `self_type_from_qualified_name`, `bare_type_name`).

**Symbol roles** — A bitmask on SCIP [occurrences](#occurrence). Bit 0 (`& 1`) indicates a definition (as opposed to a reference). Used to identify [function-like definitions](#function-like-definition) vs callee references.

**syn** — Rust parser library used to parse source files for function body spans. SCIP only provides function name locations; syn finds the actual end line of function bodies. See [architecture](architecture.md).

**Target entry** — A package's lib or bin crate-root file: `[lib] path`/`src/lib.rs`, `[[bin]] path`, `src/main.rs`, `src/bin/*.rs`, `src/bin/*/main.rs`. The roots of the [mount chain](#mount-chain) walk. Test/bench/example targets are deliberately excluded. See `mod_chain.rs` (`package_entries`).

**Trait-required** — The `trait-required` atom field: a bodyless trait method signature, whose proof obligations live on the implementations. Trait methods with a default body are ordinary functions. See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).

**Type context** — Nearby type references (within 5 lines above a definition) used for [disambiguation](#disambiguation) when multiple SCIP symbols share the same [base code-name](#base-code-name). See [P9](properties.md).

**.verilib** — The output directory structure: `.verilib/probes/` holds extracted [atom](#atom) files. Convention shared across the probe ecosystem.


**Unmounted** — The `is-unmounted` atom field: the file is reached by no [mount chain](#mount-chain) from any analyzed package's [target entries](#target-entry), so it is not part of any lib or bin build. Inferred only from a [provably complete walk](#provably-complete-walk). See [P19](properties.md#p19--conservative-unmountedforeigntrait-facts).