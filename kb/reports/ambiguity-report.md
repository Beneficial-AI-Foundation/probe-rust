---
auditor: ambiguity-auditor
date: 2026-09-01
status: resolved 2026-09-01 — 0 open (was 1 critical, 4 warnings, 3 info)
---

Scope: full KB pass with focus on the new P20 (Charon candidate resolution) and
the updated glossary entries ("Match key", "Span disambiguation") and
architecture.md Charon paragraph. P20's technical claims were verified against
`src/charon_names.rs` and are accurate: the filter order (file → self-type →
dedup → span), strict-maximum overlap with tie → Ambiguous, all-usable-spans-
exclude → NoMatch, missing-span → Ambiguous, single-line containment, RQN
clearing on Ambiguous, and object-form literal handling in `format_type` all
match the code. The findings below are KB-internal: undefined terms, missing
cross-references, and staleness.

## Critical

### [C1] "Manifest" enrichment source is load-bearing in P20 but undefined everywhere in the KB
- **Location**: kb/engineering/properties.md P20 ("Manifest loop helpers share the parent's `def_id`", "Manifest fails closed on candidates without file proof"); kb/engineering/glossary.md (no entry); kb/engineering/architecture.md (pipeline step 5 and the Charon component describe only `--with-charon` LLBC enrichment)
- **Issue**: P20 references "Manifest" twice as if the reader knows what it is. It is `EnrichmentSource::Manifest` — the `--translation <manifest>` path that enriches atoms from an Aeneas `translation.json` (`src/main.rs`, `src/charon_names.rs`, documented in docs/SCHEMA.md §`charon-def-id`). The KB never mentions `--translation` or the manifest source at all: no glossary entry, no architecture pipeline step, no component note. Worse, P20's "Match — exactly one validated candidate: enrich as usual" hides that the two sources behave differently on Match: the LLBC path overrides `rust-qualified-name`, the Manifest path deliberately does NOT (it only stamps `charon-def-id`; `enrich_atoms` in charon_names.rs). A reader of the KB alone cannot parse P20's Manifest clauses or know that "as usual" is source-dependent. Per the severity guide, an undefined term used in a property is critical.
- **Recommendation**: Add a glossary entry "Manifest (enrichment source)" defining the `--translation` path (Aeneas `translation.json`, `def_id` join, fails closed without file proof, never overrides RQN), add the `--translation` alternative to architecture.md step 5 and the Charon component, and split P20's Match outcome by source (LLBC overrides RQN; Manifest stamps only `charon-def-id`).

## Warnings

