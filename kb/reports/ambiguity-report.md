---
auditor: ambiguity-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB. The hub ../probe/kb/ consulted for cross-repo contracts only)
scope: branch fix/impl-header-candidate-filter, uncommitted working tree on top of 3b6b773 (a method of an impl whose header holds a non-literal const expression carries no header paths. Touches P20, glossary "Impl-header evidence", docs/SCHEMA.md, CHANGELOG.md and the `FunctionSpan::impl_header` rustdoc)
status: 0 critical, 0 open (1 warning and 1 info fixed in this round)
---

KB revision: audited on the feature branch working tree. The P20, glossary,
SCHEMA and CHANGELOG edits ship in the same pull request as the code, so there
is no cross-PR merge-order constraint.

The five passages agree on the rule and on its reason (Charon renders the
evaluated value). The examples are the only definition of "non-literal
const expression". They do not show that a bare const parameter in an array
length also counts. One SCHEMA phrase has the wrong subject.

## Warnings

### [W1] "Non-literal const expression" is defined only by examples, and the examples hide the common case
- **Location**: kb/engineering/properties.md:198 (P20), kb/engineering/glossary.md:69 ("Impl-header evidence"), docs/SCHEMA.md:174 (`charon-def-id`), CHANGELOG.md:11, src/rust_parser.rs:41-45 (`FunctionSpan::impl_header`) and src/rust_parser.rs (`impl_header_has_const_expression`)
- **Issue**: If any `syn::Expr` in the trait path or self type is not `Expr::Lit`, the code drops the header. All five documents explain the term with computed expressions only (`size_of::<Foo>()`, `{ N + 1 }`). By the code, three more forms count as non-literal. The first is a const parameter as an array length (`[u8; N]`, an `Expr::Path`). The second is a braced literal (`{ 4 }`, an `Expr::Block`). The third is a negative number (`-1`, an `Expr::Unary`). A reader of P20 expects `impl From<[u8; N]> for X` to keep its header, because `N` names no type and Charon does not evaluate it away. In a generic argument without braces (`Bits<N>`), `syn` parses the const parameter as a type. So `Bits<N>` keeps the header, but `[u8; N]` drops it. The rule is conservative (a dropped header only sends the method to the span stage), so this does not cause wrong matches. But the behavior is not in the documents, and it loses header evidence for every impl over `[T; N]`.
- **Recommendation**: Define the term once in P20. Proposed text: "any expression in the trait path or self type other than one literal token. This includes a const parameter as an array length (`[u8; N]`), a braced literal (`{ 4 }`) and a negative number (`-1`)." Point the glossary, SCHEMA and the rustdoc at P20, or use the same words there. To keep the header of `[u8; N]`, change the code to accept an `Expr::Path` with one segment, and document that instead.

## Info

### [I1] SCHEMA says impl headers carry no header
- **Location**: docs/SCHEMA.md:174 (`charon-def-id` row)
- **Issue**: The new parenthesis reads "`cfg`-split methods and impl headers with a non-literal const expression carry no header". The subject of the second item is the header itself, so the sentence says that a header carries no header. P20, the glossary, CHANGELOG and the rustdoc all say that the *method* carries no header.
- **Recommendation**: "(`cfg`-split methods, and methods of an impl whose header holds a non-literal const expression, carry no header, so they still need the span)".

## Verified clean

- The rule is the same in P20 (properties.md:198), the glossary (glossary.md:69), SCHEMA (`charon-def-id` row), CHANGELOG 0.12.1 and the `FunctionSpan::impl_header` rustdoc: the method carries no header paths, because Charon renders the evaluated value, and the span decides. The `[u8; 4usize]` rendering example matches `test_resolve_const_expression_header_left_to_span` (src/charon_names.rs) and `test_impl_header_dropped_for_const_expression` (src/rust_parser.rs).
- The code matches the text. `impl_header_has_const_expression` scans only the trait path and self type, which are the header ranges. A dropped header also leaves `impl_header_path_count` at `None`. The `impl_header_path_count` rustdoc covers this case ("or when `impl_header` is empty").
- No passage lists the no-header cases incompletely. P20, the glossary, SCHEMA, CHANGELOG and the rustdoc each name both the `cfg`-split case and the const-expression case. In P20, "carries header paths only when its method name occurs once" is a necessary condition. The next sentence adds the const-expression case with "either". The two do not conflict.
- Three passages name only the `cfg`-split case, but none claims to list all no-header cases. `drop_repeated_impl_headers` (src/rust_parser.rs:215-217) and the test comment at src/rust_parser.rs:918 describe only that mechanism. `is_trait_impl_segment` (src/charon_names.rs:840-845) gives the `cfg`-split drop as the reason that an exact header is unique. The const-expression drop does not change that reason. The glossary "Known limits" (macro-generated impls carry no header) is a separate, still-accurate case.
- architecture.md:100 and the code-quality auditor rule (.cursor/rules/auditors/code-quality-auditor.md:33) describe the header stage without listing no-header cases, so they need no change.
- P20 "Where" names `impl_header_has_const_expression`, and the function exists in src/rust_parser.rs.
- `last-updated` of properties.md, glossary.md and architecture.md is 2026-10-27. kb/index.md (2026-09-01) and kb/engineering/index.md (2026-08-11) are older than 30 days. This change does not touch what they describe.

## Resolution

- W1: P20 now defines a non-literal const expression (anything other than a literal or a single name), and the glossary, CHANGELOG-adjacent rustdoc and code match. The code now keeps the header for a single name such as `[u8; N]`, so the `Bits<N>` / `[u8; N]` inconsistency is gone. A test pins `[u8; N]` (kept) and `[u8; Foo::LEN]` (dropped).
- I1: SCHEMA now says "methods of an impl whose header holds a non-literal const expression".
