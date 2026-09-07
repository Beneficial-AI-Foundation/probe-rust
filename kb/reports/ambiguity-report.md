---
auditor: ambiguity-auditor
date: 2026-09-02
status: 0 critical, 2 warnings, 4 info
---

Round 3 on the `fix/rqn-ambiguity` working tree. Every finding of the
2026-09-02 round-2 report was re-checked against the current text of
kb/engineering/{properties,glossary,architecture}.md, docs/SCHEMA.md,
CHANGELOG.md and `src/charon_names.rs` (`resolve_charon_candidate`,
`validate_single_candidate`, `span_overlap`, `in_atom_file`,
`self_type_from_*`, `bare_type_name`, `ResolveOutcome`, `enrich_atoms`,
`resolve_enrichment`) and `src/commands/extract.rs` (`enrich_from_manifest`).
Then a focused pass for contradictions among P20, the glossary entries it
links (Candidate resolution, File proof, LLBC/Manifest sources, Loop helper,
Span disambiguation, Self type, RQN), SCHEMA.md's `rust-qualified-name` /
`charon-def-id` / `is-public-api` rows, and the code.

## Round-2 findings — status

| # | Title | Status | Evidence |
|---|-------|--------|----------|
| C1 | Glossary "Library crate" asserted retired P12 behaviour | **Resolved** | Entry now: "`is_library_crate` only labels the extract summary; `is-public-api` is computed identically for library and binary-only crates (only under `--with-public-api`, P11; P12 is retired)". Agrees with "Binary-only crate", "is-public-api", P11, P12. |
| C2 | SCHEMA compatibility row: `is-public-api` "Always for internal atoms (SCIP module walk)" | **Resolved** | Row now: "Optional: only under `--with-public-api` (`true`/`false` for atoms it reaches; absent otherwise and for stubs)". Agrees with the field-table row, the 2026-08-31 changelog bullet and P11. |
| W1 | CHANGELOG "span-less items keep the heuristic RQN" | **Resolved as recommended** (residual imprecision → W1 below) | Now: "A multi-candidate key whose candidates are all provably elsewhere or unverifiable (span-less) keeps the heuristic RQN; a lone span-less LLBC candidate is still accepted on match key alone, as before." The lone-candidate qualifier is in place. |
| W2 | Glossary "RQN" two-state | **Resolved** | "…when `--with-charon` is used and candidate resolution finds a Match; an atom no Charon entry matched (NoMatch) keeps the heuristic form even under `--with-charon`." Links [candidate resolution]. |
| W3 | "File proof" undefined | **Resolved** (see I1 for wording, I3 for ordering) | Glossary "File proof" entry added (non-empty path normalizing to the atom's `code-path`, `in_atom_file`; span is a separate signal). P20 links it; the Manifest glossary entry and architecture use the same term. |
| W4 | "Span disambiguation" omitted the NoMatch exit | **Resolved** (see W2 below for a different gap in the same sentence) | "When every survivor carries a usable span and none overlaps the atom, the stage ends NoMatch instead (e.g. an inherited default trait method's body vs. its impls further down the file) and the heuristic RQN is kept." Matches `overlaps.iter().all(|o| o.is_some())`. |
| I1 | Architecture omitted `charon-version` | **Resolved** | Step 5: "charon-def-id/charon-version from a Charon LLBC (legacy)"; data flow: "charon-def-id+charon-version (both)". |
| I2 | SCHEMA changelog garbled phrase | **Resolved** | "`?` for trait type arguments probe-rust cannot render (slice, array, tuple, raw pointer, `dyn`)". |
| I3 | P15 silent on manifest without `charon_version` | **Resolved** | "or one without a `charon_version` (matched atoms then get no def-id/version pair)". Matches `enrich_from_manifest`'s `has_version == false` branch and the pre-clear in `enrich_atoms`. |
| I4 | "Legacy" unexplained in LLBC entry | **Resolved** | "called *legacy* because it is slated for retirement once every Aeneas project ships a `translation.json` (`resolve_enrichment` is the single dispatch point between the two)". |
| I5 | P20 Where line omitted `bare_type_name` | **Resolved** | Present in the Where line. |
| I6 | `validate_single_candidate` doc over-widened the lenient case | **Resolved** | Doc now: "a candidate with no file path at all (`None`/`""`) is still accepted on match-key alone … (A candidate *with* a matching file path but no usable lines is accepted on the file match, on both sources.)" Matches the code. |

All twelve round-2 findings are closed. Nothing from rounds 1 or 2 remains open.

## Critical

None.

## Warnings

### [W1] CHANGELOG "unverifiable (span-less)" names the wrong attribute; contradicts P20 for same-file span-less collisions
- **Location**: CHANGELOG.md Unreleased, second bullet ("A multi-candidate key whose candidates are all provably elsewhere or unverifiable (span-less) keeps the heuristic RQN"); kb/engineering/properties.md P20 ("same-self-type impls … are split by span alone; if their spans are unusable they end Ambiguous"); `src/charon_names.rs` `resolve_charon_candidate` comment in the `same_file.is_empty()` branch ("unverifiable (span-less compiler-generated items, e.g. derived `fmt`)")
- **Issue**: The multi-candidate NoMatch shortcut fires on the *file* filter (`in_atom_file` on every candidate false), so "unverifiable" means *file-less*, not span-less. A multi-candidate key whose candidates are all in the atom's file but span-less does **not** keep the heuristic RQN: after dedup two or more distinct survivors reach the span stage with `overlaps` all `None` → Ambiguous → under `--with-charon` the RQN is *cleared*. Read literally, the CHANGELOG sentence says the opposite of P20 for that case. The code comment carries the same parenthetical but sits in a branch that already established "no candidate is in the file", so it is merely loose; the CHANGELOG has no such context.
- **Recommendation**: CHANGELOG: "A multi-candidate key none of whose candidates can be placed in the atom's file (other files, or no file path at all) keeps the heuristic RQN; a lone file-less LLBC candidate is still accepted on match key alone, as before." Optionally tighten the code comment to "unverifiable (no file path at all — span-less compiler-generated items such as derived `fmt`)".

### [W2] Glossary "Span disambiguation": "missing span data leaves the collision ambiguous" is false when another survivor overlaps positively
- **Location**: kb/engineering/glossary.md, "Span disambiguation" ("A tie or missing span data leaves the collision ambiguous — nothing is stamped and (LLBC source) the atom's RQN is cleared rather than guessed."); `src/charon_names.rs` `resolve_charon_candidate` span stage
- **Issue**: The code computes `best` over the survivors that *have* a positive overlap; when exactly one survivor holds that maximum, the result is **Match** even if other survivors have no span at all (`best = Some(n)` → `winners == [single]`). Only when no survivor overlaps positively does missing span data force Ambiguous (P20's "mix of an excluding usable span and a span-less survivor"). So a same-file `#[derive]`-generated span-less candidate beside a manual impl whose span contains the atom resolves to the manual impl — the glossary says it ends Ambiguous and, on the LLBC path, that the RQN is cleared. The entry's own preceding sentence ("only a strict-maximum positive overlap wins") is correct; the tie/missing sentence over-generalises it. P20 does not state this Match sub-case either, though nothing in P20 contradicts it.
- **Recommendation**: "A tie for the maximum leaves the collision ambiguous; so does missing span data when *no* survivor overlaps positively (a span-less survivor beside a single positively overlapping one does not block that one from winning). Ambiguous: nothing is stamped and (LLBC source) the atom's RQN is cleared rather than guessed." Add one clause to P20's Match bullet: "span-less co-survivors do not block a unique positive-overlap winner".

## Info

### [I1] Glossary "File proof": "a lone candidate that has none at all" is ambiguous between "no file proof" and "no file path"
- **Location**: kb/engineering/glossary.md, "File proof" (last sentence: "the LLBC source accepts a lone candidate that has none at all")
- **Issue**: "None at all" reads as "no file proof at all", but a lone LLBC candidate whose file path is present and names a *different* file has no file proof and is rejected by both sources (`validate_single_candidate`: `has_usable_file && !in_atom_file → NoMatch`). The accepted case is specifically an absent/empty path. P20 and the code doc say "no file path at all"; the glossary entry that defines the term should too.
- **Recommendation**: "…the LLBC source accepts a lone candidate that carries no file path at all (`None`/`""`); a candidate whose path names another file is rejected by both."

### [I2] Glossary "LLBC (enrichment source)" says a Match overrides `is-public` unconditionally
- **Location**: kb/engineering/glossary.md, "LLBC (enrichment source)" ("on a Match overrides the atom's RQN and `is-public`"); contrast P20 Match bullet ("`is-public` (when carried)"), docs/SCHEMA.md `is-public` row ("a candidate without visibility … never clobbers the SCIP value")
- **Issue**: `enrich_atoms` writes `is_public` only when `best.is_public` is `Some`; an LLBC entry lacking `attr_info.public` leaves the SCIP value. The glossary sentence drops the qualifier the other two documents carry.
- **Recommendation**: "overrides the atom's RQN and, when the entry carries visibility, `is-public`".

### [I3] Glossary alphabetical order broken by the new "File proof" entry (and two pre-existing entries)
- **Location**: kb/engineering/glossary.md ("Terms are listed alphabetically."); "File proof" sits between "Impl-descriptor resolution" and "Inherited default trait method"; pre-existing: "Binary-only crate" after "Blanket impl", "Candidate form" before "Call attribution"
- **Issue**: The file promises alphabetical order and the newly added entry is the furthest out of place, making the term hard to find by scanning.
- **Recommendation**: Move "File proof" between "File gate" and "Foreign declaration"; swap the two pre-existing pairs while there.

### [I4] Glossary "Span disambiguation" frames overlap as "LLBC span overlap" though the stage runs on Manifest spans too
- **Location**: kb/engineering/glossary.md, "Span disambiguation" ("compared by LLBC span overlap with the atom's source line range"); contrast P20 ("Charon and manifest spans are 1-based…"), `CharonFunInfo::line_start` doc ("from the LLBC/manifest span")
- **Issue**: The span stage is source-blind (`span_overlap` takes a `CharonFunInfo` from either source); "LLBC span" suggests the Manifest path skips it.
- **Recommendation**: "compared by span overlap (LLBC or manifest span) with the atom's source line range".

## Focused-pass notes (no finding)

- P20 ↔ `resolve_charon_candidate` / `validate_single_candidate`: filter order, lone-candidate validation, the LLBC file-less exception, the Manifest fail-closed rule, the NoMatch/Ambiguous split at the span stage, and the 1-based inclusive overlap formula all agree with the code (modulo the W2 sub-case, which P20 does not contradict).
- P20 ↔ `enrich_atoms`: Match stamps RQN only on `Llbc`, `is_public` only when carried, def-id/version only when a version exists; Ambiguous clears RQN only on `Llbc`; NoMatch is a no-op after the up-front pre-clear. Consistent with SCHEMA's coupling invariant and P15's no-`charon_version` case.
- SCHEMA `rust-qualified-name` (three states), `charon-def-id` (lone-candidate rule with the `--with-charon` file-less exception, collision narrowing, manifest refuses match-key-only), `is-public-api` (field row and compatibility row) are mutually consistent and consistent with P11/P20 and the glossary.
- Glossary "Candidate resolution", "Loop helper (Manifest)", "Self type", "RQN", "Manifest (enrichment source)": consistent with P20 and the code. "Self type" correctly states the filter is a narrowing signal only (`matching.is_empty() → same_file`).
- Staleness: all three engineering files `last-updated` 2026-09-02; `kb/engineering/index.md` 2026-08-11 (22 days, within the 30-day window).
