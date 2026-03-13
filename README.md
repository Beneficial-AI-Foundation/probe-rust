# probe-rust

Generate compact function call graph data from Rust projects.

`probe-rust` analyzes any standard Rust codebase and produces structured JSON describing every function, its dependencies (callees), source locations, and Rust-qualified names. The output follows a Schema 2.0 envelope format suitable for downstream verification and analysis tooling.

## Quick Start

```bash
# Install the latest release
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Beneficial-AI-Foundation/probe-rust/releases/latest/download/probe-rust-installer.sh | sh

# Run against a Rust project
probe-rust extract /path/to/rust-project --auto-install
```

The `--auto-install` flag automatically downloads the required `scip` CLI tool on first run.

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

## Commands

| Command | Description |
|---|---|
| `extract` | Generate function call graph atoms from a Rust project's SCIP index |
| `callee-crates` | Find which crates a function's callees belong to (post-processing on atoms.json) |
| `list-functions` | List all functions in a Rust project by parsing source files |

### `extract`

```bash
probe-rust extract <PROJECT_PATH> [OPTIONS]
```

| Option | Description |
|---|---|
| `-o, --output <PATH>` | Output file path (default: `.verilib/probes/rust_<pkg>_<ver>_atoms.json`) |
| `--regenerate-scip` | Force regeneration of the SCIP index even if cached |
| `--with-locations` | Include per-call location data in output |
| `--allow-duplicates` | Continue on duplicate `code_name` entries (first occurrence kept) |
| `--auto-install` | Automatically download missing tools (`scip`, and `charon` when `--with-charon` is set) |
| `--with-charon` | Enrich atoms with Charon-derived `rust-qualified-name` fields (for Aeneas integration) |

### `callee-crates`

```bash
probe-rust callee-crates <FUNCTION> --depth <N> [OPTIONS]
```

### `list-functions`

```bash
probe-rust list-functions <PATH> [OPTIONS]
```

For the full command reference with all options, examples, and output format details, see **[docs/USAGE.md](docs/USAGE.md)**. For the complete JSON schema specification, see **[docs/SCHEMA.md](docs/SCHEMA.md)**.

## How It Works

1. **SCIP index generation** -- runs `rust-analyzer` and `scip` to produce a Source Code Index Protocol file for the target project (cached in `<project>/data/`)
2. **Call graph construction** -- parses the SCIP JSON to identify all function definitions, call relationships, and trait impl disambiguation
3. **Accurate line spans** -- uses `syn` to parse Rust source files and resolve exact function body start/end lines
4. **Charon enrichment** (opt-in via `--with-charon`) -- runs [Charon](https://github.com/AeneasVerif/charon) to derive Aeneas-compatible `rust-qualified-name` fields; only needed for projects integrating with Aeneas
5. **Schema 2.0 output** -- wraps the call graph atoms in a metadata envelope containing git commit, repo URL, package info, and timestamps

## Prerequisites

- **Rust toolchain** with `rust-analyzer` (`rustup component add rust-analyzer`)
- **scip** CLI -- auto-downloadable via `--auto-install`, or install manually from [sourcegraph/scip](https://github.com/sourcegraph/scip/releases)
- **charon** (only when using `--with-charon`) -- auto-buildable via `--auto-install`, or install from [AeneasVerif/charon](https://github.com/AeneasVerif/charon)

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
