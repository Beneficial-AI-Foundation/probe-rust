---
auditor: test-quality-auditor
date: 2026-10-27
repo: probe-rust
kb: kb/ (probe-rust own KB), P20 as amended in the working tree
scope: branch fix/impl-header-candidate-filter, uncommitted change on top of 3b6b773. `rust_parser` drops the impl header when the trait path or self type holds a non-literal const expression (`impl_header_has_const_expression`). Only the touched properties were re-judged.
status: 0 critical, 0 open (2 warnings and 1 info fixed in this round)
---

`cargo test`: 271 passed (268 unit, 3 integration), 1 ignored.

The P20 text is amended in the same uncommitted change (kb/engineering/properties.md: the sentence on non-literal const expressions, and `impl_header_has_const_expression` in **Where**). The tests were judged against the amended text, so the code and the KB edit must land in the same pull request.

Each rule was checked by mutation on a scratch copy of the repo in /tmp. Results:
- Revert the rule (never take the early return in `visit_item_impl`): `test_impl_header_dropped_for_const_expression` fails.
- Count literals as const expressions too (`if true`): the same test fails (method `c`, `[u8; 4]`, must keep its header).
- Skip the trait path in `impl_header_has_const_expression`: the same test fails (methods `b` and `d` hold the expression in the trait path).
- Skip the self type: no test fails (W1).
- Drop `with_enclosing_cfg` in the early-return branch: no test fails (W2).
- Do not push an empty `ImplHeaderFrame` in the early-return branch: no test fails (I1).

`test_resolve_const_expression_header_left_to_span` (charon_names.rs) builds the atom by hand with no header paths, so it does not depend on `rust_parser` and does not guard the rule. It documents the outcome the rule enables (the span picks the `[u8; 4usize]` candidate). The rule guard is `test_impl_header_dropped_for_const_expression`.

## Warnings

### [W1] Const expression in the self type is not tested
- **Location**: src/rust_parser.rs, `impl_header_has_const_expression` (`finder.visit_type(&node.self_ty)`); test `test_impl_header_dropped_for_const_expression`
- **Issue**: The doc comment and P20 cover a const expression anywhere in the header, "trait path or self type". Every const-expression case in the test is in the trait path (`From<[u8; size_of::<Foo>()]>`, `From<Bits<{ N + 1 }>>`). Removing the self-type visit leaves all tests passing.
- **Evidence**: mutation `finder.visit_type(&node.self_ty)` -> `let _ = &node.self_ty;`. Result: 268 passed, 0 failed.
- **Recommendation**: Add a method under an impl such as `impl Tr for [u8; core::mem::size_of::<Foo>()]` (or `impl X<{ N + 1 }>`) to the test and assert its `impl_header` is empty.

### [W2] `cfg` of an impl with a const-expression header is not tested (P18)
- **Location**: src/rust_parser.rs, early-return branch of `visit_item_impl`
- **Issue**: The new branch repeats `with_enclosing_cfg(&node.attrs, ...)` so that methods of a `#[cfg(...)]` impl keep the impl's gate. No test covers a gated impl with a const-expression header. If the call is dropped, the methods lose a visible gate, which P18 ("as complete as visible") forbids.
- **Evidence**: mutation that replaces `v.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_impl(v, node))` with `syn::visit::visit_item_impl(v, node)`. Result: 268 passed, 0 failed.
- **Recommendation**: Put `#[cfg(feature = "x")]` on one const-expression impl in the test and assert the method's `cfg` is `Some("feature = \"x\"")` (or the format the other cfg tests use).

## Info

### [I1] Empty frame push in the early-return branch is not pinned
- **Location**: src/rust_parser.rs, early-return branch of `visit_item_impl` (`self.with_impl_header(ImplHeaderFrame::default(), ...)`)
- **Issue**: Without the push, a method of the const-expression impl reads the enclosing frame. For a top-level impl that frame is empty, so the result is the same, and no test fails. The result changes only for an impl nested inside another impl's method body. There, the inner methods get the outer header.
- **Evidence**: mutation that replaces the `with_impl_header` wrapper with a bare `with_enclosing_cfg` call. Result: 268 passed, 0 failed.
- **Recommendation**: Optional. A nested case can pin it: `impl A for B { fn f() { impl From<[u8; size_of::<Foo>()]> for X { fn g() {} } } }`, then assert that `g` has no header. The shape is rare, so the value is low.

## Previous findings

- None open. The previous round closed clean.

## Coverage matrix

| Property | Tests | Coverage | Notes |
|----------|-------|----------|-------|
| P1-P11 | existing suites | Not re-audited | Untouched |
| P12 | n/a | n/a | Retired |
| P13-P17 | existing suites | Not re-audited | Untouched |
| P18 | existing cfg suites | Partial for the new branch | W2: the cfg of a const-expression impl is untested |
| P19 | existing suites | Not re-audited | Untouched |
| P20 | test_impl_header_dropped_for_const_expression, test_resolve_const_expression_header_left_to_span, existing P20 tests | Partial | Revert, literal and trait-path mutations fail a test. The self-type mutation does not (W1) |
| C1-C3 | existing | Not re-audited | Untouched |

## Verified clean

- P20 const-expression rule, trait-path side. Put `size_of::<Foo>()` or `{ N + 1 }` in the trait path, and the method records no header. Its `impl_header_path_count` is `None`. Reverting the rule fails `test_impl_header_dropped_for_const_expression`.
- Literal array lengths (`[u8; 4]`) keep the header. Treating literals as const expressions fails the same test.
- An ordinary sibling impl (`From<(Foo, u8)>`) keeps its header.
- With no header paths, the resolver falls back to the span and picks the correct `[u8; 4usize]` candidate over the line-shifted `From<(c::Foo, u8)>` sibling (`test_resolve_const_expression_header_left_to_span`).
- P20 rules from earlier rounds keep their tests. No existing test changed.

## Resolution

- W1: `test_impl_header_dropped_for_const_expression` now has `impl Tr for [u8; core::mem::size_of::<Foo>()]` and asserts its method has no header.
- W2: the `Foo::LEN` impl carries `#[cfg(feature = "x")]`, and the test asserts its method keeps that `cfg`.
- I1: the test nests a const-expression impl inside another impl's method and asserts the nested method does not inherit the outer header.
