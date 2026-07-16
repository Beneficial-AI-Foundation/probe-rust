# Plan: charon enrichment from `translation.json` (manifest path), LLBC as legacy

Status: planned. Companion to probe-aeneas `docs/charon-def-id-matching-plan.md`
(WS3) and the durable-fix drafts in probe-aeneas `docs/upstream-issues/`.

## Goal

When an Aeneas `translation.json` is available, probe-rust should enrich atoms
with charon-derived data (`charon-def-id`, and optionally `rust-qualified-name`)
**from the manifest**, without running charon a second time. The current
LLBC-based path (`enrich_atoms_with_charon_names` + `charon_cache`) becomes the
**legacy** path for Aeneas projects that ship no `translation.json`.

Architected so that, once every Aeneas project is required to ship a
`translation.json`, deleting the legacy path is a localized change — mirroring
probe-aeneas's `function_source` seam (`RecordSource::{Manifest,LegacyScrape}`).

## Why this is possible (sanity-checked on spqr)

charon already ran once, inside Aeneas, to produce `translation.json`. For the
Rust↔Lean join, the manifest is self-sufficient:

- Every `functions[]` entry carries `rust_name` + `def_id` + `source`
  `{file, begin_line, end_line}` — verified 881/881 on spqr.
- `def_id` on `functions[]` is the charon `FunDeclId` — the exact id
  probe-rust emits as `charon-def-id`.
- **id-space caveat (must honor):** `functions[]` / `globals[]` / `trait_impls[]`
  are numbered in charon's separate `FunDeclId`/`GlobalDeclId`/`TraitImplId`
  spaces and their integers overlap (21 collisions on spqr). Only `functions[]`
  `def_id`s are `FunDeclId`s. The manifest path must build records from
  `functions[]` only (probe-aeneas already gates the consumer side with
  `def_id_is_fun_decl`).

What the LLBC has that the manifest does **not**, and why it doesn't block this:
- Coverage of non-translated functions (backlog, tests) — they have no Lean def
  to join to; they keep their SCIP-derived RQN.
- Rust visibility (`attr_info.public` → `is-public`) — probe-rust already
  derives `is-public` from SCIP; charon is only an override.
- The raw structured `Name` + decl tables — only needed because probe-rust
  re-renders the RQN itself; the manifest ships the rendered `rust_name`.

## Design: an enrichment-source seam

Today `enrich_atoms_with_charon_names(atoms, llbc_path, verbose)`:
1. `parse_llbc_names(llbc_path)` → `LlbcNames { by_match_key: HashMap<String, Vec<CharonFunInfo>>, charon_version }`
2. for each atom: match-key lookup + `resolve_charon_candidate` (span
   disambiguation) → stamp `rust_qualified_name`, `is_public`, `charon_def_id`,
   `charon_version`.

Step 2 is already source-agnostic — it only needs the `by_match_key` map + a
`charon_version`. So introduce **one producer choice** before it:

```
src/charon_enrichment.rs  (new seam, the ONLY place that picks a source)

  enum EnrichmentSource { Manifest, Llbc }   // for logging / test assertions

  struct Enrichment {
      by_match_key: HashMap<String, Vec<CharonFunInfo>>,
      charon_version: Option<String>,
      source: EnrichmentSource,
  }

  fn resolve_enrichment(
      translation_json: Option<&Path>,   // NEW
      llbc_path: Option<&Path>,          // legacy
  ) -> Result<Option<Enrichment>, String>
```

Precedence: `translation.json` if present → LLBC (legacy) → `None` (skip
enrichment). `enrich_atoms_with_charon_names` takes an `Enrichment` and becomes
fully source-blind (rename to `enrich_atoms(atoms, &Enrichment)`).

### Manifest arm — `records_from_translation_json`

Build the same `Vec<CharonFunInfo>` the LLBC arm builds, from `functions[]`
only:

| `CharonFunInfo` field | from `translation.json` |
|---|---|
| `qualified_name` | `rust_name` |
| `def_id` | `def_id` (a `FunDeclId`, since `functions[]`) |
| `match_key` | `make_match_key_from_charon(rust_name, crate)` — reused as-is |
| `file_path` | `source.file` |
| `line_start` / `line_end` | `source.begin_line` / `source.end_line` |
| `is_public` | `None` (absent from the manifest) |

`charon_version` = manifest top-level `charon_version`. Reuse
`resolve_charon_candidate` unchanged — the span disambiguation handles
same-line derive impls exactly as with the LLBC.

Spot-checked: for `mac_ct`, `translation.json` `rust_name`
(`spqr::authenticator::{...Authenticator}::mac_ct`) is identical to the
LLBC-derived qualified name, and `make_match_key_from_charon` reduces both to
`authenticator::mac_ct`, so match keys align even where the full RQN strings
diverge (the ~7% short-vs-brace renderings).

