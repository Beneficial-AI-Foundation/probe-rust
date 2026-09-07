# probe-rust Data Schemas

Version: 3.0
Date: 2026-07-27

This document specifies the JSON output formats produced by each probe-rust
subcommand. It complements the language-agnostic
[envelope-rationale.md](https://github.com/Beneficial-AI-Foundation/probe/blob/main/docs/envelope-rationale.md)
which defines the envelope wrapper; this document defines what goes **inside**
the `data` field and the output of non-enveloped commands.

---

## Common Types

### CodeTextInfo

Line range of a function body (1-based, inclusive).

```json
{
  "lines-start": 42,
  "lines-end": 67
}
```

| Field | Type | Description |
|-------|------|-------------|
| `lines-start` | integer | First line of the function (1-based) |
| `lines-end` | integer | Last line of the function (1-based, inclusive) |

### DeclKind

Declaration kind, serialized as a lowercase string.

| Value | Meaning |
|-------|---------|
| `"exec"` | Executable code (always `"exec"` for standard Rust) |

### Code-Name Format

All atom entries use **probe code-names** as dictionary keys. The format is:

```
probe:<crate>/<version>/<module-path>/<Type>#<Trait><TypeParam>#<method>()
```

Examples:

- Free function: `probe:my-crate/1.0.0/module/helper()`
- Inherent method: `probe:my-crate/1.0.0/module/MyStruct#process()`
- Trait impl: `probe:my-crate/1.0.0/module/MyStruct#Add<&MyStruct>#add()`

For standard library functions whose SCIP symbol uses a URL-style version:

```
probe:core/https://github.com/rust-lang/rust/library/core/option/impl#map()
```

The code-name is not serialized inside the value object -- it is the dictionary key.

---

## Schema 3.0 Envelope

Commands that produce enveloped output (`extract`) wrap the payload in a
standardized metadata envelope:

```json
{
  "schema": "probe-rust/extract",
  "schema-version": "3.0",
  "tool": {
    "name": "probe-rust",
    "version": "0.4.0",
    "command": "extract"
  },
  "source": {
    "repo": "https://github.com/org/project.git",
    "commit": "abc123def456...",
    "language": "rust",
    "package": "my-crate",
    "package-version": "1.0.0"
  },
  "timestamp": "2026-03-13T12:00:00Z",
  "data": { ... }
}
```

### Envelope Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema` | string | Data type identifier: `"probe-rust/extract"` |
| `schema-version` | string | Interchange spec version (`"3.0"`) |
| `tool.name` | string | Always `"probe-rust"` |
| `tool.version` | string | Semver version of the probe-rust binary |
| `tool.command` | string | Subcommand that produced the file (e.g. `"extract"`) |
| `source.repo` | string | Git remote URL of the analyzed project |
| `source.commit` | string | Full git commit hash at analysis time |
| `source.language` | string | Always `"rust"` |
| `source.package` | string | Package/crate name from `Cargo.toml` |
| `source.package-version` | string | Package version (or 7-char git hash if version is absent) |
| `timestamp` | string | ISO 8601 timestamp of when the analysis ran |
| `data` | object | The payload (structure depends on `schema`) |

---

## 1. `probe-rust/extract` -- Call Graph Atoms

**Produced by:** `extract`
**Envelope schema:** `"probe-rust/extract"`

### Data Shape

`data` is an object keyed by code-name. Each value is an `AtomWithLines`:

```json
{
  "probe:my-crate/1.0.0/module/MyStruct#method()": {
    "display-name": "MyStruct::method",
    "dependencies": [
      "probe:my-crate/1.0.0/module/helper()",
      "probe:other-crate/2.0.0/lib/utility()"
    ],
    "dependencies-with-locations": [
      {
        "code-name": "probe:my-crate/1.0.0/module/helper()",
        "location": "inner",
        "line": 55
      }
    ],
    "code-module": "module",
    "code-path": "my-crate/src/module.rs",
    "code-text": { "lines-start": 42, "lines-end": 67 },
    "kind": "exec",
    "language": "rust",
    "rust-qualified-name": "my_crate::module::MyStruct::method",
    "charon-def-id": 439,
    "charon-version": "0.1.217",
    "untracked": false,
    "cfg": "feature = \"alloc\"",
    "is-public": true,
    "is-public-api": true
  }
}
```

### Field Reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `display-name` | string | yes | Human-readable name (e.g. `"MyStruct::method"`). For impl methods, the Self type is prepended. |
| `dependencies` | array of strings | yes | Sorted code-names of callees |
| `dependencies-with-locations` | array of objects | no | Present only when `--with-locations` is used |
| `code-module` | string | yes | Module path extracted from the code-name (may be empty for top-level functions) |
| `code-path` | string | yes | Relative source file path (empty string for external stubs) |
| `code-text` | CodeTextInfo | yes | Line range of the function body |
| `kind` | DeclKind | yes | Always `"exec"` for standard Rust |
| `language` | string | yes | Always `"rust"` |
| `rust-qualified-name` | string | no | Rust-style qualified path (e.g. `my_crate::module::func`). When `--with-charon` is used, this is the Aeneas-compatible LLBC-derived name for every function Charon matched; a function no Charon entry matched keeps the SCIP-derived form; and the field is **absent** when several same-file Charon impls share the function's match key and no signal (self type, span) can tell them apart — a wrong name is worse than none (KB P20). Otherwise (including the `--translation` manifest path, which never overrides or clears it) it is derived from the SCIP symbol’s module chain and the function’s `display-name`. |
| `untracked` | bool | yes | Always `false` in probe-rust output. Downstream tools (e.g. probe-aeneas) may set this to `true` for functions that are out of scope. |
| `cfg` | string | no | The item-gating `#[cfg(...)]` predicate governing the function, as completely as static analysis can see it: its own `#[cfg]`, a file-level `#![cfg(...)]`, every enclosing same-file `impl`/`mod`/`trait`/`extern` gate, and the gates on the `mod` declaration chain mounting its file from its package's target entries, `all(...)`-joined (e.g. `all(test, feature = "serde")`). Cosmetic `#[cfg_attr(...)]` is ignored. Known under-gating (deliberate, conservative — the predicate may say "compiled" when the build would say no, never the reverse): `cfg_if!` branch predicates are dropped, and files the module-tree walk could not reach contribute no chain component. Omitted when the function is not gated at all. Downstream tools evaluate it against the build config to decide whether the function is compiled (and hence in scope). See **Predicate format** below. |
| `file-cfg` | string | no | The parent-file mod-chain component of `cfg`, alone (already folded into `cfg`). A file mounted through several chains — including chains from different targets or workspace members — gets `any(...)` over the per-chain `all(...)` conjunctions (sorted, deduplicated); a file with at least one gate-free mount chain has no file gate at all. Lets consumers report *why* a function is gated (file-level vs own gate) without re-deriving the module tree. Omitted when the file is unconditionally mounted, was not reachable by the module-tree walk, or the walk was not provably complete (same requirement as `is-unmounted` — an unseen mount could be ungated, so gates from an incomplete walk could over-state). |
| `is-unmounted` | bool | no | `true` when the function's file is reached by no `mod` chain from any analyzed package's **library or binary target entries** (`[lib]`/`src/lib.rs`, `[[bin]]`/`src/main.rs`/`src/bin/*`). Such a file is not part of any lib or bin build; it may still be the root of (or `#[path]`-mounted from) a test/bench/example target, which this walk deliberately does not scan — downstream scope policy already treats those targets as out of the verified build. Emitted only when the module-tree walk was provably complete (any unparsable file, unresolvable `mod` target, `include!`, `cfg_attr` on a `mod`, any item or macro whose tokens mention `mod`, unreadable package manifest, mount cycle, or chain-cap overflow disables this inference — and all `file-cfg` emission — for the whole project, so lib/bin-compiled code is never mislabeled). Omitted when `false`. |
| `is-foreign` | bool | no | `true` when the function is declared inside an `extern { … }` block: a binding to another language with no Rust body to analyze or verify. Judged on the enclosing AST block, so bodyless trait signatures are never foreign. Omitted when `false`. |
| `trait-required` | bool | no | `true` for a bodyless trait method signature (its proof obligations live on the implementations). Trait methods with a default body are ordinary functions and omit this. Omitted when `false`. |

> **Predicate format.** `cfg`/`file-cfg` values are not whitespace-normalized: predicates copied from source attributes keep syn's token-stream spacing (`not (feature = "std")`, `all (test , not (feature = "verify"))`), while combinators synthesized by probe-rust are tight (`all(a, b)`, `any(a, b)`), so one value can mix both regimes. Consumers must parse whitespace-insensitively (probe-aeneas's `cfg_eval` does).
| `is-public` | bool | no | `true` if the function is declared `pub`. Derived from the SCIP signature (e.g. `pub fn` vs `fn`). Always present for internal atoms; absent for external stubs. When `--with-charon` is used and the matched LLBC entry carries visibility (`attr_info.public`), the Charon-derived value takes precedence; a candidate without visibility (and the `--translation` manifest path, which carries none) never clobbers the SCIP value. This is item-level visibility, not crate-level API reachability. |
| `is-public-api` | bool | no | Set **only** by `--with-public-api` (absent for every atom otherwise): `true` = a `cargo public-api` entry proves the function public — RQN match after the candidate-form passes, or a public entry resolving uniquely to the atom's impl descriptor (which can mark an RQN-less same-crate atom); `false` = the atom has an RQN no public entry matches. Absent for stub atoms (no analyzed source) the resolution never reaches. See **Limitations** below. |
| `charon-def-id` | integer | no | Charon `FunDeclId`, carried through the same match-key/span resolution that assigns `rust-qualified-name`. Equals Aeneas's `translation.json` `def_id`, enabling a precise integer join to the Lean translation. Populated from **either** source: with `--with-charon` it is the `Fun` key from the LLBC's `item_names`; with `--translation <manifest>` it is the `def_id` of a `functions[]` entry (charon's `FunDeclId` id-space only — `globals`/`trait_impls` are excluded). Present only when a Charon function matched **and** that source's `charon_version` was read. Its accuracy is exactly that of the underlying Charon-name match (a lone candidate must match the atom's file path and, when both carry lines, overlap its span — except that `--with-charon` still accepts a lone candidate carrying no file path at all, as span-less compiler-generated items do; colliding candidates are narrowed by same file → self type → dedup → strict-maximum span overlap, and an unresolvable collision stamps no id; the manifest path additionally refuses match-key-only hits — KB P20) — it is not an independent oracle, so consumers should still gate on `charon-version` and may corroborate with `rust-qualified-name`/span rather than treat the id alone as ground truth. |
| `charon-version` | string | no | The charon version that produced `charon-def-id` — the top-level `charon_version` of the LLBC (`--with-charon`) or of the `translation.json` (`--translation`). Lets consumers provenance-gate the join — trust `charon-def-id` only when this matches the charon version behind their manifest. |

> **Coupling invariant.** `charon-def-id` and `charon-version` are emitted **together or not at all** — a `FunDeclId` is only interpretable relative to the charon run that produced it, so probe-rust never writes an id without its version. This holds for both enrichment sources (`--with-charon` LLBC and `--translation` manifest). Re-enrichment (running either source over already-enriched atoms) clears **both** for any atom that does not produce a fresh id+version this pass — whether no Charon function matched or the version could not be read — so a stale id never survives against a different source. Consumers may treat a present `charon-def-id` as always accompanied by a `charon-version`. (Enforced by the emitter, not the deserializer: an externally hand-written file with an orphan id would still parse.)

### DependencyWithLocation

Only present when `--with-locations` is passed to `extract`.

| Field | Type | Description |
|-------|------|-------------|
| `code-name` | string | Code-name of the callee |
| `location` | string | Always `"inner"` for standard Rust (no precondition/postcondition distinction) |
| `line` | integer | 1-based line number of the call site |

### External Stubs

Functions called as dependencies but defined outside the analyzed project get
stub entries with:
- `code-path`: `""`
- `code-text`: `{"lines-start": 0, "lines-end": 0}`
- `dependencies`: empty
- `rust-qualified-name`: absent
- `is-public`: absent
- `cfg`, `file-cfg`, `is-unmounted`, `is-foreign`, `trait-required`: absent
  (there is no analyzed source to derive them from)
- `is-public-api`: absent — unless `--with-public-api` resolves a public entry to
  this atom's impl descriptor, which happens only for the analyzed crate's own
  atoms (a macro-generated impl leaves a definition-less atom of exactly this
  shape); stubs from other crates always stay absent
- `charon-def-id`: absent
- `charon-version`: absent

### Limitations: `is-public-api`

Without `--with-public-api`, `is-public-api` is absent for every atom — no
default derivation exists. (An earlier SCIP module-chain walk default was
removed; `is-public` carries the signature-derived visibility instead.)

#### With `--with-public-api` (cargo-public-api)

When `--with-public-api` is used, `is-public-api` is set for all atoms
that have a `rust-qualified-name` (RQN). Matching is RQN-based: each atom's RQN
is checked against the set of qualified names parsed from `cargo public-api -sss`
output. This provides ground-truth public API surface from `rustdoc`.

Names are reconciled in additive passes because `cargo public-api` names items
by their public *use* path while atoms carry the *definition* path: crate-root
`pub use` rewriting, inherited-default-trait-method resolution, then
impl-descriptor resolution for entries no atom name matched — which marks the
implementing atom (possibly RQN-less) directly. Every pass is additive,
no pass resolves an ambiguity, and the resolution passes act only on entries
still unmatched. The pass sequence and its
guards are specified canonically in KB property P11
(`kb/engineering/properties.md`); `docs/PUBLIC_API_LIMITATIONS.md` catalogues
what remains unmatched and why.

- Standard blanket impl entries (`Into`, `TryFrom`, `TryInto`, `Borrow`,
  `BorrowMut`, `Any`, `ToOwned`, `CloneInto`, `From`) are filtered from the
  `cargo public-api` output since they have no corresponding atoms.
- Requires nightly toolchain and `cargo-public-api` (use `--auto-install`).
- Stubs from other crates (no RQN) are unaffected — `is-public-api` stays absent.
- Two different metrics are printed: `is-public-api: N true, M false` counts
  atoms; `public-api entries matched: N/M` counts `cargo public-api` entries
  backed by an atom. Several entries can share one atom, so the numbers move
  independently. See `docs/PUBLIC_API_LIMITATIONS.md`.

---

## 2. `callee-crates` -- Crate Dependencies at Call Depth

**Produced by:** `callee-crates`
**Envelope:** None (raw JSON)

### Output Shape

```json
{
  "function": "probe:my-crate/1.0.0/module/MyStruct#process()",
  "depth": 2,
  "crates": [
    {
      "crate": "my-crate",
      "version": "1.0.0",
      "functions": [
        "probe:my-crate/1.0.0/module/helper()"
      ]
    },
    {
      "crate": "dep-crate",
      "version": "2.0.0",
      "functions": [
        "probe:dep-crate/2.0.0/lib/utility()"
      ]
    }
  ]
}
```

### Field Reference

| Field | Type | Description |
|-------|------|-------------|
| `function` | string | Resolved code-name of the root function |
| `depth` | integer | BFS traversal depth used |
| `crates` | array of CrateEntry | Callees grouped by crate |

### CrateEntry

| Field | Type | Description |
|-------|------|-------------|
| `crate` | string | Crate name |
| `version` | string | Crate version, or `"stdlib"` for `core`/`alloc`/`std` |
| `functions` | array of strings | Code-names of callees in this crate |

---

## 3. `list-functions` -- Function Listing

**Produced by:** `list-functions` (with `--format json` or `--output`)
**Envelope:** None (raw JSON)

### Output Shape

```json
{
  "functions": [
    {
      "name": "process",
      "file": "src/lib.rs",
      "start_line": 42,
      "end_line": 58,
      "visibility": "pub",
      "context": "impl MyStruct",
      "is_method": true
    },
    {
      "name": "init",
      "file": "src/main.rs",
      "start_line": 10,
      "end_line": 20,
      "is_method": false
    }
  ],
  "summary": {
    "total_functions": 2,
    "total_files": 2
  }
}
```

### FunctionInfo

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Function/method name |
| `file` | string | no | Relative source file path |
| `start_line` | integer | yes | First line of the function (1-based) |
| `end_line` | integer | yes | Last line of the function (1-based) |
| `visibility` | string | no | `"pub"`, `"pub(crate)"`, etc. Absent for private functions. |
| `context` | string | no | Enclosing impl/trait block (e.g. `"impl MyStruct"`, `"trait MyTrait"`, `"impl Greet for MyStruct"`). Absent for free functions. |
| `is_method` | boolean | yes | `true` for methods inside impl/trait blocks, `false` for free functions |

### FunctionListSummary

| Field | Type | Description |
|-------|------|-------------|
| `total_functions` | integer | Number of functions in the listing |
| `total_files` | integer | Number of source files containing at least one function |

---

## Schema Evolution

When adding new optional fields, increment the minor version (`2.1` -> `2.2`).
When changing required fields or their semantics, increment the major version
(`2.0` -> `3.0`).

Consumers should check `schema-version` and reject files with an unsupported
major version.

### Changelog

- **3.0 — doc clarification** (2026-09-02, no wire change, `schema-version`
  stays `3.0`): under `--with-charon`, `rust-qualified-name` may now be
  **absent** on an analyzed-crate atom when several same-file Charon impls
  collide on its match key and cannot be told apart (KB P20); previously one
  impl's name was stamped on every colliding atom. Trait-impl qualified names
  now render primitive type arguments (`TryFrom<u8, …>`) and `?` for
  trait type arguments probe-rust cannot render (slice, array, tuple, raw
  pointer, `dyn`). `--translation` behavior
  is unchanged.
- **3.0 — doc clarification** (2026-08-31, no wire change, `schema-version`
  stays `3.0`): `is-public-api` is emitted only under `--with-public-api`;
  without the flag it is absent for every atom. This has been the wire
  behavior since 2026-04-09 — the 2.3 note below saying "SCIP walk remains
  the zero-dependency default" never took effect (the walk was unwired the
  same day, and its dead code removed in 0.11.0).
- **3.0 — doc clarification** (2026-08-28, no wire change, `schema-version`
  stays `3.0`): `--with-public-api` can now set `is-public-api: true` on an
  analyzed-crate atom with no `rust-qualified-name` (impl-descriptor
  resolution; see KB P11). Field types and names are unchanged.
- **3.0** (2026-07-27): Renamed the `is-disabled` atom field to `untracked`
  (both the JSON wire name and the Rust field). Semantics and boolean polarity
  are unchanged (`is-disabled: true` → `untracked: true`). Breaking wire-format
  change with no backward-compatibility alias, and part of the ecosystem-wide
  major bump to a unified schema-version `3.0`.
- **2.4** (2026-07-16): Added `charon-def-id` and `charon-version` optional
  fields, emitted together or not at all. Populated from either the
  `--with-charon` LLBC or a `--translation <manifest>` Aeneas `translation.json`
  (which reads `def_id`s without running charon). Enables downstream tools
  (probe-aeneas) to join Rust↔Lean by Charon `FunDeclId` integer equality,
  provenance-gated on matching charon version.
- **2.3** (2026-04-07): Replaced `cargo-public-api` integration with SCIP
  module-chain visibility walk. `is-public-api` is now always present for
  internal atoms (binary `true`/`false`), no uncertain bucket. No external
  tools or nightly toolchain required. *Updated 2026-04-09:* re-added
  `cargo-public-api` as an opt-in override (`--with-public-api`). Uses
  RQN-based matching; SCIP walk remains the zero-dependency default. No
  schema change (same fields and types).
- **2.2** (2026-04-07): Added `is-public-api` field. Changed `is-public` to
  always be present for internal atoms (derived from SCIP signature visibility),
  no longer requires `--with-charon`.
- **2.1** (2026-03-17): Added `is-public` field (Charon-only).

---

## Compatibility with probe-verus

probe-rust atoms use the same data shape as probe-verus atoms. Key differences:

| Aspect | probe-rust | probe-verus |
|--------|-----------|-------------|
| Envelope `schema` | `"probe-rust/extract"` | `"probe-verus/atoms"` |
| `kind` values | Always `"exec"` | `"exec"`, `"proof"`, `"spec"` |
| `dependencies-with-locations` `location` | Always `"inner"` | `"inner"`, `"precondition"`, `"postcondition"` |
| `rust-qualified-name` | Optional (LLBC-derived with `--with-charon`, absent there on an unresolvable impl collision; SCIP-derived otherwise, including `--translation`) | Not present |
| `is-public` | Always for internal atoms (from SCIP); an LLBC candidate with visibility overrides, `--translation` never does | Not present |
| `is-public-api` | Optional: only under `--with-public-api` (`true`/`false` for atoms it reaches; absent otherwise and for stubs) | Not present |
| `charon-def-id` / `charon-version` | Optional pair (with `--with-charon` LLBC or `--translation` manifest, when matched + version read) | Not present |

The `callee-crates` and `list-functions` commands accept atoms.json from
either tool interchangeably.
