# Changelog

All notable changes to probe-rust are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `is-disabled` field on all Rust atoms (always `false` in probe-rust output; downstream tools like probe-aeneas may override). Schema version bumped to 2.1.
- `callee-crates` command: BFS-traverse a call graph and group callees by crate/version (ported from probe-verus).
- `list-functions` command: list all functions in a Rust project by parsing source with `syn`, with text/json/detailed output formats.
- Project documentation: README, usage guide, schema specification, and changelog.
- `--with-charon` flag to opt in to Charon-based `rust-qualified-name` enrichment (for Aeneas integration).

### Changed
- Charon enrichment is now opt-in (requires `--with-charon`). Previously it ran on every extraction and silently skipped when Charon was unavailable.
- `--auto-install` only installs Charon when `--with-charon` is also passed.

## [0.1.0] - 2026-03-12

Initial release.

### Added
- `extract` command: generate function call graph atoms from any Rust project's SCIP index.
- Schema 2.0 metadata envelope wrapping all JSON output (schema, tool info, source info, timestamps).
- Accurate function body line spans via `syn` AST parsing.
- Trait implementation disambiguation using signature text, self type, definition type context, and line number fallback.
- SCIP index caching in `<project>/data/` to avoid redundant regeneration.
- Auto-download of the `scip` CLI tool via `--auto-install` (installed to `~/.probe-rust/tools/`).
- `rust-qualified-name` derivation from file paths and display names, with support for bare `src/` paths.
- External function stub generation for cross-crate dependencies.
- `--with-locations` flag for per-call-site location data.
- `--allow-duplicates` flag to handle ambiguous trait impl code names.
- Automatic Cargo.toml discovery in subdirectories (up to 2 levels deep).
- Reusable GitHub Actions workflow (`generate-atoms.yml`) for CI-driven atom generation.
- CI pipeline with formatting, clippy, and unit test checks.
- Release automation via cargo-dist for Linux, macOS, and Windows binaries.

[Unreleased]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/releases/tag/v0.1.0
