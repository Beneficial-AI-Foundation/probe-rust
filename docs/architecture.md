# Architecture

Internal mechanics of probe-rust extraction. Normative CLI/schema live in [USAGE.md](USAGE.md)/[SCHEMA.md](SCHEMA.md).

## Trait-impl disambiguation

When multiple trait impls exist for the same type (e.g. `Add<Scalar>` and
`Add<&Scalar>`), SCIP symbol names can be ambiguous. probe-rust resolves the
match with 4 fallback strategies, applied in order:

1. Signature text matching
2. Self type matching
3. Definition type context
4. Line number fallback

Implemented across `src/commands/extract.rs` and `src/rust_parser.rs`.

## Source-file map

| File | Purpose |
|------|---------|
| `src/commands/extract.rs` | Main extraction pipeline |
| `src/commands/callee_crates.rs` | BFS crate dependency traversal |
| `src/commands/list_functions.rs` | Function enumeration |
| `src/rust_parser.rs` | syn AST visitor for function body spans |
| `src/scip_cache.rs` | SCIP index caching and generation |
| `src/tool_manager.rs` | Auto-download of external tools |
| `src/metadata.rs` | Git + Cargo metadata gathering, envelope construction |
