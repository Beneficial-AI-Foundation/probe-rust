---
auditor: code-quality-auditor
date: 2026-09-02
status: clean — 0 critical, 0 warning, 1 info (round 3; round-2 Info resolved)
---

Scope: round 3 over the uncommitted working tree on `fix/rqn-ambiguity` (on top of
`db243e7`). Since round 2 only docs and doc comments/tests changed:
`kb/engineering/{glossary,properties,architecture}.md` (new glossary entries
**File proof**, **LLBC (enrichment source)**, **Loop helper (Manifest)**; P15 and
P20 wording), `docs/SCHEMA.md`, `CHANGELOG.md`, and in `src/charon_names.rs` the
`validate_single_candidate` doc, `CharonFunInfo` line docs, `span_overlap` /
`in_atom_file` / `ResolveOutcome::Ambiguous` docs, and the extended
`test_resolve_multi_candidate_eq_collision` /
`test_resolve_multi_candidate_try_from_split_by_span` tests. P20 re-verified
clause by clause against the code; every new doc sentence checked for drift.
`cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --all -- --check`
clean, `cargo test` 246 lib + 3 integration passed (1 ignored).

## Round-2 findings — status

| # | Round-2 finding | Status | Evidence |
|---|-----------------|--------|----------|
| I1 | CHANGELOG `[Unreleased]` unrenderable-kind list omitted `dyn` | **Resolved** | `CHANGELOG.md:12` now reads "(slices, arrays, tuples, raw pointers, `dyn`, unknown literal encodings)", matching P20 (`properties.md:208`) and the code comment (`charon_names.rs:315-316`). |

## Critical

None. P20 re-verified against `src/charon_names.rs`:

- **Filter order** — `resolve_charon_candidate` (`:844-930`): lone candidate →
  `validate_single_candidate` (`:849`); same-file via `in_atom_file` (`:853-856`)
  → self-type (`:868-887`) → dedup on `(def_id, qualified_name)` (`:889-897`) →
  strict-max positive overlap (`:905-929`). No index-0 pick.
- **Self-type narrowing-only** — `matching.is_empty() → same_file` (`:880-884`),
  `None => same_file` (`:886`); `self_type_from_qualified_name` reads only the
  last impl segment's self type, never trait type arguments.
