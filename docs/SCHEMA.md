# probe-rust Data Schemas

Version: 2.1
Date: 2026-03-17

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

## Schema 2.0 Envelope

Commands that produce enveloped output (`extract`) wrap the payload in a
standardized metadata envelope:

```json
{
  "schema": "probe-rust/extract",
  "schema-version": "2.1",
  "tool": {
    "name": "probe-rust",
    "version": "0.1.0",
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
| `schema-version` | string | Interchange spec version (`"2.1"`) |
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
    "is-disabled": false
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
| `rust-qualified-name` | string | no | Rust-style qualified path (e.g. `my_crate::module::func`). When `--with-charon` is used, this is the Aeneas-compatible name; otherwise a heuristic based on file path and display name. |
| `is-disabled` | bool | yes | Always `false` in probe-rust output. Downstream tools (e.g. probe-aeneas) may set this to `true` for functions they did not process. |

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

When adding new optional fields, increment the minor version (`2.0` -> `2.1`).
When changing required fields or their semantics, increment the major version
(`2.0` -> `3.0`).

Consumers should check `schema-version` and reject files with an unsupported
major version.

---

## Compatibility with probe-verus

probe-rust atoms use the same data shape as probe-verus atoms. Key differences:

| Aspect | probe-rust | probe-verus |
|--------|-----------|-------------|
| Envelope `schema` | `"probe-rust/extract"` | `"probe-verus/atoms"` |
| `kind` values | Always `"exec"` | `"exec"`, `"proof"`, `"spec"` |
| `dependencies-with-locations` `location` | Always `"inner"` | `"inner"`, `"precondition"`, `"postcondition"` |
| `rust-qualified-name` | Optional (with `--with-charon`) | Not present |

The `callee-crates` and `list-functions` commands accept atoms.json from
either tool interchangeably.
