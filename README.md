# probe-rust

Generate compact function call graph data from Rust projects.

`probe-rust` analyzes any standard Rust codebase and produces structured JSON describing every function, its dependencies (callees), source locations, and Rust-qualified names. It is designed for downstream verification and analysis tooling — in particular, the [Beneficial AI Foundation](https://github.com/Beneficial-AI-Foundation)'s verification pipeline, where it feeds into [probe-aeneas](https://github.com/Beneficial-AI-Foundation/probe-aeneas) for Rust-to-Lean translation mapping. Output follows the Schema 2.1 envelope format; see [docs/SCHEMA.md](docs/SCHEMA.md) for the full specification.

## Prerequisites

- **Rust toolchain** with `rust-analyzer`:

  ```bash
  rustup component add rust-analyzer
  ```

- **scip** CLI — auto-downloadable via `--auto-install`, or install manually from [sourcegraph/scip releases](https://github.com/sourcegraph/scip/releases). Pre-built binaries are available for Linux and macOS only (no Windows).

- **charon** (only when using `--with-charon`) — auto-buildable via `--auto-install` (requires `cargo`), or install from [AeneasVerif/charon](https://github.com/AeneasVerif/charon).

## Installation

### Pre-built binaries (recommended)

Every tagged release publishes binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64), and Windows (x86_64). See the [Releases](https://github.com/Beneficial-AI-Foundation/probe-rust/releases) page or use the installer scripts:

**Linux / macOS:**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Beneficial-AI-Foundation/probe-rust/releases/latest/download/probe-rust-installer.sh | sh
```

**Windows (PowerShell):**

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/Beneficial-AI-Foundation/probe-rust/releases/latest/download/probe-rust-installer.ps1 | iex"
```

> **Note:** On Windows, the `extract` command requires `scip`, which has no Windows binary. You will need to build `scip` from source or run `extract` under WSL.

### From source

```bash
cargo install --git https://github.com/Beneficial-AI-Foundation/probe-rust
```

Or clone and build locally:

```bash
git clone https://github.com/Beneficial-AI-Foundation/probe-rust
cd probe-rust
cargo install --path .
```

## Quick Start

```bash
# Install the latest release
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Beneficial-AI-Foundation/probe-rust/releases/latest/download/probe-rust-installer.sh | sh

# Run against a Rust project (downloads scip automatically on first run)
probe-rust extract /path/to/rust-project --auto-install
```

`rust-analyzer` must already be installed (see Prerequisites above). The `--auto-install` flag downloads `scip` but does not install `rust-analyzer`.

Output lands in `.verilib/probes/rust_<pkg>_<ver>.json` by default.

## Commands

| Command | Description |
|---------|-------------|
| `extract` | Generate function call graph atoms from a Rust project's SCIP index |
| `callee-crates` | Find which crates a function's callees belong to |
| `list-functions` | List all functions in a Rust project by parsing source files |

### `extract`

```bash
probe-rust extract <PROJECT_PATH> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <PATH>` | Output file path (default: `.verilib/probes/rust_<pkg>_<ver>.json`) |
| `--regenerate-scip` | Force regeneration of the SCIP index even if cached |
| `--with-locations` | Include per-call location data in output |
| `--allow-duplicates` | Continue on duplicate `code_name` entries (first occurrence kept) |
| `--auto-install` | Automatically download missing tools (`scip`, and `charon` when `--with-charon` is set) |
| `--with-charon` | Enrich atoms with Charon-derived `rust-qualified-name` fields (for Aeneas integration) |

For the full command reference with examples, see **[docs/USAGE.md](docs/USAGE.md)**. For the complete JSON schema specification, see **[docs/SCHEMA.md](docs/SCHEMA.md)**.

## Example Output

Running `probe-rust extract` produces a JSON envelope. Each entry in `data` describes a function and its callees:

```json
{
  "schema": "probe-rust/extract",
  "schema-version": "2.1",
  "tool": { "name": "probe-rust", "version": "0.1.0", "command": "extract" },
  "source": {
    "repo": "https://github.com/org/project.git",
    "commit": "abc123...",
    "language": "rust",
    "package": "my-crate",
    "package-version": "1.0.0"
  },
  "timestamp": "2026-03-17T12:00:00Z",
  "data": {
    "probe:my-crate/1.0.0/module/MyStruct#process()": {
      "display-name": "MyStruct::process",
      "dependencies": [
        "probe:my-crate/1.0.0/module/helper()"
      ],
      "code-module": "module",
      "code-path": "my-crate/src/module.rs",
      "code-text": { "lines-start": 42, "lines-end": 67 },
      "kind": "exec",
      "language": "rust",
      "rust-qualified-name": "my_crate::module::MyStruct::process",
      "is-disabled": false
    }
  }
}
```

## How It Works

1. **SCIP index generation** — runs `rust-analyzer` and `scip` to produce a Source Code Index Protocol file for the target project (cached in `<project>/data/`)
2. **Call graph construction** — parses the SCIP JSON to identify all function definitions, call relationships, and trait impl disambiguation
3. **Accurate line spans** — uses `syn` to parse Rust source files and resolve exact function body start/end lines
4. **Charon enrichment** (opt-in via `--with-charon`) — runs [Charon](https://github.com/AeneasVerif/charon) to derive Aeneas-compatible `rust-qualified-name` fields; only needed for projects integrating with Aeneas
5. **Schema 2.1 output** — wraps the call graph atoms in a metadata envelope containing git commit, repo URL, package info, and timestamps

## Releases

Releases are managed with [cargo-dist](https://opensource.axo.dev/cargo-dist/) and published automatically when a version tag (e.g. `v0.1.0`) is pushed. Each release includes:

- Pre-built binaries for all supported platforms
- Shell and PowerShell installer scripts
- SHA256 checksums

See the [Releases](https://github.com/Beneficial-AI-Foundation/probe-rust/releases) page for downloads and the [CHANGELOG](CHANGELOG.md) for what changed in each version.

## CI Integration

A reusable GitHub Actions workflow is provided for generating atoms from any Rust repository:

```yaml
jobs:
  generate:
    uses: Beneficial-AI-Foundation/probe-rust/.github/workflows/generate-atoms.yml@main
    with:
      target_repo: some-org/some-rust-project
```

See [docs/USAGE.md](docs/USAGE.md#ci-integration) for full details and matrix examples.

## License

MIT OR Apache-2.0
