# Usage Guide

## Commands

### `extract`

Generate function call graph atoms from a Rust project.

```
probe-rust extract <PROJECT_PATH> [OPTIONS]
```

**Arguments:**

- `PROJECT_PATH` -- Path to the Rust project root. Must contain a `Cargo.toml`, or probe-rust will search subdirectories (up to 2 levels deep) for one. If exactly one is found, it is used automatically; if multiple are found, you are prompted to specify which one.

**Options:**

| Flag | Short | Description |
|---|---|---|
| `--output <PATH>` | `-o` | Override the output file path. Default: `.verilib/probes/rust_<pkg>_<ver>.json` inside the project directory. |
| `--regenerate-scip` | | Force regeneration of the SCIP index, even if a cached version exists in `<project>/data/`. Useful after code changes. |
| `--with-locations` | | Include a `dependencies-with-locations` array in each atom, recording the source line of every call site. |
| `--allow-duplicates` | | Continue when duplicate `code_name` keys are detected. The first occurrence is kept and later duplicates are dropped. Without this flag, duplicates cause an error. |
| `--auto-install` | | Automatically download missing external tools. Downloads `scip` from GitHub releases. When combined with `--with-charon`, also builds `charon` from source. Tools are installed to `~/.probe-rust/tools/`. |
| `--with-charon` | | Run Charon to enrich atoms with Aeneas-compatible `rust-qualified-name` fields. Only needed for projects integrating with Aeneas. Requires `charon` to be installed or `--auto-install` to be set. |

### Examples

**Basic extraction:**

```bash
probe-rust extract ./my-rust-project
```

**With all options:**

```bash
probe-rust extract ./my-rust-project \
  --output ./output/atoms.json \
  --regenerate-scip \
  --with-locations \
  --allow-duplicates \
  --auto-install \
  --with-charon
```

**With Charon enrichment (for Aeneas projects):**

```bash
probe-rust extract ./my-aeneas-project --with-charon --auto-install
```

**Analyze a workspace member directly:**

```bash
probe-rust extract ./my-workspace/crates/core --auto-install
```

### `callee-crates`

Find which crates a function's callees belong to. Given a function and a traversal depth, BFS-walks the call graph and groups discovered callees by crate and version.

```
probe-rust callee-crates <FUNCTION> [OPTIONS]
```

**Arguments:**

- `FUNCTION` -- A function code-name (`probe:...`) or display-name to search for. Display-names are matched exactly first, then by partial/substring match. If the match is ambiguous, candidates are listed.

**Options:**

| Flag | Short | Description |
|---|---|---|
| `--depth <N>` | `-d` | Maximum traversal depth. 1 = direct callees, 2 = callees of callees, etc. |
| `--atoms-file <PATH>` | `-a` | Path to atoms.json file. Reads from stdin if omitted. Supports both bare-dict and Schema 2.0 envelope formats. |
| `--output <PATH>` | `-o` | Output file path. Prints to stdout if omitted. |
| `--exclude-stdlib` | | Exclude standard library crates (`core`, `alloc`, `std`) from output. |
| `--exclude-crates <LIST>` | | Comma-separated list of crate names to exclude. |

**Examples:**

```bash
# Direct callees of a function, reading from a file
probe-rust callee-crates "MyStruct::process" -d 1 -a atoms.json

# Two levels deep, excluding stdlib, piped from extract
cat atoms.json | probe-rust callee-crates "probe:my-crate/1.0/module/func()" -d 2 --exclude-stdlib

# Save output to file
probe-rust callee-crates "init" -d 3 -a atoms.json -o callees.json --exclude-crates "log,serde"
```

**Output format:**

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

---

### `list-functions`

List all functions in a Rust project by parsing source files with `syn`. Does not require SCIP or any external tools.

```
probe-rust list-functions <PATH> [OPTIONS]
```

**Arguments:**

- `PATH` -- Path to a Rust source file or directory. When a directory is given, all `.rs` files are recursively scanned (skipping `target/` and `.git/`).

**Options:**

| Flag | Short | Description |
|---|---|---|
| `--format <FORMAT>` | `-f` | Output format: `text` (default), `json`, or `detailed`. |
| `--exclude-methods` | | Exclude trait and impl methods; only show free functions. |
| `--show-visibility` | | Show function visibility (`pub`/`private`) in detailed output. |
| `--output <PATH>` | `-o` | Write JSON output to file (forces json format). |

**Examples:**

```bash
# List function names in a project
probe-rust list-functions ./my-project

# Detailed output with visibility for a single file
probe-rust list-functions src/lib.rs --format detailed --show-visibility

# JSON output to file
probe-rust list-functions ./my-project -o functions.json

# Only free functions (no methods)
probe-rust list-functions ./my-project --exclude-methods
```

**Output formats:**

`text` -- sorted, deduplicated function names, one per line:

```
build_call_graph
cmd_extract
parse_scip_json
```

`detailed` -- one line per function with location and context:

```
process [pub] @ src/lib.rs:42:58 in impl MyStruct
greet [private] @ src/lib.rs:60:65 in impl Greet for MyStruct
init [pub] @ src/main.rs:10:20

Summary: 3 functions in 2 files
```