### Legacy arm — unchanged

`parse_llbc_names` + `charon_cache` generation, exactly as today. This arm still
runs charon and remains the source for non-manifest projects.

## Behavioral deltas to decide (call out in the PR)

1. **`rust-qualified-name` on translated atoms.** Reusing the enrich step as-is
   would overwrite the SCIP RQN with the manifest's `rust_name`.
   - *Pro:* it's the name Aeneas actually used; makes probe-aeneas name-matching
     exact; def-id join is unaffected either way.
   - *Con:* changes the RQN string other consumers see; `--with-public-api`
     matches `cargo-public-api` output by RQN and could be sensitive to format.
   - **Recommendation:** default to the **minimal** manifest path — stamp only
     `charon-def-id` + `charon-version`, leave `rust-qualified-name` as
     SCIP-derived. The join uses `def_id`, not names, so this delivers the
     feature with the least behavioral change. Make manifest-RQN-override an
     explicit follow-up if perfect name-matching is wanted.
2. **`is-public`.** The manifest lacks it, so the manifest path must **not**
   clobber the SCIP value with `None`. Fix `enrich` to only set `is_public` when
   `Some` (needed regardless; a latent bug on the current path too).
3. **Coverage.** Manifest-path runs enrich only over translated functions;
   non-translated atoms keep SCIP RQN and no `charon-def-id`. This is correct
   (nothing to join to) but means manifest projects no longer get
   charon-enriched RQN on out-of-scope functions. Acceptable for the Aeneas use
   case; document it.

## Interface / coordination with probe-aeneas

- **probe-rust CLI:** add `--translation <PATH>` to `extract`. When given (and
  readable), use the manifest arm and **skip charon entirely**. When absent, use
  `--with-charon`/LLBC as today.
- **probe-aeneas:** it already resolves the `translation.json` path
  (`translation_manifest::resolve_path`). Change the extract flow: when a
  manifest exists, pass `--translation` to `probe-rust extract` and **skip
  `ensure_charon_llbc`**. This removes the second charon run and makes WS2's
  regenerate-on-stale logic dead for manifest projects (it stays only for the
  legacy path / until removed).
- Net for a manifest project: charon runs **once** (in Aeneas), probe-rust
  consumes `translation.json`, def-ids match by construction, and the WS1
  provenance gate is satisfied trivially (same `charon_version`).

## Easy-removal structure (the seam contract)

Once `translation.json` is mandatory:
- delete the `Llbc` arm of `resolve_enrichment`, `parse_llbc_names`,
  `charon_cache` generation, and the `--with-charon` charon-run path;
- keep `records_from_translation_json`, `resolve_charon_candidate`, and the
  source-blind `enrich_atoms`.
Nothing downstream of the seam changes. This is the same containment
probe-aeneas achieved by funnelling record sources through `function_source`.

## Testing

- Unit: `records_from_translation_json` builds correct `CharonFunInfo`
  (functions-only, def_id = FunDeclId, span mapping); `resolve_enrichment`
  precedence (manifest > llbc > none); `enrich` no longer clobbers `is_public`
  with `None`.
- Integration: run `extract --translation <spqr manifest>` on cached spqr atoms;
  assert translated atoms get `charon-def-id` matching the manifest and
  `charon-version` = `0.1.217`, with **no charon process spawned**.
- A/B: manifest-path output vs legacy-path output on spqr should differ only by
  (a) `charon-def-id` values (now the manifest's `FunDeclId`s, matching the Lean
  side) and (b) whatever RQN-override decision is taken. Byte-compare the rest.
- Cross-repo: re-run the probe-aeneas activation A/B (finally unblocked — no
  charon 0.1.217 toolchain needed, since we consume the manifest directly).

## Sequencing

1. `enrich` `is_public` no-clobber fix (independent, safe).
2. Introduce the seam (`resolve_enrichment` + `Enrichment`), legacy arm wraps
   current behavior; no functional change. A/B byte-identical.
3. Add `records_from_translation_json` + `--translation` flag (minimal:
   def-id/version only).
4. probe-aeneas: pass `--translation`, skip `ensure_charon_llbc` when manifest
   present.
5. Activation A/B on spqr; recheck on a second manifest project.
6. Later: legacy removal when `translation.json` is required ecosystem-wide.

## Open questions

- Minimal vs RQN-override manifest path (decision #1) — recommend minimal first.
- Should probe-rust auto-detect `translation.json` from `aeneas-config.yml`
  (like probe-aeneas) as a convenience, or require the explicit `--translation`
  flag from probe-aeneas? Explicit is simpler and keeps probe-rust
  Aeneas-agnostic; auto-detect duplicates config parsing.
- Does any current consumer depend on `charon-def-id` being present for
  *non-translated* functions? (It never was — the field is new — so no.)
