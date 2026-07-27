---
review: feat/charon-def-id (PR #21)
date: 2026-07-16
base: main (2414b39)
head: d3cbdbc
commits:
  - 3050c19 feat(charon): emit charon-def-id + charon-version on atoms
  - d3cbdbc fix(charon): emit charon-def-id only with its charon-version
scope: unsoundness, comment/code gaps, missing tests, Rust practice
---

# Branch review: `feat/charon-def-id`

Surfaces the resolved Charon `FunDeclId` as `charon-def-id` on atoms, plus the
LLBC's top-level `charon-version`, so probe-aeneas can join Rust↔Lean by integer
equality gated on matching Charon versions.

**Verdict:** Emit-side provenance pairing (`d3cbdbc`) is the right fail-closed
contract. Remaining issues are documentation/schema drift, a fragile version
reader whose rationale does not match the call site, and a few test gaps. No
medium+ security findings under the local-CLI threat model.

Changed files: `src/charon_names.rs`, `src/lib.rs`, `src/commands/callee_crates.rs`,
`src/public_api.rs`.

---

## High

### 1. Prefix version parse is unjustified and weaker than data already in hand

`enrich_atoms_with_charon_names` always calls `parse_llbc_names`, which reads and
fully deserializes the LLBC, then opens the same file again for a 4 KiB string
scan (`read_charon_version` / `parse_charon_version_prefix`).

The comment on `read_charon_version` says the prefix read “avoids re-parsing the
multi-megabyte AST” — but that parse just happened. Consequences:

- Second open / wasted I/O
- TOCTOU window between the two reads
- Non-JSON heuristic (`find("\"charon_version\"")`, no escape handling, first
  substring wins) with no performance win

**Preferred fix:** take `charon_version` from the already-parsed `root` in
`parse_llbc_names` (return `(HashMap, Option<String>)` or a small struct). Fail
closed if missing or non-string. Drop the prefix scanner unless a path that never
full-parses is truly needed.

### 2. Schema / docs not updated (repo rule violated)

`docs/SCHEMA.md` Schema Evolution:

> When adding new optional fields, increment the minor version (`2.1` -> `2.2`).

This branch adds two optional atom fields and leaves `SCHEMA_VERSION` / docs at
**2.3**, with:

- no field rows in the AtomWithLines table
- no example JSON
- no schema changelog entry
- no consumer provenance rule
- `kb/engineering/properties.md` still saying schema is `"2.3"`

Largest process gap: consumers (probe-aeneas) cannot rely on the documented
contract for the join this feature exists to enable.

**Preferred fix:** bump to **2.4**, document both fields + paired-omit /
provenance rule in `SCHEMA.md`, `SCHEMA_VERSION`, and properties.

---

## Medium

### 3. Doc comments disagree with code

| Location | Claim | Reality |
|----------|--------|---------|
| `AtomWithLines::charon_def_id` | “resolved by matching … to a `fun_decls[]` entry” | `def_id` is the `item_names` `Fun` key; match is match-key (+ span disambiguation), not a direct `fun_decls` lookup |
| Both new field docs | “Omitted when no Charon function was matched” | Also omitted when match succeeds but version read fails (`d3cbdbc`) |
| PR body / version tests | “compact + pretty” | `test_read_charon_version` only covers compact JSON; no whitespace / pretty case |

`CharonFunInfo`’s comment (“`Fun` key in `item_names`”) is accurate; the
`AtomWithLines` docs should match that and state the paired-omit rule.

### 4. Provenance contract not enforced on the “don’t emit” path

```rust
if let Some(version) = &charon_version {
    atom.charon_def_id = Some(best.def_id);
    atom.charon_version = Some(version.clone());
}
```

There is no `else` that clears both fields. Fresh extract atoms start as `None`,
so today’s pipeline is fine; re-enrichment or future callers can leave a stale id
next to a newly updated RQN.

Serde also accepts orphan `charon-def-id` without `charon-version` on ingest —
fine for a CLI, but should be documented as an emitter invariant, not a
deserializer invariant.

**Preferred fix:** in the `else`, set both to `None` (and ideally clear whenever
a match fails).

### 5. Wrong match now looks like a precise integer join

`charon_def_id` rides on the same `resolve_charon_candidate` result as RQN /
`is_public`. Pre-existing ambiguity (single candidate with no usable span,
heuristic RQN fallback) now stamps an authoritative-looking integer. Not a new
matcher bug, but it raises the cost of any mismatch for probe-aeneas.

**Document:** join should require version match and preferably corroborate
RQN/span; do not treat `charon-def-id` alone as ground truth.

---

## Low

### 6. Missing / thin tests

Covered well:

- happy-path enrich with both fields
- omit both when version absent
- `def_id` on parse
- missing file → `None`

Gaps:

1. **Serde shape** — no assert that JSON keys are `charon-def-id` /
   `charon-version`, omit when `None`, round-trip when `Some`
2. **Whitespace / pretty prefix** — claimed, untested (less relevant if the
   prefix scanner is removed)
3. **Explicit clear** — no test that a pre-set `charon_def_id` is cleared when
   version is missing
4. Optional: assert / document that `item_names` `Fun` key and
   `fun_decls[].def_id` are treated as the same id if they ever diverge

### 7. Rust practice nits

- Prefer extracting version from the parsed `serde_json::Value` over hand-rolled
  scanners
- Per-atom `version.clone()` is fine for short strings; `Arc<str>` is optional
  micro-polish
- Fixed temp dirs (`probe_rust_test_charon_version`) can collide under parallel
  tests — pre-existing pattern; `tempfile::TempDir` would be more idiomatic

---

## What looks sound

- Using `item_names` `Fun` key as `FunDeclId` matches the Aeneas
  `translation.json` `def_id` story
- Paired emit after Copilot’s review is the right fail-closed rule for orphan ids
- `test_enrich_omits_def_id_without_charon_version` correctly checks RQN /
  `is_public` still enrich when version is absent
- No security issues in the CLI threat model (no injection, path traversal, or
  cross-tenant paths introduced)

---

## Subagent summaries

### Bugbot

| Severity | Location | Finding |
|----------|----------|---------|
| medium | `src/charon_names.rs:798-801` | Stale provenance fields not cleared when version is `None` |
| medium | `src/lib.rs:341-360` / `docs/SCHEMA.md` | Schema docs omit new fields; no minor bump |
| low | `src/lib.rs:344-354` | Field docs omit version-missing omission case |
| low | `src/charon_names.rs` serde tests | No serde key / omit / round-trip coverage for new fields |

### Security review

No medium, high, or critical security vulnerabilities. Residual concerns are
metadata correctness / verification integrity (prefix-parser fragility,
downstream consumers that skip version gating) — outside exploitable threat
model for a local analysis CLI.

---

## Suggested fix order

1. Read `charon_version` from the parsed LLBC root; delete the prefix scanner
   (or keep only as fallback with tests).
2. Bump schema to **2.4**; document both fields + paired-omit / provenance rule
   in `SCHEMA.md`, `SCHEMA_VERSION`, and properties.
3. Fix `AtomWithLines` docs; `else` clear both fields.
4. Add serde (+ optional stale-clear / pretty) tests.

---

## Related

- PR: https://github.com/Beneficial-AI-Foundation/probe-rust/pull/21
- Issue: #20
- Downstream: Beneficial-AI-Foundation/probe-aeneas#40 / PR #41
- Copilot review (addressed in `d3cbdbc`): orphan `charon-def-id` without version
