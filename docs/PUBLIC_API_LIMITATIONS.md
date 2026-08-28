# Public API Detection: Known Limitations

Version: 3.0
Date: 2026-08-28

## Overview

`probe-rust extract --with-public-api` detects public API membership by
cross-referencing function atoms (from SCIP/rust-analyzer) against the output of
`cargo public-api`. This document catalogues the public API functions that
cannot be matched to a probe atom, and explains why.

All examples below are drawn from `curve25519-dalek` 4.1.3 (member directory,
not the workspace root).

### The two metrics

The extract output prints both, and they are deliberately different numbers:

| Metric | Output line | Counts |
|---|---|---|
| **Atoms marked** | `is-public-api: N true, M false` | probe atoms whose `is-public-api` is `true` / `false` |
| **Entries matched** | `public-api entries matched: N/M` | `cargo public-api` entries backed by at least one atom |

They cannot be added or compared: several entries can share one atom (the five
macro-generated basepoint tables all inherit one trait body), and one entry can
be reachable through several candidate name forms. Improving one does not imply
improving the other — a pass that resolves an entry to an atom already marked
`true` moves the entry count and nothing else.

Earlier versions of this document conflated the two, reporting "130/169 matched,
46 unmatched" (130 + 46 = 176 ≠ 169). 130 was the *atom* count; the entry count
at that time was 123/169.

### Match statistics

| Metric | Before (0.10.0) | Now |
|---|---|---|
| Public API entries detected by `cargo public-api` | 169 | 169 |
| **Entries matched** | 123 (73%) | **147 (87%)** |
| Entries unmatched | 46 | **22** (13%) |
| **Atoms marked `is-public-api: true`** | 130 | **142** |
| Atoms marked `is-public-api: false` | 400 | 399 |
| Atoms with `is-public-api: null` | 273 | 262 |

Of the 12 newly-marked atoms, 11 are macro-generated impl-evidence atoms that
previously carried no verdict at all (`null`), and one — the crate's own blanket
impl `impl<T> IsIdentity for T` — was actively **mislabeled `false`**. It was the
only genuine mislabeling in the set.

The 22 remaining unmatched entries fall into two structural categories. There
are zero unexplained gaps.

---

## Category A: Macro-generated functions with no atom (19 entries)

### Root cause

`impl_basepoint_table!` (`src/edwards.rs:912`) generates one basepoint table
type per radix. `cargo public-api` reports every instantiation's methods as
distinct public API entries, but rust-analyzer's SCIP index has no symbol
definition for most of them: the macro body is indexed once, and the type name
is a macro parameter, absent from the SCIP symbol string.

