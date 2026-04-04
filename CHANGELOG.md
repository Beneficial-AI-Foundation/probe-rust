# Changelog

All notable changes to probe-rust are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-04-04

### Added
- **`setup` command**: installs scip and verifies rust-analyzer availability.
  Downloads scip into `~/.probe-rust/tools/`; checks whether rust-analyzer
  is installed and provides instructions if missing (must be installed via
  `rustup component add rust-analyzer`). Use `--status` to inspect without
  installing.

## [0.2.1] - 2026-03-31

### Fixed
- **Charon enrichment for nested functions**: inner functions (e.g. `step_2` defined inside `fn decompress()`) now match their Charon LLBC names via a `code_module`-based fallback key. Previously the match key was built from `code_path` only, missing parent-function scoping that Charon includes in its `Name` path.

### Changed
- **Refactored Charon candidate matching**: extracted `resolve_charon_candidate` helper from `enrich_atoms_with_charon_names` to avoid duplicating the 1-candidate / span-disambiguation / heuristic-RQN logic across primary and fallback key paths.

## [0.2.0] - 2026-03-27

### Added
- **`is-public` field** (Charon): extract visibility from Charon LLBC `attr_info.public`. Only present when `--with-charon` is used.
- **`is-disabled` field** on all Rust atoms (always `false` in probe-rust output; downstream tools like probe-aeneas may override). Schema version bumped to 2.1.
- **`callee-crates` command**: BFS-traverse a call graph and group callees by crate/version (ported from probe-verus).
- **`list-functions` command**: list all functions in a Rust project by parsing source with `syn`, with text/json/detailed output formats.
- **`--with-charon` flag** to opt in to Charon-based `rust-qualified-name` enrichment (for Aeneas integration).
- **Library API**: expose `commands` module from `lib.rs` so downstream crates and integration tests can call `cmd_extract` directly.
- **Integration tests** using `probe-extract-check` for backward-compatible output validation.
- **SHA-256 checksum verification** (`PROBE_SCIP_SHA256`) and download size validation for scip binary downloads.
- Project documentation: README, usage guide, schema specification, testing guide, and changelog.
- Example output file (`examples/rust_curve25519-dalek_4.1.3.json`).

### Changed
- **Schema renamed** from `probe-rust/atoms` to `probe-rust/extract`, aligning with the `tool/command` naming convention used by probe-lean, probe-verus, and probe-aeneas.
- **Default output filename** drops `_atoms` suffix (e.g. `rust_mycrate_0.1.0.json` instead of `rust_mycrate_0.1.0_atoms.json`).
- **Charon enrichment is now opt-in** (requires `--with-charon`). Previously it ran on every extraction and silently skipped when Charon was unavailable.
- **`--auto-install`** only installs Charon when `--with-charon` is also passed.
- **Aeneas-compatible trait names** in Charon-derived `rust-qualified-name`: emit `{Trait for SelfType}` format, include trait generic parameters (e.g. `{Mul<&Scalar, EdwardsPoint> for EdwardsPoint}`).
- **`#[non_exhaustive]`** on all public enums.
- JSON parsing uses `from_reader(BufReader)` to halve peak memory usage.
- Decompose `build_call_graph` into focused helper functions; eliminate duplicated iteration pass.

### Fixed
- **Charon name disambiguation**: use span-based line overlap and trait generics to resolve collisions when multiple Charon functions share the same match key. Previously fell back to `candidates.first()`, assigning arbitrary wrong names.
- **Exact type-name match** for trait implementation disambiguation (prevents false matches).
- **Path traversal prevention**: sanitize package name/version in output filenames; canonicalize+starts_with containment checks for workspace member paths and SCIP relative paths.
- **SCIP range safety**: clamp negative range values to 0 before `usize` cast.
- **Path matching boundary**: require path-separator boundary in `paths_match_by_suffix` to prevent false positives (e.g. `my_lib.rs` no longer matches `lib.rs`).
- Fail safely when project root cannot be canonicalized (return error instead of panic).
- Replace `expect()` panics with proper `Result` returns in `ScipCache`/`CharonCache`.
- Migrate `ScipError`, `CharonError`, `ToolError` to `thiserror` with `#[source]` for proper error chains.

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

[Unreleased]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/releases/tag/v0.1.0
