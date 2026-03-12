# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

probe-rust is a Rust CLI tool that generates compact function call graph data from SCIP (Source Code Index Protocol) indexes for standard Rust projects. It has a single subcommand:
- **extract**: Generate call graph atoms with accurate line numbers from any Rust project

## Build and Test Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Optimized release build
cargo install --path .         # Install locally

# Test
cargo test                     # All tests
cargo test --lib --verbose     # Unit tests only

# Code quality (all enforced in CI)
cargo fmt --all                # Format code
cargo clippy --all-targets -- -D warnings  # Lint (no warnings allowed)

# Development workflow
cargo fmt && cargo clippy --all-targets && cargo test
```

## Project Structure

```
src/
├── main.rs          # CLI entry point with subcommand routing
├── lib.rs           # Core data structures and SCIP JSON parsing
├── commands/        # Subcommand implementations
│   ├── mod.rs       # Module declarations
│   └── extract.rs   # Extract command (generate call graph atoms)
├── constants.rs     # Shared constants (SCIP kinds, prefixes)
├── error.rs         # Unified error types
├── metadata.rs      # Schema 2.0 envelope construction, project metadata
├── path_utils.rs    # Path matching utilities
├── rust_parser.rs   # AST parsing using syn for function spans
├── scip_cache.rs    # SCIP index generation, caching
└── tool_manager.rs  # Auto-download manager for scip CLI tool
```

## Architecture

### Pipeline

1. **Extract Pipeline** (`extract` command): SCIP JSON → call graph parsing → spans via syn → Schema 2.0 envelope → `.verilib/probes/`

### Key Architectural Patterns

**Accurate Line Spans**: SCIP only provides function name locations. Uses `syn` AST visitor to parse actual function body spans.

**Trait Implementation Disambiguation**: Multiple strategies to resolve SCIP symbol conflicts for trait impls: signature text extraction, self type from parameters, definition type context, line number fallback.

**SCIP Data Caching**: Generated SCIP data is cached in `<project>/data/` to avoid re-running slow external tools.

**Auto-download Tool Manager**: The `scip` CLI tool can be auto-downloaded to `~/.probe-rust/tools/`. Version resolution: env var override → GitHub `/releases/latest` API → compiled-in fallback.

**Schema 2.0 Metadata Envelope**: All JSON outputs are wrapped in a standardized envelope containing `schema`, `schema-version`, `tool`, `source`, `timestamp`, and `data` fields.

### Key Types

- `FunctionNode`: Call graph node with callees and type context
- `AtomWithLines`: Output format with line ranges
- `Envelope<T>`: Schema 2.0 metadata wrapper for JSON output
- `ProjectMetadata`: Git commit, repo URL, timestamp, package name/version

## External Tool Dependencies

- **rust-analyzer**: Must be installed (via `rustup component add rust-analyzer` or on PATH)
- **scip CLI**: Auto-downloadable via `--auto-install` flag

## Before Committing

Always run fmt and clippy before committing:

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings
```

## Commit Message Style

Use conventional commits: `feat(module):`, `fix(module):`, `perf(module):`, `refactor(module):`
