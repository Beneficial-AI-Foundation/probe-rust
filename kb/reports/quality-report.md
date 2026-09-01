---
auditor: code-quality-auditor
date: 2026-09-01
status: resolved 2026-09-01 — 0 open (was 0 critical, 2 warnings, 2 info; Info 2 acknowledged as won't-fix)
---

Scope: uncommitted working-tree changeset — multi-candidate Charon resolution
reworked into a filter pipeline in `src/charon_names.rs` (`resolve_charon_candidate`,
`validate_single_candidate`, `ResolveOutcome`, `span_overlap`, self-type helpers,
object-form literal handling in `format_type`), new property P20, and KB updates
(properties/architecture/glossary/index/CLAUDE.md). Deep verification on P20, P10,
P15, and change-introduced doc staleness; grep-level sanity pass on P1-P9, P11-P14,
P16-P19 (only `charon_names.rs` changed in `src/`, so those properties' code paths
are untouched). `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings`
clean, 239 lib + 3 integration tests pass.

## Critical

None. P20 as written in `kb/engineering/properties.md` is satisfied by the
implementation; every edge case checked holds:

- **Filter order** — `resolve_charon_candidate` (`charon_names.rs:834-923`) applies
  exactly the documented sequence: same-file → self-type → dedup on
  `(def_id, qualified_name)` → strict-maximum positive span overlap.
- **Empty candidate lists** — cannot occur in `by_match_key` (`entry().or_default().push`
  always pushes ≥1), and an empty slice would still fall through to the same-file
  filter and return `NoMatch`, never panic or pick.
- **Atoms with `lines_start == 0`** — `span_overlap` returns `None` (line 662-663);
  a lone candidate is then accepted on file-path proof alone, and a multi-candidate
  collision becomes `Ambiguous` (missing span data = undecidable), matching the
  glossary's "tie or missing span data leaves the collision ambiguous".
- **`file_path` `Some("")` vs `None`** — both rejected under Manifest
  (`has_usable_file`, lines 788-791: fail-closed, no `charon-def-id` without file
  proof); both lenient-accepted only for a *lone* LLBC candidate; both excluded
  from the multi-candidate same-file filter (lines 846-850), so an unverifiable
  candidate is never picked under a collision.
- **Manifest fail-closed survives the multi→1 reduction** — the dedup-to-single
  path re-routes through `validate_single_candidate` (line 892); the span-stage
  winner (line 916) bypasses it but by construction already carries a non-empty
  file path matching the atom (same-file filter) plus a positive overlap, i.e.
  strictly more evidence than `validate_single_candidate` demands.
- **Tie handling** — equal best overlaps among deduped survivors → `Ambiguous`
  (line 917); all spans usable and none positive → `NoMatch` (line 920); any
  missing span with no positive winner → `Ambiguous` (line 921). Matches P20's
  three-outcome table exactly.
- **Ambiguous clears everything, both sources** — `enrich_atoms` pre-clears
  `charon_def_id`/`charon_version` for every atom (lines 1081-1082) and the
  `Ambiguous` arm sets `rust_qualified_name = None` with no source check
  (line 1137). Pinned by `test_enrich_ambiguous_clears_rqn_and_stamps_nothing`,
  which iterates over both `EnrichmentSource::Llbc` and `::Manifest` and asserts
  all three fields are `None`.
- **No first-match-wins path remains** — no arbitrary index into the candidate
  list anywhere in the resolution pipeline; `disambiguate_by_span` is gone.
- **`format_type` object-form literals** — `{"UInt":"U8"}`/`{"Int":"I32"}`/
  `{"Float":"F64"}` → lowercased primitive name (lines 249-253), pinned by
  `test_format_type_literal_object_forms`; string-form (`"Bool"`, `"Char"`)
  unchanged.
- **Test coverage** — 12 new/updated resolution tests cover cross-file rejection,
  same-file span mismatch, span-less lone candidates (accepted for LLBC), the
  eq/try_from collisions, self-type splitting, all-spans-elsewhere NoMatch,
  dedup, and the file filter.

## Warnings

1. **CHANGELOG has no entry for an output-affecting behavior change.** The last
   entry is `[0.10.0] - 2026-08-11` and there is no `[Unreleased]` section, yet
   this changeset changes emitted atoms: an ambiguous collision now *clears*
   `rust-qualified-name` (previously a first-match candidate's RQN or the SCIP
   heuristic was kept), and object-form literal types change trait-impl RQN
   strings (`TryFrom<u8>` vs one collapsed name). Consumers diffing probe output
   across versions need this recorded. If the entry is planned for the release
   commit, add it before merging.

2. **Auditor checklists not updated to P20.**
   `.cursor/rules/auditors/code-quality-auditor.md:7` still reads "P1-P19, C1-C3"
   and has no P20 check item; `.cursor/rules/auditors/test-quality-auditor.md`
   (lines 7 and 26) likewise stops at P19. The changeset updated `CLAUDE.md` and
   `kb/engineering/index.md` to P1-P20 but missed the auditor briefs — the same
   drift the 2026-08-11 round fixed for P17-P19.

## Info

1. **Stale comment referencing removed `disambiguate_by_span`** —
   `src/charon_names.rs:2776` (inside
   `test_resolve_single_candidate_real_file_zero_lines_accepted`): "must be
   treated as 'no span' (like disambiguate_by_span's `s > 0`)". The function no
   longer exists; the live equivalent is `span_overlap`'s `s > 0` guard. The
   `CHANGELOG.md:72` mention is a historical 0.6.3 entry and is correct as a
   record; the plan doc `docs/charon-enrichment-from-manifest-plan.md` describes
   `resolve_charon_candidate` at design time and reads fine against the new code.
   No other stale references to the removed function or the first-match-wins
   fallback found in code comments, `docs/`, or `kb/` (the P20/glossary mentions
   are deliberate descriptions of the *removed* behavior).

2. **Untracked debris at repo root** — `has-body.json` and `no-body.json` are
   untracked at the project root and look like manual-testing leftovers; delete
   or gitignore before committing.

## Verified clean

- **P10 (is-public from SCIP, Charon override only when carried)** —
  `is_signature_public` (`lib.rs:162`) unchanged; `enrich_atoms` overrides
  `is_public` only inside `if let Some(is_public) = best.is_public`
  (`charon_names.rs:1118-1120`), never clobbering the SCIP value with `None`
  (Manifest candidates always carry `is_public: None`). Pinned by
  `test_enrich_does_not_clobber_is_public_with_none` and
  `test_enrich_propagates_visibility`. The `Ambiguous` arm touches only RQN,
  never `is_public`.
- **P15 (Charon non-fatal)** — `enrich_with_charon` (`commands/extract.rs:287-322`)
  warns and returns on both LLBC generation failure and parse failure;
  `enrich_from_translation_manifest` warns and skips on manifest read/parse
  errors without falling back to a charon run. Pinned by
  `test_charon_failure_is_non_fatal`.
- **Sanity pass (grep-level)** — P1 (schema `"3.0"` in `metadata.rs:13` matches
  `docs/SCHEMA.md`), P4 (`add_external_stubs` present, untouched), P5
  (`dependencies: BTreeSet<String>`), P6 (`normalize_code_name` call sites
  present), P7 (`is_function_like_kind` in `constants.rs`), P13
  (`sanitize_for_filename` in `metadata.rs`), P14 (`regenerate` flag honored in
  `scip_cache.rs:126-131`). P2-P3, P8-P9, P11-P12, P16-P19 live entirely outside
  the changed file and their tests are in the passing suite; no re-verification
  needed beyond the 2026-08-11 audit.
- **KB consistency** — the updated glossary ("Match key", "Span disambiguation"),
  `architecture.md` Charon section, `index.md`, and `CLAUDE.md` all agree with
  the P20 text and with the code; the glossary's `#span-disambiguation` anchor
  still resolves.

## Resolution (2026-09-01)

- **[W1] RESOLVED** — CHANGELOG.md now has an `[Unreleased]` section with a Fixed entry covering both output-affecting changes: RQN clearing on ambiguous collisions (filter pipeline, no first-match-wins) and object-form literal types in `format_type` (`TryFrom<u8>` vs collapsed RQNs).
- **[W2] RESOLVED** — `.cursor/rules/auditors/code-quality-auditor.md` and `test-quality-auditor.md` both read "P1-P20, C1-C3"; code-quality-auditor.md line 33 adds a dedicated P20 check item (filter pipeline, three outcomes, Manifest fail-closed, object-form literals).
- **[I1] RESOLVED** — `grep disambiguate_by_span src/charon_names.rs` returns no matches; the stale test comment is gone.
- **[I2] ACKNOWLEDGED (won't-fix)** — `has-body.json` and `no-body.json` are pre-existing user files at the repo root, deliberately kept; not part of this changeset.