For *some* of these methods SCIP does emit an impl-evidence symbol — a bodyless
atom with no `rust-qualified-name` — and those are now resolved (see
[Resolved](#what-is-now-resolved) below). The 19 entries here have no atom of
any kind.

### Affected functions

| Method | Types |
|---|---|
| `fmt` | `EdwardsBasepointTable`, `Radix32`, `Radix64`, `Radix128`, `Radix256` |
| `from` | `Radix16`, `Radix32`, `Radix64`, `Radix128`, `Radix256` |
| `mul_base` | `EdwardsBasepointTable`, `Radix32`, `Radix64`, `Radix128`, `Radix256` |
| `basepoint` | `Radix32`, `Radix64`, `Radix128`, `Radix256` |

### Why this cannot be resolved

There is nothing to match against and nothing to mark. Resolving these would
require macro-expansion-aware indexing, i.e. a change in SCIP/rust-analyzer, not
in probe-rust. Inventing atoms for them would fabricate call-graph nodes with no
body, no span, and no callees.

---

## Category B: Bodyless required trait signatures (3 entries)

### Root cause

A trait method *without* a default body has no body to index. When SCIP records
no definition occurrence for the signature, the atom that exists (if any) is an
empty stub: no RQN, no `code-path`, no span — the same shape as an external
stub (P4).

### Affected functions

| Public API qualified name | Declaration | Atom |
|---|---|---|
| `curve25519_dalek::traits::BasepointTable::basepoint` | `src/traits.rs:59`, required | none |
| `curve25519_dalek::traits::BasepointTable::mul_base` | `src/traits.rs:62`, required | bodyless stub, no RQN |
| `curve25519_dalek::traits::VartimePrecomputedMultiscalarMul::optional_mixed_multiscalar_mul` | `src/traits.rs:388`, required | bodyless stub, no RQN |

Note the contrast with the same traits' *default* methods, which are indexed
normally and matched: `BasepointTable::mul_base_clamped` (`src/traits.rs:66`,
default body), `VartimePrecomputedMultiscalarMul::vartime_multiscalar_mul`
(`:316`) and `::vartime_mixed_multiscalar_mul` (`:347`).

### Possible future mitigation

The two bodyless stubs could be marked from their entries the same way
impl-evidence atoms are, by extending impl-descriptor resolution to trait-side
key descriptors (`…/traits/Trait#method()`). Deliberately not done: those atoms
are indistinguishable in shape from external stubs (empty `code-path`), so
marking them trades a small entry-count gain for a weaker guarantee about what
`is-public-api: true` means. `BasepointTable::basepoint` has no atom at all and
is unreachable in any case.

---

## What is now resolved

Three mechanisms cover what earlier versions of this document listed as
unfixable. Each is additive and skips on ambiguity; see
[P11](../kb/engineering/properties.md#p11--is-public-api-from-scip-module-walk).

### Inherited default trait methods (11 entries)

When a concrete type does not override a trait method with a default body, the
body exists once — in the trait. The trait's atom is the entry's only
implementation, so the entry is matched to it:

| Entries | Resolved to |
|---|---|
| `{EdwardsBasepointTable, Radix32, Radix64, Radix128, Radix256}::mul_base_clamped` | `traits::BasepointTable::mul_base_clamped` |
| `{EdwardsPoint, RistrettoPoint}::vartime_multiscalar_mul` | `traits::VartimeMultiscalarMul::vartime_multiscalar_mul` |
| `Vartime{Edwards,Ristretto}Precomputation::vartime_{,mixed_}multiscalar_mul` | `traits::VartimePrecomputedMultiscalarMul::vartime_{,mixed_}multiscalar_mul` |

Guards: no atom may define `Type::method` itself (an override means the default
is not what runs), exactly one trait providing the method may be in the type's
impl set, exactly one atom may answer to `Trait::method`, and that atom must
itself be public API. **This moves no atom's tag** — the trait atoms were
already `true`. It is an entry-count-only fix, which is exactly why the two
metrics are reported separately.

### Macro-generated impl-evidence atoms (11 atoms, 11 entries)

Where SCIP does emit a symbol for a macro-generated impl method, the atom is
bodyless with no RQN, so no name can match it. It is instead resolved from the
entry through the SCIP impl descriptor in its key
(`…/edwards/impl#[EdwardsBasepointTableRadix32][BasepointTable]create()`) and
marked `true` directly:
`{EdwardsBasepointTable, Radix32, Radix64, Radix128, Radix256}::{create, mul}`
plus `EdwardsBasepointTable::basepoint`. Restricted to the analyzed crate's own
atoms; a non-unique descriptor never resolves.

### The crate's own blanket impl (1 atom, 2 entries)

`impl<T> IsIdentity for T` (`src/traits.rs:41`) yields the atom
`…/traits/&T#impl<bool>#[T][IsIdentity]is_identity()`, whose RQN is
`curve25519_dalek::traits::T::is_identity` — a name no public entry carries, so
it was marked `false`. `cargo public-api` reports it twice, as `T::is_identity`
and as `curve25519_dalek::traits::IsIdentity::is_identity`; both now resolve to
this atom, and it is marked `true`.

Blanket-ness is verified with `syn` against the source (the impl self type must
be one of the impl's own generic parameters), never inferred from the key: a
concrete type genuinely named `T` must not be resolved this way.

---

## Corrections to earlier versions of this document

Versions ≤ 2.2 contained four factual errors, each verified against pristine
crates.io 4.1.3 sources:

1. **Alias direction was inverted.** `src/edwards.rs:1112` reads
   `pub type EdwardsBasepointTableRadix16 = EdwardsBasepointTable;` —
   `EdwardsBasepointTable` is the macro-generated struct and `…Radix16` is the
   alias, not the reverse. The old "Category B: Type Alias Re-Exports" table had
   it backwards, and `cargo public-api`'s own output agrees with the source
   (`data/public-api.txt:288`).
2. **`Vartime{Edwards,Ristretto}Precomputation` are not aliases.** They are
   `pub struct` newtypes over `backend::VartimePrecomputedStraus`
   (`src/edwards.rs:868`, `src/ristretto.rs:1014`). Their impls provide only the
   required `new` / `optional_mixed_multiscalar_mul`, so their `vartime_*`
   entries are inherited trait defaults — Category C, not B.
3. **`BasepointTable::{create, basepoint, mul_base}` are required methods**
   (`src/traits.rs:56`, `:59`, `:62`), not default methods. Only
   `mul_base_clamped` (`:66`) has a default body. The old Category C table
   labeled the required ones as defaults.
4. **The statistics conflated atoms with entries** (see
   [The two metrics](#the-two-metrics)).

A consequence of (1) and (2): a `pub type` alias-mapping pass — the mitigation
the old document proposed for Category B — would have fixed **zero** entries on
this crate, because the aliased name is the one SCIP indexes.

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