### [W1] "Candidate resolution" has no glossary entry; the Match key entry links it to a narrower term
- **Location**: kb/engineering/glossary.md "Match key" ("[candidate resolution](#span-disambiguation) narrows them by file, self type, and span"); "Span disambiguation" ("The span stage of Charon candidate resolution (P20)")
- **Issue**: The whole-process term "candidate resolution" (P20's subject and title) is defined nowhere; the glossary hyperlinks the phrase to "Span disambiguation", which explicitly defines itself as only the *span stage* of that process. The link target is a part standing in for the whole, so a reader following it gets the fourth filter and nothing about the file, self-type, or dedup stages, or the three outcomes.
- **Recommendation**: Add a "Candidate resolution" glossary entry (the four-stage narrowing plus Match/Ambiguous/NoMatch outcomes, pointing to P20) and have "Match key" link to it; keep "Span disambiguation" as the span-stage sub-entry.

### [W2] Domain terms in P20/P18 missing glossary entries: `charon-def-id`, probe-aeneas, self type
- **Location**: kb/engineering/properties.md P20 ("per the `charon-def-id` join contract", "probe-aeneas would link the wrong Lean translation", "self-type match"), P18 ("consumers (probe-aeneas) evaluate them"); kb/engineering/glossary.md (no entries)
- **Issue**: (1) `charon-def-id` is a schema field with a documented coupling invariant and join contract in docs/SCHEMA.md, but P20 cites "the `charon-def-id` join contract" with no link and the glossary has no entry — the contract is unfindable from the KB. (2) probe-aeneas is now named in two properties as the consumer whose behavior motivates design decisions (RQN clearing, cfg evaluation) but is defined nowhere in this KB, not even as a pointer to the ecosystem KB that kb/index.md references. (3) "self type" is used with a tool-specific meaning (extracted from the atom's `display_name` `Type::` prefix on one side and the candidate's last impl segment on the other) in P16, P20, "Match key", and "Span disambiguation", without a definition.
- **Recommendation**: Add glossary entries for `charon-def-id` (linking to docs/SCHEMA.md for the coupling invariant / join contract), probe-aeneas (one line plus a link to the probe ecosystem KB), and self type (both extraction sides); link P20's "join contract" phrase to SCHEMA.md.

### [W3] RQN glossary entry not updated for P20's cleared state and the Manifest no-override rule
- **Location**: kb/engineering/glossary.md "RQN (Rust Qualified Name)"
- **Issue**: The entry says RQN is "derived heuristically from file path + display name, or precisely from Charon LLBC when `--with-charon` is used." After P20 there is a third state the entry omits: the field can be *absent* — cleared even of the heuristic value — when a Charon collision is ambiguous. It also omits that only the LLBC source ever replaces the heuristic (the Manifest source keeps the SCIP-derived RQN, see C1). Consumers reading the glossary would assume every internal atom carries an RQN.
- **Recommendation**: Extend the RQN entry: "May be absent when Charon candidate resolution ends Ambiguous (P20 — a wrong RQN is worse than none); the `--translation` manifest source never overrides the heuristic form."

### [W4] Stale stamps: kb/index.md (2026-04-07, still "draft") and CLAUDE.md property range (P1-P19)
- **Location**: kb/index.md front matter (`last-updated: 2026-04-07`, `status: draft`); CLAUDE.md Knowledge Base section ("numbered invariants P1-P19, known issues C1-C3")
- **Issue**: kb/index.md's stamp is nearly five months old (>30-day threshold) and the KB is clearly past "draft" (it is declared the source of truth). CLAUDE.md's summary of properties.md is one generation behind after P20 landed — the same drift the previous audit's W6 flagged for P16→P19. kb/engineering/index.md correctly says P1-P20.
- **Recommendation**: Re-stamp kb/index.md (and reconsider `status: draft`); bump CLAUDE.md to "P1-P20".

## Info

### [I1] Summaries of candidate resolution list three of its four stages
- **Location**: kb/engineering/glossary.md "Match key" ("narrows them by file, self type, and span"); kb/engineering/architecture.md Charon component ("resolved by successive file, self-type, and span filters")
- **Issue**: Both summaries omit the dedup stage (`(def_id, qualified_name)` collapse) that P20 and `resolve_charon_candidate` include. Not a contradiction — the dedup stage never changes which impl wins — but two of the three descriptions of the same pipeline disagree with the third on its shape.
- **Recommendation**: Either add "dedup" to both summaries or mark them explicitly as abridged ("file, self-type, and span filters; see P20 for the full sequence").

### [I2] P20 wording: "plausibly in the atom's file" understates a hard guarantee; Where omits the atom-side extractor
- **Location**: kb/engineering/properties.md P20, Ambiguous bullet and Where line
- **Issue**: Ambiguous survivors have all passed the normalized-file-path equality filter — they are provably in the atom's file, not "plausibly"; the hedge makes the outcome sound weaker than the code guarantees. Separately, the Where line names the candidate-side `self_type_from_qualified_name` but not the atom-side `self_type_from_display_name` used by the same filter.
- **Recommendation**: Replace "plausibly" with "confirmed (by normalized file path)" and add `self_type_from_display_name` to the Where line.

### [I3] "Blanket impl" glossary definition narrower than P20's use of the term
- **Location**: kb/engineering/glossary.md "Blanket impl" (defined as "a standard library trait implementation ... filtered during `--with-public-api`"); kb/engineering/properties.md P20 NoMatch bullet ("a trait's provided method vs. its blanket impls")
- **Issue**: P20 uses "blanket impls" in the general Rust sense (any crate's blanket implementations of its own trait, spans further down the file), while the glossary entry defines the term only in its cargo-public-api std-trait filtering role. A reader resolving P20's term via the glossary lands on a definition that does not fit the sentence.
- **Recommendation**: Generalize the glossary entry (general Rust meaning first, then the cargo-public-api filtering role as a note), or reword P20 to "its impls further down the file".

## Resolution (2026-09-01)

All 8 findings verified against the current working tree.

- **[C1] RESOLVED** — glossary has a "Manifest (enrichment source)" entry (`--translation`, stamps only `charon-def-id`, fails closed, never overrides RQN); architecture.md pipeline step 5 and the Charon component paragraph both describe the `--translation` manifest path; P20's Match outcome is split by source (LLBC overrides RQN/is-public, Manifest stamps only `charon-def-id`/`charon-version`).
- **[W1] RESOLVED** — glossary has a "Candidate resolution" entry (four filters, three outcomes, links to P20); the "Match key" entry now links the phrase to `#candidate-resolution`, with "Span disambiguation" kept as the span-stage sub-entry.
- **[W2] RESOLVED** — glossary entries added for `charon-def-id` (linking docs/SCHEMA.md for the coupling invariant/join contract), probe-aeneas, and "Self type" (both extraction sides); P20's join-contract sentence links to `docs/SCHEMA.md`.
- **[W3] RESOLVED** — the RQN entry now states the field may be absent (cleared on an Ambiguous collision, P20) and that the Manifest source never replaces the heuristic form.
- **[W4] RESOLVED** — kb/index.md restamped `last-updated: 2026-09-01`; CLAUDE.md now says "P1-P20". (`status: draft` retained — the recommendation asked only to reconsider it.)
- **[I1] RESOLVED** — both summaries now list all four stages: "Match key" says "file, self type, dedup, and span"; architecture.md says "file, self-type, dedup, and span filters".
- **[I2] RESOLVED** — P20's Ambiguous bullet reads "confirmed (by normalized file path)"; the Where line includes `self_type_from_display_name`.
- **[I3] RESOLVED** — P20's NoMatch bullet now reads "its impls further down the file"; the term "blanket impls" is gone from P20, so the glossary entry's cargo-public-api scope no longer conflicts.

Anchor sanity check: all new cross-reference anchors match their entry names under GitHub rules (lowercase, spaces→hyphens, parens dropped): `#candidate-resolution`, `#manifest-enrichment-source`, `#charon-def-id`, `#self-type`, and `#p20--charon-candidate-resolution-never-picks-arbitrarily` all resolve. Note: glossary entries are bold-text terms, not headings — the pre-existing convention for the whole file.