`json` -- structured output:

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
    }
  ],
  "summary": {
    "total_functions": 1,
    "total_files": 1
  }
}
```

---

## Output Format

For the complete JSON schema specification covering all commands, see [SCHEMA.md](SCHEMA.md).

The `extract` command produces a JSON file wrapped in a Schema 2.0 metadata envelope:

```json
{
  "schema": "probe-rust/atoms",
  "schema-version": "2.0",
  "tool": {
    "name": "probe-rust",
    "version": "0.1.0",
    "command": "extract"
  },
  "source": {
    "repo": "https://github.com/org/project",
    "commit": "abc123def456...",
    "language": "rust",
    "package": "my-crate",
    "package-version": "1.0.0"
  },
  "timestamp": "2026-03-13T12:00:00Z",
  "data": {
    "probe:my-crate/1.0.0/module/MyStruct#my_method()": {
      "display-name": "MyStruct::my_method",
      "dependencies": [
        "probe:my-crate/1.0.0/module/helper()"
      ],
      "code-module": "module",
      "code-path": "my-crate/src/module.rs",
      "code-text": {
        "lines-start": 42,
        "lines-end": 58
      },
      "kind": "exec",
      "language": "rust",
      "rust-qualified-name": "my_crate::module::MyStruct::my_method"
    }
  }
}
```

### Atom Fields

| Field | Description |
|---|---|
| `display-name` | Human-readable function name (e.g. `MyStruct::method`). For impl methods, the Self type is prepended. |
| `dependencies` | Sorted set of `code_name` URIs for all functions called by this function. |
| `dependencies-with-locations` | (Only with `--with-locations`) Array of `{code-name, location, line}` objects for each call site. |
| `code-module` | Module path extracted from the code_name URI. |
| `code-path` | Relative source file path within the project. |
| `code-text.lines-start` | 1-based line number where the function body begins. |
| `code-text.lines-end` | 1-based line number where the function body ends. Derived from `syn` AST parsing for accuracy. |
| `kind` | Always `"exec"` for standard Rust functions. |
| `language` | Always `"rust"`. |
| `rust-qualified-name` | Rust-style qualified path (e.g. `my_crate::module::func`). When `--with-charon` is used and Charon enrichment succeeds, this is the Aeneas-compatible name; otherwise a heuristic based on file path and display name is used. |

### External Stubs

Functions called but defined outside the analyzed project (e.g. from dependencies) are included as stub entries with empty `code-path`, zero line numbers, and no dependencies.

## External Tools

probe-rust depends on two external tools to generate its input data:

### scip

The [scip](https://github.com/sourcegraph/scip) CLI converts `rust-analyzer` output into the SCIP index format.

**Resolution order:**

1. `~/.probe-rust/tools/scip` (managed directory)
2. `scip` on `$PATH`
3. Auto-download from GitHub if `--auto-install` is passed

**Version resolution for auto-download:**

1. `PROBE_SCIP_VERSION` environment variable (e.g. `v0.6.1`)
2. Latest stable release via GitHub API
3. Compiled-in fallback version (`v0.6.1`)

### rust-analyzer

Must be installed separately. The recommended approach:

```bash
rustup component add rust-analyzer
```

### charon (opt-in)

[Charon](https://github.com/AeneasVerif/charon) is used to derive Aeneas-compatible `rust-qualified-name` fields. It is only needed for projects that integrate with [Aeneas](https://github.com/AeneasVerif/aeneas) and is activated with the `--with-charon` flag. Charon is a rustc driver built from source with a matching nightly toolchain.

**Resolution order (when `--with-charon` is passed):**

1. `~/.probe-rust/tools/charon` (managed directory)
2. `charon` on `$PATH`
3. Clone and build from source if `--auto-install` is also passed

If Charon is not found and `--auto-install` is not set, the enrichment step will fail with an actionable error message. If Charon runs but encounters an error, extraction continues with heuristic-based qualified names.

## SCIP Caching

SCIP index generation is the most time-consuming step. Generated SCIP data is cached in `<project>/data/` to avoid re-running on subsequent invocations. Use `--regenerate-scip` to force a fresh index after code changes.

## Environment Variables

| Variable | Description |
|---|---|
| `PROBE_SCIP_VERSION` | Override the scip version to download (e.g. `v0.6.1`). Takes precedence over GitHub API and fallback. |

## CI Integration

A reusable GitHub Actions workflow is provided at `.github/workflows/generate-atoms.yml` for generating atoms from any Rust repository without installing probe-rust locally.

### Basic usage

```yaml
jobs:
  generate:
    uses: Beneficial-AI-Foundation/probe-rust/.github/workflows/generate-atoms.yml@main
    with:
      target_repo: some-org/some-rust-project
```

### Workflow inputs

| Input | Required | Default | Description |
|---|---|---|---|
| `target_repo` | Yes | | Repository to analyze in `org/repo` format |
| `target_ref` | No | default branch | Branch, tag, or SHA to checkout |
| `project_path` | No | `.` | Path to the Rust project within the repo |
| `artifact_name` | No | `atoms-json` | Name for the uploaded artifact |

### Matrix strategy (multiple repos)

```yaml
jobs:
  generate:
    strategy:
      matrix:
        include:
          - repo: some-org/project-a
            name: project-a
          - repo: some-org/project-b
            name: project-b
    uses: Beneficial-AI-Foundation/probe-rust/.github/workflows/generate-atoms.yml@main
    with:
      target_repo: ${{ matrix.repo }}
      artifact_name: ${{ matrix.name }}-atoms
```

### Retrieving artifacts

The workflow uploads the generated `atoms.json` as a GitHub Actions artifact with 30-day retention. Download it from the workflow run page or use it in a dependent job:

```yaml
jobs:
  use-atoms:
    needs: generate
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: atoms-json
```

## Supported Platforms

| Platform | Architecture | Binary | Installer |
|---|---|---|---|
| Linux | x86_64 | Yes | Shell |
| Linux | aarch64 | Yes | Shell |
| macOS | x86_64 | Yes | Shell |
| macOS | aarch64 (Apple Silicon) | Yes | Shell |
| Windows | x86_64 | Yes | PowerShell |
