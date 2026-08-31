# Public API Detection: Known Limitations

Version: 3.1
Date: 2026-08-28

## Overview

`probe-rust extract --with-public-api` detects public API membership by
cross-referencing function atoms (from SCIP/rust-analyzer) against the output of
`cargo public-api`. Matching runs in additive passes with ambiguity guards,
specified canonically in
[P11](../kb/engineering/properties.md#p11--is-public-api-from-scip-module-walk);
this document catalogues the entries that remain unmatched and why. Examples are
drawn from `curve25519-dalek` 4.1.3 (member directory, not the workspace root).

### The two metrics

The extract output prints both, and they are deliberately different numbers:

| Metric | Output line | Counts |
|---|---|---|
| **Atoms marked** | `is-public-api: N true, M false` | probe atoms whose `is-public-api` is `true` / `false` |
| **Entries matched** | `public-api entries matched: N/M` | `cargo public-api` entries backed by at least one atom |

They cannot be added or compared: several entries can share one atom (the five
macro-generated basepoint tables all inherit one trait body), and one entry can
be reachable through several candidate name forms. Improving one does not imply
improving the other.

### Match statistics (curve25519-dalek 4.1.3)

| Metric | Before (0.10.0) | Now |
|---|---|---|
| **Entries matched** | 123 (73%) | **147 (87%)** |
| Entries unmatched | 46 | **22** |
| **Atoms marked `is-public-api: true`** | 130 | **142** |

The "Before" and "Now" atom counts are computed under slightly different
semantics: 0.10.0 counted what the RQN enrichment pass wrote, while the current
count is the final state of every atom (including SCIP-walk values kept on
atoms the override never touched). On this crate the two agree, but the
comparison is not exact by construction.

The 22 remaining unmatched entries fall into the two structural categories
below. There are zero unexplained gaps.

> **Historical note — why there is no type-alias pass.** Versions ≤ 2.2 of this
> document proposed mapping `pub type` aliases to their underlying types
> ("Category B: Type Alias Re-Exports"), with the alias direction inverted:
> `src/edwards.rs:1112` actually reads
> `pub type EdwardsBasepointTableRadix16 = EdwardsBasepointTable;` — the
> macro-generated struct carries the name `cargo public-api` reports, and the
> `…Radix16` alias is the derived one. An alias-mapping pass would therefore
> have resolved zero entries on this crate, and was dropped; the entries it was
> meant to cover are handled by the trait-default and impl-descriptor passes
> instead.

---

## Category A: Macro-generated functions with no atom (19 entries)

`impl_basepoint_table!` (`src/edwards.rs:912`) generates one basepoint table
type per radix. `cargo public-api` reports every instantiation's methods as
distinct public API entries, but the SCIP index has no symbol definition for
most of them: the macro body is indexed once, and the type name is a macro
parameter, absent from the SCIP symbol string. Where SCIP does emit an
impl-evidence symbol, the entry now resolves through the impl descriptor
(P11 pass 4); the 19 entries here have no atom of any kind.

Resolving these would require macro-expansion-aware indexing, i.e. a change in
SCIP/rust-analyzer, not in probe-rust. Inventing atoms for them would fabricate
call-graph nodes with no body, no span, and no callees.

---

## Category B: Bodyless required trait signatures (3 entries)

A trait method *without* a default body has no body to index. When SCIP records
no definition occurrence for the signature, the atom that exists (if any) is an
empty stub: no RQN, no `code-path`, no span — the same shape as an external
stub. Affected (all required methods, `src/traits.rs`):
`BasepointTable::basepoint`, `BasepointTable::mul_base`, and
`VartimePrecomputedMultiscalarMul::optional_mixed_multiscalar_mul`.

The two bodyless stubs could be marked from their entries by extending
impl-descriptor resolution to trait-side key descriptors. Deliberately not
done: those atoms are indistinguishable in shape from external stubs (empty
`code-path`), so marking them trades a small entry-count gain for a weaker
guarantee about what `is-public-api: true` means. `basepoint` has no atom at
all and is unreachable in any case.

---

## Summary

| Category | Count | Root cause | Fixable? |
|---|---|---|---|
| A — macro-generated, no atom | 19 | SCIP indexes the macro body once; no symbol per instantiation | No (SCIP limitation) |
| B — bodyless required trait signatures | 3 | No body to index; atom (if any) is an empty stub | Not worth the guarantee it costs |
| **Total unmatched** | **22** | | |

Both remaining categories are structural limitations of the SCIP representation
produced by rust-analyzer, not defects in the matching logic. Every entry that a
probe atom can honestly answer for is now matched, and every atom that a public
entry proves public is now marked.
