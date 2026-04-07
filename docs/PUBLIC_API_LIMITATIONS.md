# Public API Detection: Known Limitations

Version: 2.2
Date: 2026-04-07

## Overview

`probe-rust extract` detects public API membership by cross-referencing function
atoms (from SCIP/rust-analyzer) against the output of `cargo public-api`. This
document catalogues public API functions that cannot be matched to a probe atom,
and explains why.

All examples below are drawn from `curve25519-dalek` 4.1.3.

### Match statistics

| Metric | Count |
|---|---|
| Public API functions detected by `cargo public-api` | 169 |
| Matched to probe atoms (`is-public-api: true`) | 130 (77%) |
| Unmatched | 46 (23%) — see categories below |
| Probe atoms marked `is-public-api: false` | 296 |
| Probe atoms marked `is-public-api: null` (uncertain) | 58 |

All 46 unmatched entries fall into three structural categories. There are zero
unexplained gaps.

---

## Category A: Macro-Generated Types (29 functions)

### Root cause

The `define_basepoint_table!` macro generates five concrete types
(`EdwardsBasepointTableRadix16`, `Radix32`, `Radix64`, `Radix128`, `Radix256`),
each with the same set of methods. `cargo public-api` reports every
monomorphization as a distinct public API entry. However, rust-analyzer (SCIP)
indexes the macro body once — it does not produce separate symbol definitions
for each instantiation.

### Affected functions

| Type | Methods |
|---|---|
| `EdwardsBasepointTableRadix16` | `from` |
| `EdwardsBasepointTableRadix32` | `basepoint`, `create`, `fmt`, `from`, `mul`, `mul_base`, `mul_base_clamped` |
| `EdwardsBasepointTableRadix64` | `basepoint`, `create`, `fmt`, `from`, `mul`, `mul_base`, `mul_base_clamped` |
| `EdwardsBasepointTableRadix128` | `basepoint`, `create`, `fmt`, `from`, `mul`, `mul_base`, `mul_base_clamped` |
| `EdwardsBasepointTableRadix256` | `basepoint`, `create`, `fmt`, `from`, `mul`, `mul_base`, `mul_base_clamped` |

### Why this cannot be resolved

SCIP has no representation for macro-expanded code at the monomorphized level.
The macro body's single symbol cannot be mapped to all five concrete types
because the type name is a parameter of the macro, not present in the SCIP
symbol string.

---

## Category B: Type Alias Re-Exports (10 functions)

### Root cause

The public API surface includes type aliases that map to internal types:

| Public alias | Underlying type |
|---|---|
| `EdwardsBasepointTable` | `EdwardsBasepointTableRadix16` |
| `VartimeEdwardsPrecomputation` | `VartimePrecomputedStraus` |
| `VartimeRistrettoPrecomputation` | `VartimePrecomputedStraus` |

`cargo public-api` reports methods under the alias name. SCIP indexes symbols
under the underlying definition type. Since the qualified names differ, they
cannot be matched.

### Affected functions

| Public API qualified name |
|---|
| `curve25519_dalek::edwards::EdwardsBasepointTable::basepoint` |
| `curve25519_dalek::edwards::EdwardsBasepointTable::create` |
| `curve25519_dalek::edwards::EdwardsBasepointTable::fmt` |
| `curve25519_dalek::edwards::EdwardsBasepointTable::mul` |
| `curve25519_dalek::edwards::EdwardsBasepointTable::mul_base` |
| `curve25519_dalek::edwards::EdwardsBasepointTable::mul_base_clamped` |
| `curve25519_dalek::edwards::VartimeEdwardsPrecomputation::vartime_mixed_multiscalar_mul` |
| `curve25519_dalek::edwards::VartimeEdwardsPrecomputation::vartime_multiscalar_mul` |
| `curve25519_dalek::ristretto::VartimeRistrettoPrecomputation::vartime_mixed_multiscalar_mul` |
| `curve25519_dalek::ristretto::VartimeRistrettoPrecomputation::vartime_multiscalar_mul` |

### Possible future mitigation

Parse `pub type` lines from `cargo public-api` output and build an alias → real
type mapping. Then try matching atoms using both the alias path and the
definition path.

---

## Category C: Default Trait Methods (7 functions)

### Root cause

