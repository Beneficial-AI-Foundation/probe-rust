# Testing

## Quick start

```bash
cargo test
```

## Test layers

| Layer | Count | Location | Requires |
|-------|-------|----------|----------|
| Unit tests | 72 | `src/**/*.rs` (`#[cfg(test)]` modules) | Nothing |
| Integration tests | 3 | `tests/extract_check.rs` | Nothing |
| Live extract test | 1 (ignored) | `tests/extract_check.rs` | scip, rust-analyzer |

## Unit tests

72 tests across the library modules:

| Module | Tests | What they cover |
|--------|-------|-----------------|
| `lib.rs` | 16 | Display-name enrichment (impl methods, free functions), external function detection, stub generation, `rust-qualified-name` derivation, code-name normalization, language field defaults |
| `commands/callee_crates.rs` | 13 | Callee collection at various depths, crate grouping, stdlib/custom crate exclusion, function resolution (exact, partial, ambiguous) |
| `charon_names.rs` | 10 | Impl block stripping, bare function names, module-from-code-path, match key derivation, name/type formatting |
| `metadata.rs` | 8 | Envelope wrapping/unwrapping, default output paths, project root discovery, Cargo package info reading |
| `tool_manager.rs` | 7 | Platform mapping, download URLs, binary names, tools directory, version resolution with env overrides |
| `rust_parser.rs` | 5 | Simple function parsing, impl method parsing, bare names, visibility/context, trait impl context |
| `path_utils.rs` | 4 | Source suffix extraction, suffix-based path matching, path match scoring |
| `scip_cache.rs` | 3 | Cache paths, error display, auto-install logic |
| `charon_cache.rs` | 3 | Cache paths, error display, auto-install logic |
| `error.rs` | 2 | Error display formatting, JSON error conversion |

Run only unit tests: `cargo test --lib`

## Integration tests

3 tests in `tests/extract_check.rs` that load the shipped example JSON
and validate it using `probe-extract-check`:

| Test | What it checks |
|------|---------------|
| `example_json_structural_check` | Loads `examples/rust_curve25519-dalek_4.1.3.json` as `AtomEnvelope`, runs `check_all` for envelope fields, line ranges, and referential integrity. Verifies >10 atoms. |
| `example_json_atom_keys_have_probe_prefix` | All atom keys start with `probe:` |
| `example_json_atoms_have_required_fields` | Every atom has non-empty `display-name`, `kind`, and `language` |

These tests need no external tools -- they validate the pre-generated example file.

## Live extract test

1 ignored test that calls `cmd_extract` via the library API:

| Test | What it checks |
|------|---------------|
| `live_extract_structural_check` | Runs extraction on the `rust_micro` fixture from `probe-extract-check`, then validates the output with `check_all` including source-grounded checks. |

**Prerequisites:** `scip` and `rust-analyzer` must be installed.

Run with:

```bash
cargo test -- --include-ignored
```

## CI

`.github/workflows/ci.yml` runs on push/PR to `main`:

1. **Format** -- `cargo fmt --all -- --check`
2. **Clippy** -- `cargo clippy --all-targets -- -D warnings`
3. **Test** -- `cargo test --verbose` (all tests except `#[ignore]`)

The CI checks out the sibling `probe` repo alongside for the
`probe-extract-check` dev-dependency.

## Adding tests

- **Unit tests:** add to the `#[cfg(test)] mod tests` block in the relevant `src/` module.
- **Integration tests:** add to `tests/extract_check.rs`. Use `probe_extract_check::{check_all, load_extract_json}` for structural validation.
- **New example JSON:** place in `examples/` and add corresponding test assertions.