- **Lone survivor / LLBC file-less exception / Manifest fail-closed** —
  `validate_single_candidate` (`:793-824`): `file_path` `None`/`""` → `Match`
  for `Llbc`, `NoMatch` for `Manifest`; `!in_atom_file` → `NoMatch`; usable
  overlap `<= 0` → `NoMatch`; else `Match`. Dedup-to-single re-enters it
  (`:897`). Multi-candidate keys with no same-file candidate → `NoMatch`
  (`:857-866`). Matches P20's exception sentence, glossary **File proof**
  ("Manifest rejects any lone candidate without file proof; LLBC accepts a lone
  candidate that has none at all"), and SCHEMA `charon-def-id` (`:172`).
- **Inclusive overlap** — `span_overlap` (`:668-677`) is
  `min(atom_end, c_end) - max(atom_start, c_start) + 1`; `None` for
  `atom_start == 0` or candidate lines absent / `line_start == 0`. Matches P20
  (`:200`), glossary **Span disambiguation**, and the new `CharonFunInfo`
  line docs (`:42-47`: 1-based, `0`/`None` = no usable span).
- **Three outcomes** — unique max → `Match`; tied max → `Ambiguous` (`:924`);
  all usable, none positive → `NoMatch` (`:927`); any missing span with no
  positive winner → `Ambiguous` (`:928`). A positive span beats a span-less
  sibling (`Match`), pinned by `test_resolve_multi_candidate_mixed_span_availability`
  and exercised by the extended `eq` collision test (span-less derived
  `PartialEq for ServiceId` loses to the manual impl at 338-340).
- **Ambiguous arm** — `enrich_atoms` pre-clears def-id/version (`:1087-1088`);
  `Ambiguous` clears `rust_qualified_name` only under `EnrichmentSource::Llbc`
  (`:1148-1150`), never touches `is_public`. `Match`: RQN override LLBC-only
  (`:1117-1119`), `is_public` only when carried (`:1124-1126`), def-id +
  version together or not at all (`:1131-1134`). Matches P20's Match/Ambiguous
  bullets, glossary **LLBC (enrichment source)** ("the only source that writes
  and, on Ambiguous, clears the RQN"), and SCHEMA's coupling invariant (`:175`).
- **`format_type` / `?` placeholder** — string literals `Bool`/`Char`
  (`:240-247`); object literals via `UInt`/`Int`/`Float` (`:254-258`); else
  `None`. `build_trait_impl_type_info_map` maps `None` on `types[1..]` to `?`
  (`:320-324`), `None` on `types[0]` skips the entry (`:313`) → type-less
  `{impl Trait}` segment, as P20's last paragraph states.

## Warnings

None.

## Info

1. **`span_overlap` doc slightly over-generalizes the `0`-line guard** —
   `charon_names.rs:666-667` says `None` when "candidate lines absent/`0`", but
   the match arm (`:673`) guards only `line_start > 0`. A candidate with
   `line_start > 0` and `line_end == 0` (only reachable from a malformed LLBC
   span with `beg` but no `end`, since `build_fun_span_map` defaults each to
   `0` independently, `:537-546`) yields a negative overlap → `NoMatch` rather
   than `None`. Conservative, so not a P20 violation; a one-word doc tweak
   ("`line_start` absent/`0`") or `&& e >= s` guard if wanted.

## Verified clean

- **New glossary entries** — **File proof** (`glossary.md:67`) matches
  `in_atom_file` (`:682-685`: non-empty path, `normalize_source_path` equality).
  **Loop helper (Manifest)** (`:79`) matches the dedup stage and
  `Enrichment::from_translation_json` (`:973`). **LLBC (enrichment source)**
  (`:77`) matches the `Match`/`Ambiguous` arms and cites `resolve_enrichment`
  (`:1041`) as the single dispatch point — correct, `extract.rs:318` calls it.
- **P15 wording** — `properties.md:151` now covers the Manifest source:
  `enrich_from_manifest` (`extract.rs:311-345`) warns on read/parse error
  (`:341-342`) and on a missing `charon_version` (`:332-336`), never aborts;
  LLBC parse failure warns at `extract.rs:301-302`. `Where` lists
  `commands/extract.rs`, `charon_cache.rs` — both exist.
- **`validate_single_candidate` doc** (`:783-792`) — the parenthetical "a
  candidate *with* a matching file path but no usable lines is accepted on the
  file match, on both sources" matches `:817-823` (`span_overlap` `None` →
  fall through to `Match` regardless of `source`); pinned by
  `test_resolve_single_candidate_real_file_zero_lines_accepted`.
- **Extended tests** — `test_resolve_multi_candidate_eq_collision` (`:2931`)
  and `test_resolve_multi_candidate_try_from_split_by_span` (`:2980`) doc
  claims verified against their candidate sets: `SpecificServiceId` atoms are
  split off by self type alone (single survivor → validated), `DeviceId`
  `TryFrom<u8|i32|u32>` split by span alone.
- **P1** — `metadata.rs:13` `SCHEMA_VERSION = "3.0"` matches `docs/SCHEMA.md:3`.
- **SCHEMA rows** — `rust-qualified-name` (`:161`), `charon-def-id` (`:172`),
  `charon-version` (`:173`), coupling invariant (`:175`), comparison table
  (`:408-411`) all consistent with `enrich_atoms`.
- **CLI flags** — `architecture.md` mentions `--auto-install`, `--with-charon`,
  `--with-public-api`, `--translation` (all present in `main.rs`; `--include` is
  Charon's own flag, correctly attributed). `last-updated: 2026-09-02`.
- **Architecture** — `architecture.md:96-100, 185` describe both enrichment
  sources, the filter pipeline, LLBC-only RQN clearing, Manifest fail-closed,
  and inclusive overlap consistently with P20 and the code.
- **Auditor brief** — `code-quality-auditor.md:33` P20 item matches current P20.
- **P12 / C1-C3** — unchanged; no binary-crate `is-public-api` special-casing;
  C1-C3 tests still present and passing.
- **Working tree** — `git status` shows only the ten expected modified files, no
  untracked debris; fmt/clippy/test all clean.
