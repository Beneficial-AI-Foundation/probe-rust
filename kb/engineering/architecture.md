# Architecture

- **last-updated**: 2026-04-07

## Overview

probe-rust is a Rust CLI tool that analyzes function call graphs in standard Rust projects. It produces Schema 2.0 JSON envelopes containing atom dictionaries with accurate line spans, dependency edges, and visibility metadata.

## Subcommands

| Command | Purpose | Envelope? |
|---------|---------|-----------|
| `extract` | Generate call graph atoms from SCIP index | Yes (`probe-rust/extract`) |
| `callee-crates` | BFS traversal from a function, group callees by crate | No |
| `list-functions` | List all functions via syn parsing | No |

## Extract pipeline

```
main.rs (CLI routing)
  |
  v
commands/extract.rs (orchestration)
  |
  |-- 1. validate_project()
  |      Checks for Cargo.toml; discovers project root
  |
  |-- 2. scip_cache::generate_scip_index()              [P14]
  |      rust-analyzer + scip CLI -> index.scip -> index.scip.json
  |      Cached in <project>/data/
  |
  |-- 3. lib.rs pipeline                                [P7, P8, P9, P10, P11, P16]
  |      parse_scip_json()          Parse SCIP JSON into ScipIndex
  |        -> build_call_graph()    Walk documents/symbols/occurrences
  |             -> FunctionNode map + symbol_to_display_name map
  |             -> module_visibility map (SCIP module-chain walk)
  |        -> convert_to_atoms_with_parsed_spans()
  |             rust_parser (syn)   Parse .rs files for function body spans
  |             classify_public_api()  is-public-api from module chain  [P11, P12]
  |             -> AtomWithLines map                    [P3, P5, P6]
  |
  |-- 4. find_duplicate_code_names() + dedupe into BTreeMap  [P2]
  |
  |-- 5. [optional] charon_cache + charon_names (--with-charon)  [P15]
  |      Enriches rust-qualified-name and is-public from Charon LLBC
  |
  |-- 6. add_external_stubs()                           [P4]
  |      Creates stub atoms for referenced-but-undefined functions
  |
  |-- 7. metadata::wrap_in_envelope()                   [P1, P13]
  |      Gathers git/cargo metadata, wraps atoms in Schema 2.0 envelope
  |
  v
.verilib/probes/rust_<pkg>_<ver>.json
```

## Component boundaries

### SCIP / rust-analyzer (external)

Source of all symbol definitions, occurrence ranges, kinds, and signature text. Not under probe-rust's control. Two known symbol formats for impl methods (P16).

**Files**: `scip_cache.rs`, `tool_manager.rs`

### Core call graph (internal)

Parses SCIP JSON, builds the function node graph, resolves disambiguation, converts to atoms. The heart of the tool.

**Files**: `lib.rs`, `constants.rs`

### syn / rust_parser (internal)

Refines function body end lines. SCIP only gives the definition location (function name); syn parses the actual source files to find where the function body ends.

**Files**: `rust_parser.rs`

### Charon (optional, external)

Provides precise Rust qualified names and item visibility via LLBC analysis. Failure is non-fatal ([P15](../engineering/properties.md#p15--charon-non-fatal)). Requires a nightly toolchain compatible with Charon's upstream requirements.

**Files**: `charon_cache.rs`, `charon_names.rs`

### Metadata / envelope (internal)

Gathers project metadata (git commit, repo URL, package info) and wraps atoms in the Schema 2.0 envelope.

**Files**: `metadata.rs`

### Path utilities (internal)

Path matching, suffix extraction, and score-based matching for correlating SCIP paths with filesystem paths.

**Files**: `path_utils.rs`

### Error handling (internal)

Unified error type covering all failure modes: SCIP parsing, I/O, JSON, source parsing, tool execution, duplicate code-names.

**Files**: `error.rs`

## Source file map

```
src/
  main.rs              CLI entry point, subcommand routing
  lib.rs               Core types, SCIP parsing, call graph, atom conversion
  constants.rs         SCIP kinds, role bits, tolerances, prefixes
  error.rs             ProbeError / ProbeResult
  metadata.rs          Schema 2.0 envelope, project metadata
  path_utils.rs        Path matching utilities
  rust_parser.rs       syn-based function span parsing
  scip_cache.rs        SCIP index generation and caching
  tool_manager.rs      Auto-download for scip CLI tool
  charon_cache.rs      Charon tool management and caching
  charon_names.rs      Charon LLBC name enrichment
  commands/
    mod.rs             Module declarations
    extract.rs         Extract command orchestration
    callee_crates.rs   Callee-crates command
    list_functions.rs  List-functions command
```

## External tool dependencies

| Tool | Required? | Auto-installable? | Purpose |
|------|-----------|-------------------|---------|
| rust-analyzer | Yes | No (must be on PATH or via rustup) | SCIP index generation |
| scip CLI | Yes | Yes (`--auto-install`) | Convert index.scip to JSON |
| Charon | No (`--with-charon`) | Yes (`--auto-install`) | Precise RQN and visibility (requires nightly) |

## Data flow

```
Rust project (source + Cargo.toml)
    |
    v
rust-analyzer ──> index.scip ──> scip CLI ──> index.scip.json
                                                    |
                                                    v
                                            parse_scip_json()
                                                    |
                                                    v
                                            build_call_graph()
                                              + module_visibility_map
                                                    |
                              .rs files ──> syn ──> span_map
                                                    |
                                                    v
                                    convert_to_atoms_with_parsed_spans()
                                      + classify_public_api() per atom
                                                    |
                                                    v
                                            BTreeMap<code-name, AtomWithLines>
                                                    |
                          [Charon LLBC] ──> enrich RQN + is-public
                                                    |
                                                    v
                                        Schema 2.0 envelope JSON
```
