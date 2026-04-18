# Changelog

All notable changes to probe-rust are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.3] - 2026-04-18

### Fixed
- **Charon enrichment now works for multi-crate LLBCs** (issue #7):
  `make_match_key_from_charon` stripped only the target crate prefix from
  qualified names. Functions from dependency crates included via Charon's
  `--include` flag retained their full crate-qualified name as the match key
  and never matched any atom. The function now strips the first `::` segment
  unconditionally, which is always the crate name regardless of which crate
  the function belongs to.
- **Span disambiguation handles single-line Charon spans** (issue #8):
  `disambiguate_by_span` used an overlap formula that always yields zero when
  `line_start == line_end` (a single-line span pointing at the function
  signature). This is common for dependency crates in multi-crate LLBCs —
  89% of spans in the libsignal-verify LLBC are single-line. The function
  now uses a containment check for single-line spans: if the line falls
  within the atom's body range, overlap is 1. This increased enrichment from
  95 to 116 atoms and exact translations from 12/21 to 17/21 on
  libsignal-verify.

## [0.6.2] - 2026-04-16

### Fixed
- **`rust-qualified-name` now reflects the SCIP module chain**: the RQN was
  previously reconstructed from the filesystem path + bare display name, which
  dropped inline module segments (`mod tests`, nested `mod inner {}`). A
  `#[test] fn foo()` inside `mod tests` collided with a free `fn foo()` in the
  same file — both got `crate::foo` instead of `crate::tests::foo` vs
  `crate::foo`. This broke probe-aeneas's RQN-keyed translation matching:
  the test atom could inherit the verified function's `is-disabled: false`
  status, and disambiguation could route the Lean translation to the wrong
  atom, leaving the verified function rendered as unverified.
  `derive_rust_qualified_name` now parses the module chain from the SCIP
  symbol, which is the authoritative source.
- **Charon enrichment no longer collides tests with impl methods**:
  `charon_names` built its lookup key from `(file-derived module, bare
  display_name)`, so a `#[test] fn mul()` in `mod test` and a `Scalar52::mul`
  method in the same file shared a match key — the test atom silently
  inherited Charon's RQN for the real impl method, re-creating the same
  collision at a different layer. The key now uses the atom's SCIP-derived
  RQN, which keeps the `::test::` segment and produces a distinct key.

## [0.6.1] - 2026-04-16

### Fixed
- **`setup` now installs rust-analyzer**: `probe-rust setup` previously only
  checked whether rust-analyzer was present and emitted a warning if missing.
  It now runs `rustup component add rust-analyzer` directly. This fixes
  Docker and CI environments where setup completed "successfully" but
  rust-analyzer was never installed, causing `extract` to fail with
  `Unknown binary 'rust-analyzer'`.
- **`--auto-install` installs rust-analyzer**: `resolve_or_install` now handles
  `Tool::RustAnalyzer` the same way it handles scip and charon — installing
  via rustup when the tool is missing and `--auto-install` is set.

## [0.6.0] - 2026-04-09

### Added
- **`--with-public-api` flag** on `extract`: opt-in override of `is-public-api` using `cargo-public-api` output matched via `rust-qualified-name` (RQN). Provides ground-truth public API surface from `rustdoc` for cross-tool alignment (probe-rust ↔ probe-verus ↔ probe-aeneas). SCIP module-chain walk remains the zero-dependency default.
- **`src/public_api.rs`**: `cargo-public-api` integration with RQN-based matching, blanket impl filtering (`Into`, `TryFrom`, `TryInto`, `Borrow`, `BorrowMut`, `Any`, `ToOwned`, `CloneInto`, `From`), output caching in `<project>/data/public-api.txt`, nightly toolchain and tool auto-install support.
- **Property P17** (public-API override non-fatal): `--with-public-api` failure preserves SCIP-walk values and never aborts the pipeline, analogous to P15 for Charon.
- **14 new unit tests** for RQN extraction (lifetime prefixes, generics, restricted visibility), blanket impl filtering, and enrichment logic.
- **`cargo-public-api (opt-in)`** section in `docs/USAGE.md` documenting resolution, nightly requirement, caching, and failure behavior.

### Changed
- **`docs/SCHEMA.md`**: `is-public-api` field description updated to mention optional `--with-public-api` override. Limitations section expanded with `--with-public-api` subsection.
- **Knowledge Base**: P11 updated for optional override semantics, P14 expanded with `public-api.txt` cache, architecture/glossary updated with new module and terms.

## [0.5.0] - 2026-04-07

### Changed
- **Replaced `cargo-public-api` with SCIP module-chain visibility walk** for `is-public-api` detection. No external tools, no nightly toolchain required. Every internal atom now gets a definitive `true`/`false` (no uncertain bucket).
- **Schema version** bumped from 2.2 to **2.3**.

### Removed
- **`src/public_api.rs`** (655 lines): `cargo-public-api` integration, nightly toolchain code, output parsing, caching, and three-way enrichment logic.
- **`docs/PUBLIC_API_LIMITATIONS.md`**: cargo-public-api-specific analysis (no longer applicable).

### Added
- **Module visibility map** built from SCIP module symbols during call graph construction.
- **`classify_public_api`**: classifies functions as public API by walking ancestor module chain. Trait impl methods classified by implementing type's module chain.
- **`is_library_crate`**: lightweight binary crate detection (moved from deleted `public_api.rs`).
- **22 new tests**: module visibility map, trait impl detection, `classify_public_api` edge cases, `is_function_like_kind` (P7), `is_library_crate` filesystem tests.

## [0.4.0] - 2026-04-07

### Added
- **`is-public-api` field** on all atoms: binary `true`/`false` for internal atoms. Derived from SCIP module-chain visibility walk — no external tools or nightly toolchain required. `true` = function reachable from crate root (pub function with all pub ancestor modules, or trait impl in public module chain). `false` = not public API. Absent only for external stubs. Binary-only crates get `false` for all atoms.
- **`is-public` field from SCIP**: item-level `pub` visibility now derived from SCIP `signature_documentation.text` (always present for internal atoms, no longer requires `--with-charon`).
- **Knowledge Base** (`kb/`): engineering properties (P1-P16), architecture, glossary, and auditor report templates.
- **Auditor skills** (`.cursor/rules/auditors/`): ambiguity, code quality, and test quality auditors for the Ralph Loop development workflow.
- **New tests**: `find_duplicate_code_names` (P2), Charon non-fatal fallback (P15), module visibility map, trait impl detection, `classify_public_api`, `is_library_crate`, `is_function_like_kind` (P7).
- **`SCHEMA_VERSION` constant** in `metadata.rs` for maintainability.

### Changed
- **Schema version** bumped from 2.1 to **2.3** to reflect the new `is-public-api` field.
- **Display name enrichment** now handles both old (`Type#Trait#method`) and new (`impl#[Type][Trait]method`) SCIP symbol formats from rust-analyzer.
- **`CLAUDE.md`** updated with KB references and Ralph Loop workflow.

### Fixed
- **Deterministic output** (P3): `dependencies-with-locations` array is now sorted by `(line, code_name)` to avoid non-deterministic iteration over `HashSet<CalleeInfo>`.

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

[Unreleased]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.6.2...HEAD
[0.6.2]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Beneficial-AI-Foundation/probe-rust/releases/tag/v0.1.0