Traits can provide default method implementations. When a concrete type does not
override the default, the method body only exists in the trait definition. SCIP
does not create a separate function symbol for each concrete type that inherits
the default — it only indexes the trait's own definition.

`cargo public-api` lists these methods under both the trait name and each
concrete implementing type, producing entries that have no corresponding atom.

### Affected functions

| Public API qualified name | Explanation |
|---|---|
| `T::is_identity` | Generic bound artifact (`where T: IsIdentity`) |
| `curve25519_dalek::traits::IsIdentity::is_identity` | Default trait method |
| `curve25519_dalek::traits::BasepointTable::basepoint` | Default trait method |
| `curve25519_dalek::traits::BasepointTable::mul_base` | Default trait method |
| `curve25519_dalek::traits::VartimePrecomputedMultiscalarMul::optional_mixed_multiscalar_mul` | Default trait method |
| `curve25519_dalek::edwards::EdwardsPoint::vartime_multiscalar_mul` | Inherited default on concrete type |
| `curve25519_dalek::ristretto::RistrettoPoint::vartime_multiscalar_mul` | Inherited default on concrete type |

### Possible future mitigation

When a public API function is `Trait::method` and there is an atom for the same
trait method defined in the trait source file, match them. For concrete-type
entries (`EdwardsPoint::vartime_multiscalar_mul`), resolve through the trait
impl chain.

---

## Worked Example: `mul_base_clamped`

To illustrate how the matching works end-to-end, here is the full picture for a
single function name that appears across traits, concrete types, macro
instantiations, and type aliases.

### Probe atoms (5 total)

| display-name | RQN | Location | is-public-api |
|---|---|---|---|
| `EdwardsPoint::mul_base_clamped` | `curve25519_dalek::edwards::EdwardsPoint::mul_base_clamped` | `src/edwards.rs:778–786` | **true** |
| `MontgomeryPoint::mul_base_clamped` | `curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped` | `src/montgomery.rs:144–152` | **true** |
| `BasepointTable::mul_base_clamped` | `curve25519_dalek::traits::BasepointTable::mul_base_clamped` | `src/traits.rs:66–74` | **true** |
| `mul_base_clamped` (test fn) | `curve25519_dalek::edwards::mul_base_clamped` | `src/edwards.rs:1886–1931` | false |
| `mul_base_clamped` (test fn) | `curve25519_dalek::montgomery::mul_base_clamped` | `src/montgomery.rs:612–633` | false |

### `cargo public-api` entries (8 unique qualified names)

| Qualified name | Matched? | Category |
|---|---|---|
| `curve25519_dalek::edwards::EdwardsPoint::mul_base_clamped` | **yes** | — |
| `curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped` | **yes** | — |
| `curve25519_dalek::traits::BasepointTable::mul_base_clamped` | **yes** | — |
| `curve25519_dalek::edwards::EdwardsBasepointTable::mul_base_clamped` | no | B: type alias |
| `curve25519_dalek::edwards::EdwardsBasepointTableRadix32::mul_base_clamped` | no | A: macro-generated |
| `curve25519_dalek::edwards::EdwardsBasepointTableRadix64::mul_base_clamped` | no | A: macro-generated |
| `curve25519_dalek::edwards::EdwardsBasepointTableRadix128::mul_base_clamped` | no | A: macro-generated |
| `curve25519_dalek::edwards::EdwardsBasepointTableRadix256::mul_base_clamped` | no | A: macro-generated |

The 3 real definitions (`EdwardsPoint`, `MontgomeryPoint`, trait `BasepointTable`)
all match correctly. The 5 unmatched entries are macro instantiations (Category A)
and a type alias (Category B) — they have no separate symbol in the SCIP index,
so no probe atom exists for them.

---

## Summary

All three categories are structural limitations of the SCIP representation
produced by rust-analyzer, not bugs in the matching logic. The probe atoms
correctly cover all functions that have explicit symbol definitions in the SCIP
index.

| Category | Count | Root cause | Fixable? |
|---|---|---|---|
| Macro-generated types | 29 | SCIP indexes macro body once | No (SCIP limitation) |
| Type alias re-exports | 10 | Alias vs. definition name mismatch | Partially (alias mapping) |
| Default trait methods | 7 | No per-type symbol for inherited defaults | Partially (trait resolution) |
| **Total** | **46** | | |
