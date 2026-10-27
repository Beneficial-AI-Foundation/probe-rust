---
auditor: code-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB, working tree on fix/impl-header-candidate-filter)
scope: branch fix/impl-header-candidate-filter vs main, plus the uncommitted working-tree change on top of 3b6b773 (`impl_header_has_const_expression`: a method of an impl whose trait path or self type holds a non-literal const expression records no impl header), eighth round
status: 0 critical, 0 warnings, 0 open info (I1 fixed; I2 accepted)
---

Validation: `cargo fmt -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 271 passed (1 ignored).

KB revision: the P20 impl-header text is an amendment on this branch. On `main`, P20 has no impl-header stage at all. The code and the amended P20 are in the same pull request. Thus this is not a cross-PR merge-order constraint. But the P20 results below hold only against the branch text.

## Info

### [I1] The const-expression rule also drops headers whose expression names no type
- **Location**: src/rust_parser.rs:436-452 (`impl_header_has_const_expression`), called at src/rust_parser.rs:368
- **Issue**: the rule fires on every non-literal expression, including a bare const parameter or const item (`[Foo; N]`, `[u8; SIZE]`) and `{ N + 1 }`. The KB rationale (P20, glossary "Impl-header evidence") is that a *type* in the expression contradicts the correct candidate. A bare `N` or `SIZE` names no type. `scip_type_symbol_to_path` keeps only type descriptors, so these symbols never became header paths. Also, Charon keeps a const parameter by name: the SPQR manifest has `{impl core::cmp::PartialEq<[U; N]> for [T]}`. For `impl<const N: usize> From<[Foo; N]> for X`, the change discards the `Foo` evidence and falls back to span alone.
- **Evidence**: `Finder::visit_expr` sets the flag for any `Expr` that is not `Expr::Lit`. `spqr-main-extract/translation.json` keeps `[U; N]` as written.
- **Recommendation**: none needed for correctness. The rule only removes evidence, so it cannot add a wrong `charon-def-id`. It returns these impls to the pre-0.12.1 span behavior. Code, doc comment, P20, glossary, SCHEMA and CHANGELOG all say "non-literal const expression", so they agree. To get more precision later, narrow the rule to expressions that contain a type (a turbofish or a cast), or skip `Expr::Path`. I found no such impl in the SPQR, SymCRust or dalek manifests. Thus the practical cost is not measured.

### [I2] The "Charon renders the evaluated value" claim is not checked against a real manifest
- **Location**: src/rust_parser.rs:41-44, kb/engineering/properties.md:198, kb/engineering/glossary.md:69, CHANGELOG.md:11
- **Issue**: all four say that Charon renders `[u8; size_of::<Foo>()]` as `[u8; 4usize]`. The tests use this rendering as a fixture (src/charon_names.rs:3600). I found no manifest with an impl segment that shows an evaluated const-expression length. Literal lengths do appear as `32usize` (dalek manifest, `FnOnce<([u8; 32usize],), …>`), which fits the claim but does not prove it.
- **Evidence**: grep of every `translation.json` under `../` for `; <digits>usize]` impl segments found only the literal-length dalek case.
- **Recommendation**: none required. If Charon keeps the expression instead, the dropped header costs only evidence, not correctness (see I1). If a crate shows this rendering, name it in the CHANGELOG or P20 as the source of the claim.

## Verified clean

- P20, header drop: `visit_item_impl` (src/rust_parser.rs:367-374) pushes `ImplHeaderFrame::default()` for a const-expression impl. Its methods get empty `impl_header` and `impl_header_path_count: None` from `current_impl_header`. `push_function` records no header key for them, so they do not take part in the `cfg`-split repeat count. The early-return path still applies the impl's `#[cfg]` attributes through `with_enclosing_cfg`, so P18 `cfg` folding is unchanged.
- P20, downstream: with an empty `impl_header`, lib.rs (src/lib.rs:1556-1564) sets `impl_header_paths` empty and `impl_header_complete` false. `header_evidence` then returns `Compatible`. The header stage drops nothing, and the lone-candidate path needs the span. This matches the P20 and glossary text and the SCHEMA `charon-def-id` row ("carries no header ... so they still need the span").
- `Finder` scans the trait path (its generic arguments, including `GenericArgument::Const`) and the self type (array lengths, nested arguments). It does not look at the impl's own generics or where clause, which are not header ranges. Literal lengths (`[u8; 4]`) keep the header, as the doc comment and the test say.
- Tests: `test_impl_header_dropped_for_const_expression` (src/rust_parser.rs:930) covers a path-only header kept, a `size_of::<Foo>()` length dropped (ranges and path count), a literal length kept, and a `{ N + 1 }` const argument dropped. `test_resolve_const_expression_header_left_to_span` (src/charon_names.rs:3600) pins that with no header paths the Manifest span picks the evaluated-length impl over a shifted `From<(Foo, u8)>` sibling. The atom fixture sets `impl_header_complete = false`, as lib.rs produces for an empty header.
- Docs agree. These all describe the same rule with the same example: the `FunctionSpan::impl_header` doc comment, P20 (kb/engineering/properties.md:198), the P20 "Where" list, the glossary "Impl-header evidence" entry, the docs/SCHEMA.md `charon-def-id` row and the CHANGELOG 0.12.1 entry. Cargo.toml is 0.12.1 and `main` is 0.12.0, so the CHANGELOG entry is unreleased and amending it is correct. kb/engineering/architecture.md does not list the header drop conditions, so it needs no change.
- P3: no new serialized collection. `Finder` holds a `bool`.
- P1-P19, C1-C3: this change does not touch them.

## Resolution

- I1: fixed in the same round. A single name (`[Foo; N]`, `[u8; SIZE]`) now keeps its header; only other non-literal expressions drop it. The parser test pins `[u8; N]` (kept) and `[u8; Foo::LEN]` (dropped).
- I2: accepted. The `[u8; 4usize]` rendering follows Charon's evaluation of anonymous constants (`hax/constant_utils/uneval.rs`) and the `32usize` literals in the dalek manifest. If Charon kept the expression, the dropped header would cost evidence only, not correctness.
