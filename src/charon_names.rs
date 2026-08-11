//! Parse Charon LLBC files and extract function-level rust-qualified-names.
//!
//! Charon's LLBC (Low-Level Borrow Calculus) JSON encodes structured `Name`s
//! for every item.  A `Name` is a `Vec<PathElem>` where each `PathElem` is
//! either `Ident(name, disambiguator)` or `Impl(ImplElem)`.
//!
//! This module reconstructs the string form of those names so that
//! `probe-rust` atoms carry the same `rust-qualified-name` that Aeneas uses
//! during Lean translation.
//!
//! ## Matching strategy
//!
//! SCIP-derived atoms have a `code_path` (file) and a `display_name`
//! (e.g. `Scalar::from_bytes_mod_order`).  Charon names are fully qualified
//! paths like `curve25519_dalek::scalar::{curve25519_dalek::scalar::Scalar}::from_bytes_mod_order`.
//!
//! We match by building a lookup key `(module_suffix, bare_function_name)`:
//! - From the atom: `code_path = "src/scalar.rs"` -> module `scalar`,
//!   `display_name = "Scalar::from_bytes_mod_order"` -> bare fn `from_bytes_mod_order`.
//! - From the Charon name: strip the first `::` segment (always the crate
//!   name, which may differ from `translated.crate_name` for dependency
//!   crates pulled in via `--include`) and `{...}` impl blocks to get the
//!   same `scalar::from_bytes_mod_order` key.

use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CharonFunInfo {
    pub qualified_name: String,
    /// Charon `FunDeclId` (the `Fun` key in `item_names`). Equals Aeneas's
    /// `translation.json` `def_id`.
    pub def_id: u64,
    /// Match key: `module::bare_fn_name`, e.g. `scalar::from_bytes_mod_order`
    pub match_key: String,
    /// Source file path from the LLBC span, e.g. `src/scalar.rs`
    pub file_path: Option<String>,
    /// 0-based start line from the LLBC span
    pub line_start: Option<usize>,
    /// 0-based end line from the LLBC span
    pub line_end: Option<usize>,
    /// Whether the function is declared `pub` (from `item_meta.attr_info.public`)
    pub is_public: Option<bool>,
}

/// Parsed LLBC name data: function info grouped by match key, plus the LLBC's
/// top-level `charon_version` (the provenance for `charon-def-id`).
pub struct LlbcNames {
    pub by_match_key: HashMap<String, Vec<CharonFunInfo>>,
    /// Top-level `charon_version`, `None` if absent or non-string.
    pub charon_version: Option<String>,
}

/// Parse an LLBC JSON file into [`LlbcNames`].
pub fn parse_llbc_names(llbc_path: &Path) -> Result<LlbcNames, String> {
    let contents =
        std::fs::read_to_string(llbc_path).map_err(|e| format!("failed to read LLBC file: {e}"))?;
    let root: serde_json::Value = {
        let mut deserializer = serde_json::Deserializer::from_str(&contents);
        deserializer.disable_recursion_limit();
        let stacked = serde_stacker::Deserializer::new(&mut deserializer);
        serde::Deserialize::deserialize(stacked)
            .map_err(|e| format!("failed to parse LLBC JSON: {e}"))?
    };

    // Provenance: the version comes from the already-parsed root — no second
    // read of the multi-megabyte file needed.
    let charon_version = root
        .get("charon_version")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let translated = root
        .get("translated")
        .ok_or("missing 'translated' key in LLBC")?;

    let crate_name = translated
        .get("crate_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let item_names = translated
        .get("item_names")
        .and_then(|v| v.as_array())
        .ok_or("missing or invalid 'item_names'")?;

    let trait_decl_names = build_trait_decl_name_map(item_names);
    let trait_impl_to_decl = build_trait_impl_to_decl_map(translated);
    let type_decl_names = build_type_decl_name_map(item_names);
    let trait_impl_type_info = build_trait_impl_type_info_map(translated, &type_decl_names);

    let fun_spans = build_fun_span_map(translated);

    let mut result: HashMap<String, Vec<CharonFunInfo>> = HashMap::new();

    for entry in item_names {
        let key = match entry.get("key") {
            Some(k) => k,
            None => continue,
        };

        let fun_id = match key.get("Fun").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => continue,
        };

        let path_elems = match entry.get("value").and_then(|v| v.as_array()) {
            Some(v) => v,
            None => continue,
        };

        let qualified_name = format_name(
            path_elems,
            &trait_decl_names,
            &trait_impl_to_decl,
            &type_decl_names,
            &trait_impl_type_info,
        );

        let match_key = make_match_key_from_charon(&qualified_name, crate_name);
        let meta = fun_spans.get(&fun_id);

        result
            .entry(match_key.clone())
            .or_default()
            .push(CharonFunInfo {
                qualified_name,
                def_id: fun_id,
                match_key,
                file_path: meta.map(|m| m.file_path.clone()),
                line_start: meta.map(|m| m.line_start),
                line_end: meta.map(|m| m.line_end),
                is_public: meta.and_then(|m| m.is_public),
            });
    }

    Ok(LlbcNames {
        by_match_key: result,
        charon_version,
    })
}

// ---------------------------------------------------------------------------
// Name formatting (Charon -> string)
// ---------------------------------------------------------------------------

fn format_name(
    path_elems: &[serde_json::Value],
    trait_decl_names: &HashMap<u64, String>,
    trait_impl_to_decl: &HashMap<u64, u64>,
    type_decl_names: &HashMap<u64, String>,
    trait_impl_type_info: &HashMap<u64, TraitImplTypeInfo>,
) -> String {
    let mut parts = Vec::new();

    for pe in path_elems {
        if let Some(ident) = pe.get("Ident").and_then(|v| v.as_array()) {
            if let Some(name) = ident.first().and_then(|n| n.as_str()) {
                parts.push(name.to_string());
            }
        } else if let Some(impl_data) = pe.get("Impl") {
            if let Some(trait_impl_id) = impl_data.get("Trait").and_then(|v| v.as_u64()) {
                let trait_name = trait_impl_to_decl
                    .get(&trait_impl_id)
                    .and_then(|decl_id| trait_decl_names.get(decl_id))
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                if let Some(info) = trait_impl_type_info.get(&trait_impl_id) {
                    let trait_with_generics = if info.trait_generics.is_empty() {
                        trait_name.to_string()
                    } else {
                        format!("{}<{}>", trait_name, info.trait_generics.join(", "))
                    };
                    parts.push(format!("{{{trait_with_generics} for {}}}", info.self_type));
                } else {
                    parts.push(format!("{{impl {trait_name}}}"));
                }
            } else if let Some(ty_data) = impl_data.get("Ty") {
                if let Some(type_name) = resolve_impl_ty(ty_data, type_decl_names) {
                    parts.push(format!("{{{type_name}}}"));
                } else {
                    parts.push("{impl}".to_string());
                }
            }
        }
    }

    parts.join("::")
}

fn resolve_impl_ty(
    ty_data: &serde_json::Value,
    type_decl_names: &HashMap<u64, String>,
) -> Option<String> {
    let skip_binder = ty_data.get("skip_binder")?;
    // Handle both direct and Untagged-wrapped type representations
    let inner = skip_binder.get("Untagged").unwrap_or(skip_binder);
    let adt = inner.get("Adt")?;
    let adt_id = adt.get("id")?.get("Adt")?.as_u64()?;
    type_decl_names.get(&adt_id).cloned()
}

/// Resolve an LLBC type JSON value to a human-readable name.
/// Handles Adt (named types), Ref (references), and Literal (primitives).
/// Some Charon versions wrap types in an `Untagged` envelope; this is handled
/// transparently.
fn format_type(ty: &serde_json::Value, type_decl_names: &HashMap<u64, String>) -> Option<String> {
    // Unwrap the `Untagged` wrapper if present
    let ty = ty.get("Untagged").unwrap_or(ty);

    if let Some(adt) = ty.get("Adt") {
        let adt_id = adt.get("id")?.get("Adt")?.as_u64()?;
        return type_decl_names.get(&adt_id).cloned();
    }

    if let Some(ref_arr) = ty.get("Ref") {
        if let Some(arr) = ref_arr.as_array() {
            let inner_ty = arr.get(1)?;
            let inner_name = format_type(inner_ty, type_decl_names)?;

            let region_str = arr.first().and_then(|r| {
                r.get("Var")
                    .and_then(|v| v.get("Free"))
                    .and_then(|f| f.as_u64())
                    .map(|idx| format!("&'{idx} "))
            });
            let prefix = region_str.as_deref().unwrap_or("&");
            return Some(format!("{prefix}({inner_name})"));
        }
    }

    if let Some(lit) = ty.get("Literal") {
        if let Some(s) = lit.as_str() {
            let name = match s {
                "Bool" => "bool",
                "Char" => "char",
                _ => s,
            };
            return Some(name.to_string());
        }
    }

    if let Some(tv) = ty.get("TypeVar") {
        if let Some(free) = tv.get("Free").and_then(|v| v.as_u64()) {
            return Some(format!("T{free}"));
        }
    }

    None
}

/// Formatted trait impl info: Self type + any additional generic parameters.
struct TraitImplTypeInfo {
    self_type: String,
    /// Formatted types[1..] (e.g. Rhs, Output for `Mul<Rhs, Output>`).
    trait_generics: Vec<String>,
}

/// Build a map from TraitImpl def_id to the formatted Self type and trait generics.
/// Extracts `impl_trait.generics.types` from each trait_impl entry:
///   types[0] = Self type, types[1..] = trait generic parameters.
fn build_trait_impl_type_info_map(
    translated: &serde_json::Value,
    type_decl_names: &HashMap<u64, String>,
) -> HashMap<u64, TraitImplTypeInfo> {
    let mut map = HashMap::new();
    let trait_impls = match translated.get("trait_impls").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return map,
    };

    for ti in trait_impls {
        if ti.is_null() {
            continue;
        }
        let def_id = match ti.get("def_id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => continue,
        };
        let types = match ti
            .get("impl_trait")
            .and_then(|it| it.get("generics"))
            .and_then(|g| g.get("types"))
            .and_then(|t| t.as_array())
        {
            Some(arr) => arr,
            None => continue,
        };

        let self_type = match types
            .first()
            .and_then(|ty| format_type(ty, type_decl_names))
        {
            Some(name) => name,
            None => continue,
        };

        let trait_generics: Vec<String> = types
            .iter()
            .skip(1)
            .filter_map(|ty| format_type(ty, type_decl_names))
            .collect();

        map.insert(
            def_id,
            TraitImplTypeInfo {
                self_type,
                trait_generics,
            },
        );
    }

    map
}

// ---------------------------------------------------------------------------
// Match key generation
// ---------------------------------------------------------------------------

/// From a Charon qualified name like
/// `curve25519_dalek::scalar::{impl core::clone::Clone}::clone`,
/// produce a match key `scalar::clone` by stripping the crate prefix
/// and all `{...}` segments.
///
/// Strips the first `::` segment unconditionally (always the crate name)
/// so that included dependency crates (via `--include`) are handled
/// identically to the target crate.
fn make_match_key_from_charon(qualified_name: &str, _crate_name: &str) -> String {
    let without_crate = match qualified_name.find("::") {
        Some(idx) => &qualified_name[idx + 2..],
        None => qualified_name,
    };

    strip_impl_blocks(without_crate)
}

/// Produce a match key like `scalar::from_bytes_mod_order` for an atom.
///
/// When `rust_qualified_name` is available we derive the module path from it
/// directly — it carries inline `mod tests` / nested `mod inner` segments that
/// the filesystem path alone doesn't reveal. Without that, a `#[test] fn mul()`
/// inside `mod test` collapses to the same key as a `Scalar52::mul` impl method
/// in the same file, and the test atom silently inherits Charon's RQN for the
/// real method. Falls back to the filesystem path when RQN is absent.
fn make_match_key_from_atom(
    rust_qualified_name: Option<&str>,
    code_path: &str,
    display_name: &str,
) -> String {
    let bare_fn = bare_function_name(display_name);

    if let Some(rqn) = rust_qualified_name {
        if let Some(module) = module_from_rqn(rqn, display_name) {
            return if module.is_empty() {
                bare_fn.to_string()
            } else {
                format!("{module}::{bare_fn}")
            };
        }
    }

    let module = module_from_code_path(code_path);
    if module.is_empty() || module == "lib" {
        bare_fn.to_string()
    } else {
        format!("{module}::{bare_fn}")
    }
}

/// Extract the module path from a Rust-qualified name by stripping the crate
/// prefix and the trailing `::<display_name>` suffix.
///
/// Returns `None` when the RQN doesn't end in `display_name` (indicating the
/// caller should fall back to another derivation strategy).
fn module_from_rqn(rqn: &str, display_name: &str) -> Option<String> {
    let without_name = rqn
        .strip_suffix(display_name)
        .and_then(|s| s.strip_suffix("::"))?;
    match without_name.split_once("::") {
        Some((_crate, module)) => Some(module.to_string()),
        None => Some(String::new()),
    }
}

/// Build a fallback match key from `code_module` (which captures parent
/// function scoping that `code_path` alone cannot express).
///
/// Returns `None` when `code_module` is empty, refers to an external dep
/// (starts with `/`), or contains special characters from impl/generic
/// scopes (`<`, `>`, `(`) that don't correspond to Charon path segments.
fn make_match_key_from_code_module(code_module: &str, display_name: &str) -> Option<String> {
    if code_module.is_empty()
        || code_module.starts_with('/')
        || code_module.contains('<')
        || code_module.contains('>')
        || code_module.contains('(')
    {
        return None;
    }
    let module = code_module.replace('/', "::");
    let bare_fn = bare_function_name(display_name);
    Some(format!("{module}::{bare_fn}"))
}

/// Strip `{...}::` blocks from a path, e.g.
/// `scalar::{impl core::clone::Clone}::clone` -> `scalar::clone`
fn strip_impl_blocks(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut depth = 1;
            while depth > 0 {
                match chars.next() {
                    Some('{') => depth += 1,
                    Some('}') => depth -= 1,
                    None => break,
                    _ => {}
                }
            }
            // Skip any trailing `::`
            if chars.peek() == Some(&':') {
                chars.next();
                if chars.peek() == Some(&':') {
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Extract the module path from a SCIP-style code_path.
/// `src/scalar.rs` -> `scalar`
/// `curve25519-dalek/src/backend/serial/u64/field.rs` -> `backend::serial::u64::field`
fn module_from_code_path(code_path: &str) -> String {
    let file_path = if let Some(pos) = code_path.find("/src/") {
        &code_path[pos + 5..]
    } else if let Some(rest) = code_path.strip_prefix("src/") {
        rest
    } else {
        return String::new();
    };

    file_path
        .trim_end_matches(".rs")
        .trim_end_matches("/mod")
        .replace('/', "::")
}

/// Strip `Type::` prefix from display names to get the bare function name.
/// `Scalar::from_bytes_mod_order` -> `from_bytes_mod_order`
/// `free_function` -> `free_function`
fn bare_function_name(display_name: &str) -> &str {
    if let Some(pos) = display_name.rfind("::") {
        &display_name[pos + 2..]
    } else {
        display_name
    }
}

// ---------------------------------------------------------------------------
// Lookup table builders
// ---------------------------------------------------------------------------

/// Metadata extracted from a single `fun_decls[]` entry in LLBC.
struct FunDeclMeta {
    file_path: String,
    line_start: usize,
    line_end: usize,
    is_public: Option<bool>,
}

/// Build fun_id -> `FunDeclMeta` from `fun_decls` and `files`.
fn build_fun_span_map(translated: &serde_json::Value) -> HashMap<u64, FunDeclMeta> {
    let mut map = HashMap::new();

    let files = translated
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let fun_decls = match translated.get("fun_decls").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return map,
    };

    for fd in fun_decls {
        if fd.is_null() {
            continue;
        }
        let def_id = match fd.get("def_id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => continue,
        };
        let item_meta = match fd.get("item_meta") {
            Some(m) => m,
            None => continue,
        };
        let span_data = match item_meta.get("span").and_then(|s| s.get("data")) {
            Some(d) => d,
            None => continue,
        };
        let file_id = match span_data.get("file_id").and_then(|v| v.as_u64()) {
            Some(id) => id as usize,
            None => continue,
        };
        let beg_line = span_data
            .get("beg")
            .and_then(|b| b.get("line"))
            .and_then(|l| l.as_u64())
            .unwrap_or(0) as usize;
        let end_line = span_data
            .get("end")
            .and_then(|b| b.get("line"))
            .and_then(|l| l.as_u64())
            .unwrap_or(0) as usize;

        let is_public = item_meta
            .get("attr_info")
            .and_then(|a| a.get("public"))
            .and_then(|v| v.as_bool());

        if file_id >= files.len() {
            continue;
        }
        let file_name = files[file_id]
            .get("name")
            .and_then(|n| n.get("Local").or_else(|| n.get("Virtual")))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if !file_name.is_empty() {
            map.insert(
                def_id,
                FunDeclMeta {
                    file_path: file_name,
                    line_start: beg_line,
                    line_end: end_line,
                    is_public,
                },
            );
        }
    }

    map
}

fn build_trait_decl_name_map(item_names: &[serde_json::Value]) -> HashMap<u64, String> {
    let mut map = HashMap::new();
    for entry in item_names {
        if let Some(id) = entry
            .get("key")
            .and_then(|k| k.get("TraitDecl"))
            .and_then(|v| v.as_u64())
        {
            if let Some(path_elems) = entry.get("value").and_then(|v| v.as_array()) {
                let name = idents_joined(path_elems);
                map.insert(id, name);
            }
        }
    }
    map
}

fn build_type_decl_name_map(item_names: &[serde_json::Value]) -> HashMap<u64, String> {
    let mut map = HashMap::new();
    for entry in item_names {
        if let Some(id) = entry
            .get("key")
            .and_then(|k| k.get("Type"))
            .and_then(|v| v.as_u64())
        {
            if let Some(path_elems) = entry.get("value").and_then(|v| v.as_array()) {
                let name = idents_joined(path_elems);
                map.insert(id, name);
            }
        }
    }
    map
}

fn idents_joined(path_elems: &[serde_json::Value]) -> String {
    path_elems
        .iter()
        .filter_map(|pe| {
            pe.get("Ident")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|n| n.as_str())
        })
        .collect::<Vec<_>>()
        .join("::")
}

fn build_trait_impl_to_decl_map(translated: &serde_json::Value) -> HashMap<u64, u64> {
    let mut map = HashMap::new();
    if let Some(trait_impls) = translated.get("trait_impls").and_then(|v| v.as_array()) {
        for ti in trait_impls {
            if ti.is_null() {
                continue;
            }
            if let (Some(def_id), Some(trait_decl_id)) = (
                ti.get("def_id").and_then(|v| v.as_u64()),
                ti.get("impl_trait")
                    .and_then(|it| it.get("id"))
                    .and_then(|v| v.as_u64()),
            ) {
                map.insert(def_id, trait_decl_id);
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Source-path normalization
// ---------------------------------------------------------------------------

/// Strip leading package-name component so both
/// `"curve25519-dalek/src/foo.rs"` and `"src/foo.rs"` become `"src/foo.rs"`.
fn normalize_source_path(p: &str) -> &str {
    if let Some(idx) = p.find("/src/") {
        &p[idx + 1..]
    } else {
        p
    }
}

/// Pick the Charon candidate whose span best overlaps the atom's line range.
///
/// For **multi-line** Charon spans (`line_start < line_end`) the overlap is
/// `min(atom_end, c_end) - max(atom_start, c_start)`; the candidate with the
/// largest positive value wins.
///
/// For **single-line** spans (`line_start == line_end`) -- common when Charon
/// reports only the function signature line, especially for dependency crates
/// pulled in via `--include` -- the standard formula always yields zero.
/// Instead we use a containment check: if the single line falls within
/// `[atom_start, atom_end]` the overlap is 1; otherwise 0.
///
/// Returns `None` if no candidate has a positive overlap.
fn disambiguate_by_span<'a>(
    candidates: &'a [CharonFunInfo],
    atom_file: &str,
    atom_start: usize,
    atom_end: usize,
) -> Option<&'a CharonFunInfo> {
    if atom_start == 0 {
        return None;
    }

    let mut best: Option<&CharonFunInfo> = None;
    let mut best_overlap: i64 = 0;

    for c in candidates {
        let c_file = match c.file_path.as_deref() {
            Some(f) => normalize_source_path(f),
            None => continue,
        };
        if c_file != atom_file {
            continue;
        }
        let (c_start, c_end) = match (c.line_start, c.line_end) {
            (Some(s), Some(e)) if s > 0 => (s, e),
            _ => continue,
        };
        let overlap = if c_start == c_end {
            if c_start >= atom_start && c_start <= atom_end {
                1
            } else {
                0
            }
        } else {
            std::cmp::min(atom_end, c_end) as i64 - std::cmp::max(atom_start, c_start) as i64
        };
        if overlap > best_overlap {
            best_overlap = overlap;
            best = Some(c);
        }
    }

    best
}

// ---------------------------------------------------------------------------
// Enrichment: cross-reference atoms with Charon names
// ---------------------------------------------------------------------------

/// Try to resolve a single Charon candidate from `candidates`, using span
/// disambiguation and heuristic RQN matching as tiebreakers.
///
/// Returns the best `CharonFunInfo` or `None` if resolution fails.
///
/// For single candidates: validates file-path match and span overlap before
/// accepting. This prevents two mis-assignment patterns:
/// 1. Cross-file collisions (e.g. `subtle::{impl}::from` assigned to every
///    `lib.rs` atom whose match key is bare `from`).
/// 2. Same-file derive collisions (e.g. a `#[derive]` at L16 producing an
///    `eq` candidate that gets assigned to unrelated manual impls).
///
/// The single-candidate acceptance policy depends on `source`:
/// - [`EnrichmentSource::Llbc`] keeps the lenient legacy behavior: a candidate
///   with no usable file/span (empty file path, lines `0`) is still accepted on
///   match-key alone, so span-less compiler-generated items (e.g. derived
///   `fmt`) still enrich `rust-qualified-name`/`is-public`.
/// - [`EnrichmentSource::Manifest`] fails **closed**: a manifest `def_id` feeds
///   an integer Rust↔Lean join, so a single candidate is accepted only with a
///   non-empty file path that matches the atom (and span overlap when both
///   carry usable lines). A match-key-only "match" is not enough evidence to
///   stamp a `charon-def-id`; such candidates are rejected here.
fn resolve_charon_candidate<'a>(
    candidates: &'a [CharonFunInfo],
    atom: &crate::AtomWithLines,
    source: EnrichmentSource,
) -> Option<&'a CharonFunInfo> {
    if candidates.len() == 1 {
        let c = &candidates[0];
        // Manifest ids are consumed as a precise integer join, so never accept a
        // single candidate on match-key alone — require file-path proof. A
        // `source: null`/empty-file manifest record (file_path `None` or `""`)
        // is rejected rather than blindly stamped onto a same-key atom.
        let has_usable_file = matches!(c.file_path.as_deref(), Some(f) if !f.is_empty());
        if source == EnrichmentSource::Manifest && !has_usable_file {
            return None;
        }
        if let (Some(f), Some(cs), Some(ce)) = (c.file_path.as_deref(), c.line_start, c.line_end) {
            if f.is_empty() {
                // Only reachable for the LLBC path now (Manifest returned above).
                return Some(&candidates[0]);
            }
            let norm_c = normalize_source_path(f);
            let norm_a = normalize_source_path(&atom.code_path);
            if norm_c != norm_a {
                return None;
            }
            // Compare spans only when both the atom and the candidate carry
            // usable line numbers. A candidate line of 0 means "no usable span"
            // (matching `disambiguate_by_span`'s `s > 0` convention), so fall
            // back to accepting on the file-path match alone rather than
            // rejecting a span-less compiler-generated item.
            if atom.code_text.lines_start > 0 && cs > 0 && ce > 0 {
                let overlaps = if cs == ce {
                    cs >= atom.code_text.lines_start && cs <= atom.code_text.lines_end
                } else {
                    std::cmp::min(atom.code_text.lines_end, ce) as i64
                        - std::cmp::max(atom.code_text.lines_start, cs) as i64
                        > 0
                };
                if !overlaps {
                    return None;
                }
            }
        }
        return Some(&candidates[0]);
    }
    let norm_atom_path = normalize_source_path(&atom.code_path);
    if let Some(best) = disambiguate_by_span(
        candidates,
        norm_atom_path,
        atom.code_text.lines_start,
        atom.code_text.lines_end,
    ) {
        return Some(best);
    }
    let heuristic = atom.rust_qualified_name.as_deref().unwrap_or("");
    candidates.iter().find(|c| {
        let simplified = strip_impl_blocks(&c.qualified_name);
        simplified == heuristic
    })
}

/// Which source produced an [`Enrichment`] — for logging and test assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentSource {
    /// Aeneas `translation.json` (charon ran once, inside Aeneas).
    Manifest,
    /// charon LLBC (legacy; produced by a probe-rust-driven charon run).
    Llbc,
}

/// Source-blind charon enrichment data: Charon function info grouped by match
/// key, plus the producing charon version. [`enrich_atoms`] consumes this
/// without knowing where it came from. This is the seam that lets the LLBC path
/// be retired once every Aeneas project ships a `translation.json` — mirroring
/// probe-aeneas's `function_source` (Manifest vs legacy) split.
pub struct Enrichment {
    pub by_match_key: HashMap<String, Vec<CharonFunInfo>>,
    pub charon_version: Option<String>,
    pub source: EnrichmentSource,
}

impl Enrichment {
    /// Build from a charon LLBC file (legacy path). Fallible: reads and parses
    /// the multi-megabyte AST.
    pub fn from_llbc(llbc_path: &Path) -> Result<Self, String> {
        let parsed = parse_llbc_names(llbc_path)?;
        Ok(Enrichment {
            by_match_key: parsed.by_match_key,
            charon_version: parsed.charon_version,
            source: EnrichmentSource::Llbc,
        })
    }

    /// Build from an Aeneas `translation.json` — charon already ran once inside
    /// Aeneas, so no second charon run is needed.
    ///
    /// Only the manifest's `functions[]` array is used: its `def_id` is the
    /// charon `FunDeclId` (the id probe-rust joins on). `globals[]`/`trait_impls[]`
    /// live in charon's separate `GlobalDeclId`/`TraitImplId` spaces and their
    /// integers can collide with a `FunDeclId`, so they must not enter the map.
    /// Loop helpers share their parent's `def_id`, so it does not matter which
    /// family member an atom's span matches — the id is the same.
    pub fn from_translation_json(path: &Path) -> Result<Self, String> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Manifest {
            #[serde(default)]
            charon_version: Option<String>,
            #[serde(default)]
            functions: Vec<ManifestFn>,
        }
        #[derive(Deserialize)]
        struct ManifestFn {
            def_id: u64,
            #[serde(default)]
            rust_name: Option<String>,
            #[serde(default)]
            source: Option<ManifestSource>,
        }
        #[derive(Deserialize)]
        struct ManifestSource {
            file: String,
            #[serde(default)]
            begin_line: usize,
            #[serde(default)]
            end_line: usize,
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read translation.json: {e}"))?;
        let manifest: Manifest = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse translation.json: {e}"))?;

        let mut by_match_key: HashMap<String, Vec<CharonFunInfo>> = HashMap::new();
        for f in &manifest.functions {
            let Some(rust_name) = f.rust_name.as_deref() else {
                continue;
            };
            let match_key = make_match_key_from_charon(rust_name, "");
            let (file_path, line_start, line_end) = match &f.source {
                Some(s) => (Some(s.file.clone()), Some(s.begin_line), Some(s.end_line)),
                None => (None, None, None),
            };
            by_match_key
                .entry(match_key.clone())
                .or_default()
                .push(CharonFunInfo {
                    qualified_name: rust_name.to_string(),
                    def_id: f.def_id,
                    match_key,
                    file_path,
                    line_start,
                    line_end,
                    is_public: None, // the manifest carries no Rust visibility
                });
        }

        Ok(Enrichment {
            by_match_key,
            charon_version: manifest.charon_version,
            source: EnrichmentSource::Manifest,
        })
    }
}

/// Resolve the charon enrichment source, preferring the Aeneas `translation.json`
/// (no charon run) over a charon LLBC (legacy). Returns `Ok(None)` when neither
/// source is available. This is the single dispatch point; retiring the LLBC
/// path later means dropping the `llbc_path` arm here and the functions it calls.
pub fn resolve_enrichment(
    translation_json: Option<&Path>,
    llbc_path: Option<&Path>,
) -> Result<Option<Enrichment>, String> {
    if let Some(tj) = translation_json {
        return Enrichment::from_translation_json(tj).map(Some);
    }
    if let Some(llbc) = llbc_path {
        return Enrichment::from_llbc(llbc).map(Some);
    }
    Ok(None)
}

/// Enrich atoms by matching their `code_path` + `display_name` against the
/// resolved [`Enrichment`], stamping `rust-qualified-name`, `is-public`, and the
/// `charon-def-id`/`charon-version` provenance pair. Source-blind. Returns the
/// number of atoms enriched.
///
/// Uses a two-key strategy: first tries the `code_path`-based match key, then
/// falls back to a `code_module`-based key which captures parent-function
/// nesting (e.g. `decompress::step_2`) that file paths cannot express.
pub fn enrich_atoms(
    atoms: &mut std::collections::BTreeMap<String, crate::AtomWithLines>,
    enrichment: &Enrichment,
    verbose: bool,
) -> usize {
    let charon_map = &enrichment.by_match_key;
    let charon_version = &enrichment.charon_version;

    if verbose {
        eprintln!(
            "  {:?} enrichment: {} unique match-keys (charon {})",
            enrichment.source,
            charon_map.len(),
            charon_version.as_deref().unwrap_or("unknown"),
        );
    }

    let mut enriched = 0;

    for atom in atoms.values_mut() {
        // Provenance is (re)derived from THIS source alone. Clear both fields up
        // front so any atom that does not produce a fresh (id, version) this
        // pass — no match, or a match without a readable version — never keeps
        // stale provenance from an earlier enrichment. Upholds the coupling
        // invariant (both set, or both absent) across re-enrichment.
        atom.charon_def_id = None;
        atom.charon_version = None;

        if atom.code_path.is_empty() {
            continue;
        }

        let match_key = make_match_key_from_atom(
            atom.rust_qualified_name.as_deref(),
            &atom.code_path,
            &atom.display_name,
        );

        let candidates = charon_map.get(&match_key).or_else(|| {
            let module_key =
                make_match_key_from_code_module(&atom.code_module, &atom.display_name)?;
            if module_key == match_key {
                return None;
            }
            charon_map.get(&module_key)
        });

        if let Some(candidates) = candidates {
            if let Some(best) = resolve_charon_candidate(candidates, atom, enrichment.source) {
                // Manifest path (minimal): do not override `rust-qualified-name`
                // — the join keys on `charon-def-id`, and the atom keeps its
                // SCIP-derived RQN, avoiding a format change for consumers that
                // match on it (e.g. `--with-public-api`). The LLBC path keeps
                // overriding, as before.
                if enrichment.source == EnrichmentSource::Llbc {
                    atom.rust_qualified_name = Some(best.qualified_name.clone());
                }
                // Only override visibility when the source actually carries it:
                // the LLBC's `attr_info.public` may be absent, and the manifest
                // has no visibility at all. Overwriting with `None` would wipe
                // the SCIP-derived value.
                if let Some(is_public) = best.is_public {
                    atom.is_public = Some(is_public);
                }
                // Emit the def-id only together with its provenance version — a
                // def-id is meaningful only relative to the Charon run that
                // produced it. Both were pre-cleared above, so set them only
                // when a version is available; otherwise they stay absent.
                if let Some(version) = charon_version {
                    atom.charon_def_id = Some(best.def_id);
                    atom.charon_version = Some(version.clone());
                }
                enriched += 1;
            }
        }
    }

    enriched
}

/// Enrich atoms from a charon LLBC file (legacy convenience wrapper over
/// [`Enrichment::from_llbc`] + [`enrich_atoms`]). Returns the number enriched.
pub fn enrich_atoms_with_charon_names(
    atoms: &mut std::collections::BTreeMap<String, crate::AtomWithLines>,
    llbc_path: &Path,
    verbose: bool,
) -> Result<usize, String> {
    let enrichment = Enrichment::from_llbc(llbc_path)?;
    Ok(enrich_atoms(atoms, &enrichment, verbose))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_impl_blocks() {
        assert_eq!(
            strip_impl_blocks("scalar::{impl core::clone::Clone}::clone"),
            "scalar::clone"
        );
        assert_eq!(
            strip_impl_blocks("scalar::{curve25519_dalek::scalar::Scalar}::from_bytes_mod_order"),
            "scalar::from_bytes_mod_order"
        );
        assert_eq!(strip_impl_blocks("module::func"), "module::func");
    }

    #[test]
    fn test_bare_function_name() {
        assert_eq!(bare_function_name("Scalar::from_bytes"), "from_bytes");
        assert_eq!(bare_function_name("free_func"), "free_func");
        assert_eq!(bare_function_name("Type::method"), "method");
    }

    #[test]
    fn test_module_from_code_path() {
        assert_eq!(
            module_from_code_path("curve25519-dalek/src/backend/serial/u64/field.rs"),
            "backend::serial::u64::field"
        );
        assert_eq!(module_from_code_path("src/lib.rs"), "lib");
        assert_eq!(
            module_from_code_path("src/commands/extract.rs"),
            "commands::extract"
        );
        assert_eq!(module_from_code_path("some/path.rs"), "");
    }

    #[test]
    fn test_make_match_key_from_charon() {
        assert_eq!(
            make_match_key_from_charon(
                "curve25519_dalek::scalar::{curve25519_dalek::scalar::Scalar}::from_bytes_mod_order",
                "curve25519_dalek"
            ),
            "scalar::from_bytes_mod_order"
        );
        assert_eq!(
            make_match_key_from_charon(
                "curve25519_dalek::scalar::{core::clone::Clone for curve25519_dalek::scalar::Scalar}::clone",
                "curve25519_dalek"
            ),
            "scalar::clone"
        );
        assert_eq!(
            make_match_key_from_charon(
                "curve25519_dalek::scalar::{impl core::clone::Clone}::clone",
                "curve25519_dalek"
            ),
            "scalar::clone"
        );
        assert_eq!(
            make_match_key_from_charon(
                "curve25519_dalek::backend::get_selected_backend",
                "curve25519_dalek"
            ),
            "backend::get_selected_backend"
        );

        // Multi-crate LLBC: included dependency crate (crate_name differs from
        // the function's actual crate prefix).
        assert_eq!(
            make_match_key_from_charon(
                "libsignal_core::address::{ServiceId}::parse_from_service_id_binary",
                "signal_crypto"
            ),
            "address::parse_from_service_id_binary"
        );
        assert_eq!(
            make_match_key_from_charon("libsignal_core::address::try_scoped", "signal_crypto"),
            "address::try_scoped"
        );
        assert_eq!(
            make_match_key_from_charon("other_crate::module::{impl Trait}::method", "target_crate"),
            "module::method"
        );

        // Edge case: bare name with no `::` (no crate prefix to strip).
        assert_eq!(
            make_match_key_from_charon("standalone_fn", "some_crate"),
            "standalone_fn"
        );

        // Edge case: empty string input.
        assert_eq!(make_match_key_from_charon("", "some_crate"), "");
    }

    #[test]
    fn test_make_match_key_from_atom_fallback_path_based() {
        // Without an RQN, fall back to the filesystem-path-derived module.
        assert_eq!(
            make_match_key_from_atom(None, "src/scalar.rs", "Scalar::from_bytes_mod_order"),
            "scalar::from_bytes_mod_order"
        );
        assert_eq!(
            make_match_key_from_atom(None, "src/backend.rs", "get_selected_backend"),
            "backend::get_selected_backend"
        );
        assert_eq!(
            make_match_key_from_atom(
                None,
                "curve25519-dalek/src/backend/serial/u64/field.rs",
                "FieldElement51::reduce"
            ),
            "backend::serial::u64::field::reduce"
        );
    }

    #[test]
    fn test_make_match_key_from_atom_rqn_based() {
        // Impl method: RQN module strips the trailing "Type::method" suffix.
        assert_eq!(
            make_match_key_from_atom(
                Some("curve25519_dalek::backend::serial::u64::field::FieldElement51::reduce"),
                "curve25519-dalek/src/backend/serial/u64/field.rs",
                "FieldElement51::reduce",
            ),
            "backend::serial::u64::field::reduce"
        );
        // Free fn in submodule.
        assert_eq!(
            make_match_key_from_atom(Some("my_crate::foo::bar"), "src/foo.rs", "bar",),
            "foo::bar"
        );
        // Free fn at crate root.
        assert_eq!(
            make_match_key_from_atom(Some("my_crate::init"), "src/lib.rs", "init",),
            "init"
        );
    }

    /// Regression: a `#[test] fn mul()` inside `mod test` and a `Scalar52::mul`
    /// impl in the same file must produce distinct match keys. Otherwise the
    /// test atom silently inherits Charon's RQN for the real impl method,
    /// re-creating the RQN collision that breaks probe-aeneas's matching.
    #[test]
    fn test_make_match_key_from_atom_inline_test_module_distinct_from_impl() {
        let impl_key = make_match_key_from_atom(
            Some("curve25519_dalek::backend::serial::u64::scalar::Scalar52::mul"),
            "curve25519-dalek/src/backend/serial/u64/scalar.rs",
            "Scalar52::mul",
        );
        let test_key = make_match_key_from_atom(
            Some("curve25519_dalek::backend::serial::u64::scalar::test::mul"),
            "curve25519-dalek/src/backend/serial/u64/scalar.rs",
            "mul",
        );
        assert_eq!(impl_key, "backend::serial::u64::scalar::mul");
        assert_eq!(test_key, "backend::serial::u64::scalar::test::mul");
        assert_ne!(impl_key, test_key);
    }

    #[test]
    fn test_module_from_rqn() {
        assert_eq!(
            module_from_rqn("c::foo::bar", "bar").as_deref(),
            Some("foo")
        );
        assert_eq!(
            module_from_rqn("c::foo::Type::method", "Type::method").as_deref(),
            Some("foo")
        );
        assert_eq!(module_from_rqn("c::my_fn", "my_fn").as_deref(), Some(""));
        // Mismatch between RQN suffix and display_name → None (caller falls back).
        assert!(module_from_rqn("c::foo::bar", "baz").is_none());
    }

    #[test]
    fn test_format_name_ident_only() {
        let elems: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["func", 0]}]"#,
        )
        .unwrap();
        let name = format_name(
            &elems,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(name, "my_crate::module::func");
    }

    #[test]
    fn test_format_name_with_trait_impl_and_self_type() {
        let elems: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Ident": ["crate", 0]}, {"Impl": {"Trait": 5}}, {"Ident": ["method", 0]}]"#,
        )
        .unwrap();

        let mut trait_decl_names = HashMap::new();
        trait_decl_names.insert(10u64, "core::clone::Clone".to_string());

        let mut trait_impl_to_decl = HashMap::new();
        trait_impl_to_decl.insert(5u64, 10u64);

        let mut trait_impl_type_info = HashMap::new();
        trait_impl_type_info.insert(
            5u64,
            TraitImplTypeInfo {
                self_type: "my_crate::MyType".to_string(),
                trait_generics: vec![],
            },
        );

        let name = format_name(
            &elems,
            &trait_decl_names,
            &trait_impl_to_decl,
            &HashMap::new(),
            &trait_impl_type_info,
        );
        assert_eq!(
            name,
            "crate::{core::clone::Clone for my_crate::MyType}::method"
        );
    }

    #[test]
    fn test_format_name_with_trait_generics() {
        let elems: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Ident": ["crate", 0]}, {"Impl": {"Trait": 5}}, {"Ident": ["mul", 0]}]"#,
        )
        .unwrap();

        let mut trait_decl_names = HashMap::new();
        trait_decl_names.insert(10u64, "core::ops::arith::Mul".to_string());

        let mut trait_impl_to_decl = HashMap::new();
        trait_impl_to_decl.insert(5u64, 10u64);

        let mut trait_impl_type_info = HashMap::new();
        trait_impl_type_info.insert(
            5u64,
            TraitImplTypeInfo {
                self_type: "my_crate::EdwardsPoint".to_string(),
                trait_generics: vec![
                    "&'0 (my_crate::Scalar)".to_string(),
                    "my_crate::EdwardsPoint".to_string(),
                ],
            },
        );

        let name = format_name(
            &elems,
            &trait_decl_names,
            &trait_impl_to_decl,
            &HashMap::new(),
            &trait_impl_type_info,
        );
        assert_eq!(
            name,
            "crate::{core::ops::arith::Mul<&'0 (my_crate::Scalar), my_crate::EdwardsPoint> for my_crate::EdwardsPoint}::mul"
        );
    }

    #[test]
    fn test_format_name_with_trait_impl_no_self_type() {
        let elems: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Ident": ["crate", 0]}, {"Impl": {"Trait": 5}}, {"Ident": ["method", 0]}]"#,
        )
        .unwrap();

        let mut trait_decl_names = HashMap::new();
        trait_decl_names.insert(10u64, "core::clone::Clone".to_string());

        let mut trait_impl_to_decl = HashMap::new();
        trait_impl_to_decl.insert(5u64, 10u64);

        let name = format_name(
            &elems,
            &trait_decl_names,
            &trait_impl_to_decl,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(name, "crate::{impl core::clone::Clone}::method");
    }

    #[test]
    fn test_format_type_adt() {
        let ty: serde_json::Value = serde_json::from_str(
            r#"{"Adt": {"id": {"Adt": 3}, "generics": {"regions": [], "types": [], "const_generics": [], "trait_refs": []}}}"#,
        ).unwrap();
        let mut type_decl_names = HashMap::new();
        type_decl_names.insert(3u64, "my_crate::Scalar".to_string());
        assert_eq!(
            format_type(&ty, &type_decl_names),
            Some("my_crate::Scalar".to_string())
        );
    }

    #[test]
    fn test_format_type_ref() {
        let ty: serde_json::Value = serde_json::from_str(
            r#"{"Ref": [{"Var": {"Free": 0}}, {"Adt": {"id": {"Adt": 3}, "generics": {"regions": [], "types": [], "const_generics": [], "trait_refs": []}}}, "Shared"]}"#,
        ).unwrap();
        let mut type_decl_names = HashMap::new();
        type_decl_names.insert(3u64, "my_crate::Point".to_string());
        assert_eq!(
            format_type(&ty, &type_decl_names),
            Some("&'0 (my_crate::Point)".to_string())
        );
    }

    #[test]
    fn test_build_fun_span_map_extracts_visibility() {
        let llbc: serde_json::Value = serde_json::from_str(r#"{
            "files": [{"name": {"Local": "src/lib.rs"}}],
            "fun_decls": [
                {
                    "def_id": 0,
                    "item_meta": {
                        "span": {"data": {"file_id": 0, "beg": {"line": 10, "col": 0}, "end": {"line": 20, "col": 0}}},
                        "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                    }
                },
                {
                    "def_id": 1,
                    "item_meta": {
                        "span": {"data": {"file_id": 0, "beg": {"line": 30, "col": 0}, "end": {"line": 40, "col": 0}}},
                        "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                    }
                }
            ]
        }"#).unwrap();

        let map = build_fun_span_map(&llbc);
        assert_eq!(map.len(), 2);

        let pub_fn = map.get(&0).unwrap();
        assert_eq!(pub_fn.is_public, Some(true));
        assert_eq!(pub_fn.file_path, "src/lib.rs");

        let priv_fn = map.get(&1).unwrap();
        assert_eq!(priv_fn.is_public, Some(false));
    }

    #[test]
    fn test_parse_llbc_names_carries_visibility() {
        let dir = std::env::temp_dir().join("probe_rust_test_vis");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "charon_version": "0.1.217",
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["public_fn", 0]}]},
                    {"key": {"Fun": 1}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["private_fn", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 1, "col": 0}, "end": {"line": 5, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    },
                    {
                        "def_id": 1,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 10, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/lib.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let parsed = parse_llbc_names(&llbc_path).unwrap();
        assert_eq!(parsed.charon_version.as_deref(), Some("0.1.217"));
        let charon_map = parsed.by_match_key;

        let pub_entries = charon_map.get("public_fn").unwrap();
        assert_eq!(pub_entries.len(), 1);
        assert_eq!(pub_entries[0].is_public, Some(true));
        assert_eq!(pub_entries[0].qualified_name, "my_crate::public_fn");
        assert_eq!(pub_entries[0].def_id, 0);

        let priv_entries = charon_map.get("private_fn").unwrap();
        assert_eq!(priv_entries.len(), 1);
        assert_eq!(priv_entries[0].is_public, Some(false));
        assert_eq!(priv_entries[0].def_id, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parse_llbc_names_charon_version_present_and_absent() {
        let dir = std::env::temp_dir().join("probe_rust_test_charon_version");
        std::fs::create_dir_all(&dir).unwrap();

        // Present at top level -> extracted from the parsed root.
        let present = dir.join("with.llbc");
        std::fs::write(
            &present,
            r#"{"charon_version":"0.1.217","translated":{"item_names":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            parse_llbc_names(&present)
                .unwrap()
                .charon_version
                .as_deref(),
            Some("0.1.217")
        );

        // Absent -> None (the paired-omit path downstream drops the def-id too).
        let absent = dir.join("without.llbc");
        std::fs::write(&absent, r#"{"translated":{"item_names":[]}}"#).unwrap();
        assert_eq!(parse_llbc_names(&absent).unwrap().charon_version, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enrich_propagates_visibility() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_vis");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "charon_version": "0.1.99",
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 7}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["do_stuff", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 7,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/module.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/module/do_stuff()".to_string(),
            crate::AtomWithLines {
                display_name: "do_stuff".to_string(),
                code_name: "probe:my-crate/1.0/module/do_stuff()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module".to_string(),
                code_path: "src/module.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 5,
                    lines_end: 15,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1);

        let atom = atoms.get("probe:my-crate/1.0/module/do_stuff()").unwrap();
        assert_eq!(atom.is_public, Some(true));
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::module::do_stuff")
        );
        // WS3: the resolved FunDeclId and charon version ride along.
        assert_eq!(atom.charon_def_id, Some(7));
        assert_eq!(atom.charon_version.as_deref(), Some("0.1.99"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// When the LLBC has no top-level `charon_version`, the def-id must NOT be
    /// emitted on its own: a def-id is only interpretable relative to a Charon
    /// version, so an orphan id would violate the provenance contract. RQN /
    /// visibility (version-independent) are still enriched.
    #[test]
    fn test_enrich_omits_def_id_without_charon_version() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_no_version");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // No top-level `charon_version` field.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 7}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["do_stuff", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 7,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/module.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/module/do_stuff()".to_string(),
            crate::AtomWithLines {
                display_name: "do_stuff".to_string(),
                code_name: "probe:my-crate/1.0/module/do_stuff()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module".to_string(),
                code_path: "src/module.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 5,
                    lines_end: 15,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1);

        let atom = atoms.get("probe:my-crate/1.0/module/do_stuff()").unwrap();
        // Version-independent enrichment still happens.
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::module::do_stuff")
        );
        assert_eq!(atom.is_public, Some(true));
        // But no orphan def-id: both provenance fields stay absent together.
        assert_eq!(atom.charon_def_id, None);
        assert_eq!(atom.charon_version, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enrich_span_disambiguation_carries_visibility() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_span_disambig_vis");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["do_stuff", 0]}]},
                    {"key": {"Fun": 1}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["do_stuff", 1]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    },
                    {
                        "def_id": 1,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 30, "col": 0}, "end": {"line": 40, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/module.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/module/do_stuff()".to_string(),
            crate::AtomWithLines {
                display_name: "do_stuff".to_string(),
                code_name: "probe:my-crate/1.0/module/do_stuff()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module".to_string(),
                code_path: "src/module.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 31,
                    lines_end: 39,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1);

        let atom = atoms.get("probe:my-crate/1.0/module/do_stuff()").unwrap();
        assert_eq!(
            atom.is_public,
            Some(false),
            "span disambiguation should pick the private candidate (lines 30-40)"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enrich_span_disambiguation_single_line_charon_span() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_span_disambig_single");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Two Charon candidates share the same match key (module::do_stuff)
        // but have different single-line spans (line_start == line_end).
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Impl": [[], [], null]}, {"Ident": ["do_stuff", 0]}]},
                    {"key": {"Fun": 1}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Impl": [[], [], null]}, {"Ident": ["do_stuff", 1]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 10, "col": 0}, "end": {"line": 10, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    },
                    {
                        "def_id": 1,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 50, "col": 0}, "end": {"line": 50, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/module.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Atom whose body range [50, 60] contains only the second candidate (line 50).
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/module/do_stuff()".to_string(),
            crate::AtomWithLines {
                display_name: "do_stuff".to_string(),
                code_name: "probe:my-crate/1.0/module/do_stuff()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module".to_string(),
                code_path: "src/module.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 50,
                    lines_end: 60,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1, "single-line span should still allow enrichment");

        let atom = atoms.get("probe:my-crate/1.0/module/do_stuff()").unwrap();
        assert_eq!(
            atom.is_public,
            Some(false),
            "should pick the candidate at line 50 (private), not line 10 (public)"
        );
        assert!(
            atom.rust_qualified_name
                .as_ref()
                .is_some_and(|rqn| rqn.contains("do_stuff")),
            "atom should be enriched with a rust-qualified-name containing do_stuff"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enrich_skips_stubs_preserves_none() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_stub_vis");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [],
                "trait_impls": [],
                "fun_decls": [],
                "files": []
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:core/some_fn()".to_string(),
            crate::AtomWithLines {
                display_name: "some_fn".to_string(),
                code_name: "probe:core/some_fn()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "".to_string(),
                code_path: "".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 0,
                    lines_end: 0,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 0, "stubs should not be enriched");

        let atom = atoms.get("probe:core/some_fn()").unwrap();
        assert_eq!(atom.is_public, None, "stubs should retain is_public: None");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_make_match_key_from_code_module() {
        assert_eq!(
            make_match_key_from_code_module("ristretto/decompress", "step_2"),
            Some("ristretto::decompress::step_2".to_string())
        );
        assert_eq!(
            make_match_key_from_code_module("scalar", "batch_invert"),
            Some("scalar::batch_invert".to_string())
        );
        assert_eq!(
            make_match_key_from_code_module("", "step_2"),
            None,
            "empty code_module should return None"
        );
        assert_eq!(
            make_match_key_from_code_module("/github.com/rust-lang/core/iter", "next"),
            None,
            "external dep paths should return None"
        );
        assert_eq!(
            make_match_key_from_code_module("backend/serial/u64/field/impl<&[u8;", "from_bytes"),
            None,
            "impl-scoped code_module should return None"
        );
    }

    #[test]
    fn test_enrich_nested_function_via_code_module_fallback() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_nested_fn_fallback");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Charon LLBC: step_2 is nested under decompress in the name path.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["my_crate", 0]},
                        {"Ident": ["ristretto", 0]},
                        {"Ident": ["decompress", 0]},
                        {"Ident": ["step_2", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 297, "col": 0}, "end": {"line": 342, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/ristretto.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/ristretto/decompress/step_2()".to_string(),
            crate::AtomWithLines {
                display_name: "step_2".to_string(),
                code_name: "probe:my-crate/1.0/ristretto/decompress/step_2()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "ristretto/decompress".to_string(),
                code_path: "src/ristretto.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 297,
                    lines_end: 342,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: Some("my_crate::ristretto::step_2".to_string()),
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(
            count, 1,
            "nested function should match via code_module fallback"
        );

        let atom = atoms
            .get("probe:my-crate/1.0/ristretto/decompress/step_2()")
            .unwrap();
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::ristretto::decompress::step_2"),
            "should get the full Charon qualified name including parent fn"
        );
        assert_eq!(atom.is_public, Some(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Integration test for multi-crate LLBC enrichment (GitHub issue #7).
    /// The LLBC has `crate_name = "target_crate"` but contains a function
    /// from an included dependency crate (`dep_crate`).  The enrichment must
    /// match it to the corresponding atom despite the prefix mismatch.
    #[test]
    fn test_enrich_multi_crate_llbc_included_dependency() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_multi_crate_enrich");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "translated": {
                "crate_name": "target_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["target_crate", 0]},
                        {"Ident": ["module_a", 0]},
                        {"Ident": ["do_stuff", 0]}
                    ]},
                    {"key": {"Fun": 1}, "value": [
                        {"Ident": ["dep_crate", 0]},
                        {"Ident": ["module_b", 0]},
                        {"Ident": ["helper", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    },
                    {
                        "def_id": 1,
                        "item_meta": {
                            "span": {"data": {"file_id": 1, "beg": {"line": 10, "col": 0}, "end": {"line": 20, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [
                    {"name": {"Local": "src/module_a.rs"}},
                    {"name": {"Local": "dep/src/module_b.rs"}}
                ]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:target-crate/1.0/module_a/do_stuff()".to_string(),
            crate::AtomWithLines {
                display_name: "do_stuff".to_string(),
                code_name: "probe:target-crate/1.0/module_a/do_stuff()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module_a".to_string(),
                code_path: "src/module_a.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 5,
                    lines_end: 15,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );
        atoms.insert(
            "probe:dep-crate/0.1/module_b/helper()".to_string(),
            crate::AtomWithLines {
                display_name: "helper".to_string(),
                code_name: "probe:dep-crate/0.1/module_b/helper()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "module_b".to_string(),
                code_path: "dep/src/module_b.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 10,
                    lines_end: 20,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(
            count, 2,
            "both target-crate and dep-crate atoms should be enriched"
        );

        let target_atom = atoms
            .get("probe:target-crate/1.0/module_a/do_stuff()")
            .unwrap();
        assert_eq!(
            target_atom.rust_qualified_name.as_deref(),
            Some("target_crate::module_a::do_stuff")
        );
        assert_eq!(target_atom.is_public, Some(true));

        let dep_atom = atoms.get("probe:dep-crate/0.1/module_b/helper()").unwrap();
        assert_eq!(
            dep_atom.rust_qualified_name.as_deref(),
            Some("dep_crate::module_b::helper")
        );
        assert_eq!(dep_atom.is_public, Some(true));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_serde_roundtrip_is_public() {
        let atom = crate::AtomWithLines {
            display_name: "func".to_string(),
            code_name: "probe:c/1.0/m/func()".to_string(),
            dependencies: std::collections::BTreeSet::new(),
            dependencies_with_locations: Vec::new(),
            code_module: "m".to_string(),
            code_path: "src/m.rs".to_string(),
            code_text: crate::CodeTextInfo {
                lines_start: 1,
                lines_end: 10,
            },
            kind: crate::DeclKind::Exec,
            language: "rust".to_string(),
            rust_qualified_name: None,
            untracked: false,
            cfg: None,
            file_cfg: None,
            is_unmounted: false,
            is_foreign: false,
            trait_required: false,
            is_public: Some(true),
            is_public_api: None,
            charon_def_id: None,
            charon_version: None,
        };

        let json = serde_json::to_value(&atom).unwrap();
        assert_eq!(json.get("is-public"), Some(&serde_json::json!(true)));
        assert!(
            json.get("rust-qualified-name").is_none(),
            "None fields should be omitted"
        );
        assert!(
            json.get("is-public-api").is_none(),
            "None is_public_api should be omitted from JSON"
        );

        let atom_none = crate::AtomWithLines {
            is_public: None,
            ..crate::AtomWithLines {
                display_name: "func2".to_string(),
                code_name: "probe:c/1.0/m/func2()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "m".to_string(),
                code_path: "src/m.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 1,
                    lines_end: 10,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            }
        };

        let json_none = serde_json::to_value(&atom_none).unwrap();
        assert!(
            json_none.get("is-public").is_none(),
            "None is_public should be omitted from JSON"
        );

        let roundtripped: crate::AtomWithLines = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(roundtripped.is_public, Some(true));
        assert_eq!(roundtripped.is_public_api, None);
    }

    /// P15: Charon LLBC parse failure returns Err and atoms retain heuristic RQN
    #[test]
    fn test_charon_failure_is_non_fatal() {
        let mut atoms = std::collections::BTreeMap::new();
        atoms.insert(
            "probe:crate/0.1.0/mod/foo()".to_string(),
            crate::AtomWithLines {
                display_name: "foo".to_string(),
                code_name: "probe:crate/0.1.0/mod/foo()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "mod".to_string(),
                code_path: "src/mod.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 1,
                    lines_end: 10,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: Some("crate::mod::foo".to_string()),
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: Some(true),
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let bad_path = std::path::PathBuf::from("/nonexistent/path/llbc.json");
        let result = enrich_atoms_with_charon_names(&mut atoms, &bad_path, false);

        assert!(result.is_err(), "bad LLBC path should return Err");

        let atom = atoms.get("probe:crate/0.1.0/mod/foo()").unwrap();
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("crate::mod::foo"),
            "heuristic RQN should be preserved on Charon failure"
        );
    }

    #[test]
    fn test_resolve_single_candidate_cross_file_rejected() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_single_cross_file");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // One Charon candidate for match key "from", located in a dependency crate file.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["subtle", 0]},
                        {"Impl": [[], [], null]},
                        {"Ident": ["from", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 153, "col": 0}, "end": {"line": 153, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "/cargo/registry/src/subtle-2.6.1/src/lib.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Atom in a completely different file with match key "from" (bare).
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:device-transfer/0.1.0/impl<u8>#[KeyFormat]from()".to_string(),
            crate::AtomWithLines {
                display_name: "KeyFormat::from".to_string(),
                code_name: "probe:device-transfer/0.1.0/impl<u8>#[KeyFormat]from()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "".to_string(),
                code_path: "rust/device-transfer/src/lib.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 49,
                    lines_end: 54,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 0, "cross-file single candidate should be rejected");

        let atom = atoms
            .get("probe:device-transfer/0.1.0/impl<u8>#[KeyFormat]from()")
            .unwrap();
        assert!(
            atom.rust_qualified_name.is_none(),
            "atom should not be enriched from a candidate in a different file"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_resolve_single_candidate_same_file_span_mismatch_rejected() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_single_span_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Single Charon candidate: derive macro at L16 in address.rs producing "eq".
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["my_crate", 0]},
                        {"Ident": ["address", 0]},
                        {"Impl": [[], [], null]},
                        {"Ident": ["eq", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 16, "col": 0}, "end": {"line": 16, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/address.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Atom at L329-L340 (manual PartialEq impl), same file but non-overlapping.
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/address/ServiceId_eq()".to_string(),
            crate::AtomWithLines {
                display_name: "ServiceId::eq".to_string(),
                code_name: "probe:my-crate/1.0/address/ServiceId_eq()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "address".to_string(),
                code_path: "src/address.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 329,
                    lines_end: 340,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(
            count, 0,
            "same-file but non-overlapping single candidate should be rejected"
        );

        let atom = atoms
            .get("probe:my-crate/1.0/address/ServiceId_eq()")
            .unwrap();
        assert!(
            atom.rust_qualified_name.is_none(),
            "atom should not be enriched from a derive macro at a distant line"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_resolve_single_candidate_same_file_span_match() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_single_span_match");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Single Charon candidate with a span that overlaps the atom.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["my_crate", 0]},
                        {"Ident": ["address", 0]},
                        {"Impl": [[], [], null]},
                        {"Ident": ["from_uuid", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 100, "col": 0}, "end": {"line": 110, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/address.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/address/from_uuid()".to_string(),
            crate::AtomWithLines {
                display_name: "SpecificServiceId::from_uuid".to_string(),
                code_name: "probe:my-crate/1.0/address/from_uuid()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "address".to_string(),
                code_path: "src/address.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 98,
                    lines_end: 112,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1, "overlapping single candidate should enrich");

        let atom = atoms.get("probe:my-crate/1.0/address/from_uuid()").unwrap();
        assert!(
            atom.rust_qualified_name
                .as_ref()
                .is_some_and(|rqn| rqn.contains("from_uuid")),
            "atom should be enriched with the matching candidate"
        );
        assert_eq!(atom.is_public, Some(true));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_resolve_single_candidate_no_span_preserved() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_single_no_span");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Charon candidate with no usable file/span (file_id points to empty path,
        // lines are 0). This mimics compiler-generated functions.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["my_crate", 0]},
                        {"Ident": ["error", 0]},
                        {"Impl": [[], [], null]},
                        {"Ident": ["fmt", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 0, "col": 0}, "end": {"line": 0, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": false}
                        }
                    }
                ],
                "files": [{"name": {"Virtual": ""}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/error/SomeError_fmt()".to_string(),
            crate::AtomWithLines {
                display_name: "SomeError::fmt".to_string(),
                code_name: "probe:my-crate/1.0/error/SomeError_fmt()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "error".to_string(),
                code_path: "src/error.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 69,
                    lines_end: 82,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(
            count, 1,
            "candidate with no span data should still enrich (fallback)"
        );

        let atom = atoms
            .get("probe:my-crate/1.0/error/SomeError_fmt()")
            .unwrap();
        assert!(
            atom.rust_qualified_name
                .as_ref()
                .is_some_and(|rqn| rqn.contains("fmt")),
            "atom should be enriched when candidate has no span to validate against"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_resolve_single_candidate_real_file_zero_lines_accepted() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_single_real_file_zero_lines");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Charon reports a real, matching file path but no usable span (lines 0),
        // as happens for some compiler-generated / macro items. The `0` lines
        // must be treated as "no span" (like disambiguate_by_span's `s > 0`),
        // so the candidate is accepted on the file-path match alone.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [
                        {"Ident": ["my_crate", 0]},
                        {"Ident": ["error", 0]},
                        {"Impl": [[], [], null]},
                        {"Ident": ["fmt", 0]}
                    ]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 0, "col": 0}, "end": {"line": 0, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/error.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:my-crate/1.0/error/SomeError_fmt()".to_string(),
            crate::AtomWithLines {
                display_name: "SomeError::fmt".to_string(),
                code_name: "probe:my-crate/1.0/error/SomeError_fmt()".to_string(),
                dependencies: std::collections::BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module: "error".to_string(),
                code_path: "src/error.rs".to_string(),
                code_text: crate::CodeTextInfo {
                    lines_start: 69,
                    lines_end: 82,
                },
                kind: crate::DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
                untracked: false,
                cfg: None,
                file_cfg: None,
                is_unmounted: false,
                is_foreign: false,
                trait_required: false,
                is_public: None,
                is_public_api: None,
                charon_def_id: None,
                charon_version: None,
            },
        );

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(
            count, 1,
            "matching file with zero (unusable) span lines should still enrich"
        );

        let atom = atoms
            .get("probe:my-crate/1.0/error/SomeError_fmt()")
            .unwrap();
        assert!(
            atom.rust_qualified_name
                .as_ref()
                .is_some_and(|rqn| rqn.contains("fmt")),
            "span-less candidate in the matching file should be accepted (file-path fallback)"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A minimal atom for serde / enrichment tests.
    fn test_atom() -> crate::AtomWithLines {
        crate::AtomWithLines {
            display_name: "do_stuff".to_string(),
            code_name: "probe:c/1.0/m/do_stuff()".to_string(),
            dependencies: std::collections::BTreeSet::new(),
            dependencies_with_locations: Vec::new(),
            code_module: "m".to_string(),
            code_path: "src/m.rs".to_string(),
            code_text: crate::CodeTextInfo {
                lines_start: 5,
                lines_end: 15,
            },
            kind: crate::DeclKind::Exec,
            language: "rust".to_string(),
            rust_qualified_name: None,
            untracked: false,
            cfg: None,
            file_cfg: None,
            is_unmounted: false,
            is_foreign: false,
            trait_required: false,
            is_public: None,
            is_public_api: None,
            charon_def_id: None,
            charon_version: None,
        }
    }

    /// Serde contract: the fields serialize under their kebab-case keys, are
    /// omitted when `None`, and round-trip when `Some`.
    #[test]
    fn charon_provenance_fields_serde_shape() {
        // None -> keys omitted.
        let bare = serde_json::to_value(test_atom()).unwrap();
        assert!(bare.get("charon-def-id").is_none());
        assert!(bare.get("charon-version").is_none());

        // Some -> kebab-case keys with the values.
        let mut atom = test_atom();
        atom.charon_def_id = Some(439);
        atom.charon_version = Some("0.1.217".to_string());
        let v = serde_json::to_value(&atom).unwrap();
        assert_eq!(v.get("charon-def-id").and_then(|x| x.as_u64()), Some(439));
        assert_eq!(
            v.get("charon-version").and_then(|x| x.as_str()),
            Some("0.1.217")
        );

        // Round-trip.
        let back: crate::AtomWithLines = serde_json::from_value(v).unwrap();
        assert_eq!(back.charon_def_id, Some(439));
        assert_eq!(back.charon_version.as_deref(), Some("0.1.217"));
    }

    /// Serde contract for the source-fact fields: kebab-case keys, omitted
    /// when `None`/`false`, emitted and round-tripping when set.
    #[test]
    fn source_fact_fields_serde_shape() {
        // Unset -> keys omitted entirely.
        let bare = serde_json::to_value(test_atom()).unwrap();
        for key in ["file-cfg", "is-unmounted", "is-foreign", "trait-required"] {
            assert!(bare.get(key).is_none(), "{key} must be omitted");
        }

        // Set -> kebab-case keys with the values.
        let mut atom = test_atom();
        atom.cfg = Some("all(feature = \"x\", test)".to_string());
        atom.file_cfg = Some("feature = \"x\"".to_string());
        atom.is_unmounted = true;
        atom.is_foreign = true;
        atom.trait_required = true;
        let v = serde_json::to_value(&atom).unwrap();
        assert_eq!(
            v.get("file-cfg").and_then(|x| x.as_str()),
            Some("feature = \"x\"")
        );
        assert_eq!(v.get("is-unmounted").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(v.get("is-foreign").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(
            v.get("trait-required").and_then(|x| x.as_bool()),
            Some(true)
        );

        // Round-trip.
        let back: crate::AtomWithLines = serde_json::from_value(v).unwrap();
        assert_eq!(back.file_cfg.as_deref(), Some("feature = \"x\""));
        assert!(back.is_unmounted);
        assert!(back.is_foreign);
        assert!(back.trait_required);
    }

    /// Re-enrichment safety: an atom that already carries provenance from an
    /// earlier run must have BOTH fields cleared when the current LLBC has no
    /// `charon_version` — never a stale id next to a freshly-updated RQN.
    #[test]
    fn test_enrich_clears_stale_provenance_when_version_missing() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_clear_stale");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // LLBC with a matching function but NO top-level charon_version.
        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 7}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["m", 0]}, {"Ident": ["do_stuff", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 7,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/m.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Pre-set stale provenance from a hypothetical earlier run.
        let mut atom = test_atom();
        atom.charon_def_id = Some(999);
        atom.charon_version = Some("0.0.1-stale".to_string());
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1);

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        // RQN re-enriched, but the stale id/version are both cleared.
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::m::do_stuff")
        );
        assert_eq!(atom.charon_def_id, None, "stale def-id must be cleared");
        assert_eq!(atom.charon_version, None, "stale version must be cleared");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn from_translation_json_builds_functions_only_records() {
        let dir = std::env::temp_dir().join("probe_rust_test_manifest_records");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("translation.json");
        // globals/trait_impls share integers with functions (0, 5) — they must
        // NOT enter the map (separate charon id spaces).
        std::fs::write(
            &path,
            r#"{
                "charon_version": "0.1.217",
                "crate": "my_crate",
                "functions": [
                    {"def_id": 0, "rust_name": "my_crate::m::do_stuff",
                     "source": {"file": "src/m.rs", "begin_line": 5, "end_line": 15}}
                ],
                "globals": [ {"def_id": 0, "rust_name": "my_crate::m::CONST"} ],
                "trait_impls": [ {"def_id": 5, "rust_name": "my_crate::{impl T for X}"} ]
            }"#,
        )
        .unwrap();

        let e = Enrichment::from_translation_json(&path).unwrap();
        assert_eq!(e.source, EnrichmentSource::Manifest);
        assert_eq!(e.charon_version.as_deref(), Some("0.1.217"));
        // Exactly one record, from functions[]; globals/trait_impls excluded.
        let all: Vec<_> = e.by_match_key.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].def_id, 0);
        assert_eq!(all[0].file_path.as_deref(), Some("src/m.rs"));
        assert_eq!(all[0].line_start, Some(5));
        assert!(e.by_match_key.contains_key("m::do_stuff"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_enrichment_prefers_manifest_over_llbc() {
        let dir = std::env::temp_dir().join("probe_rust_test_resolve_precedence");
        std::fs::create_dir_all(&dir).unwrap();
        let tj = dir.join("translation.json");
        std::fs::write(
            &tj,
            r#"{"charon_version":"0.1.217","crate":"c","functions":[]}"#,
        )
        .unwrap();
        let llbc = dir.join("charon.llbc");
        std::fs::write(
            &llbc,
            r#"{"charon_version":"0.1.174","translated":{"item_names":[]}}"#,
        )
        .unwrap();

        // Both present -> manifest wins.
        let e = resolve_enrichment(Some(&tj), Some(&llbc)).unwrap().unwrap();
        assert_eq!(e.source, EnrichmentSource::Manifest);
        assert_eq!(e.charon_version.as_deref(), Some("0.1.217"));

        // LLBC only.
        let e = resolve_enrichment(None, Some(&llbc)).unwrap().unwrap();
        assert_eq!(e.source, EnrichmentSource::Llbc);

        // Neither -> None.
        assert!(resolve_enrichment(None, None).unwrap().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manifest_enrich_stamps_def_id_and_keeps_scip_rqn() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_manifest_enrich");
        std::fs::create_dir_all(&dir).unwrap();
        let tj = dir.join("translation.json");
        std::fs::write(
            &tj,
            r#"{
                "charon_version": "0.1.217",
                "crate": "my_crate",
                "functions": [
                    {"def_id": 82, "rust_name": "my_crate::m::do_stuff",
                     "source": {"file": "src/m.rs", "begin_line": 5, "end_line": 15}}
                ]
            }"#,
        )
        .unwrap();

        // Atom with a SCIP-derived RQN that differs from the manifest rendering.
        let mut atom = test_atom();
        atom.rust_qualified_name = Some("my_crate::m::do_stuff_SCIP".to_string());
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let e = Enrichment::from_translation_json(&tj).unwrap();
        let count = enrich_atoms(&mut atoms, &e, false);
        assert_eq!(count, 1);

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        // Minimal flavor: def-id + version stamped, RQN left as SCIP-derived.
        assert_eq!(atom.charon_def_id, Some(82));
        assert_eq!(atom.charon_version.as_deref(), Some("0.1.217"));
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::m::do_stuff_SCIP"),
            "manifest path must not override the SCIP rust-qualified-name"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The enrich step must not clobber a SCIP-derived `is-public` with `None`
    /// when the matched Charon candidate carries no visibility (LLBC entry
    /// without `attr_info.public`, or a version-less/manifest source).
    #[test]
    fn test_enrich_does_not_clobber_is_public_with_none() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_pub_noclobber");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Matching function has a span but NO attr_info.public -> is_public None.
        let llbc_json = r#"{
            "charon_version": "0.1.217",
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["m", 0]}, {"Ident": ["do_stuff", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 5, "col": 0}, "end": {"line": 15, "col": 0}}}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/m.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Atom already has is-public from SCIP.
        let mut atom = test_atom();
        atom.is_public = Some(false);
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 1);

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        // RQN + def-id enriched, but the SCIP is-public survives.
        assert_eq!(
            atom.rust_qualified_name.as_deref(),
            Some("my_crate::m::do_stuff")
        );
        assert_eq!(atom.charon_def_id, Some(0));
        assert_eq!(
            atom.is_public,
            Some(false),
            "SCIP is-public must not be clobbered by a candidate without visibility"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Re-enrichment safety: an atom that matched a prior LLBC but matches
    /// NOTHING in the current one must have its stale provenance cleared — the
    /// coupling invariant holds on the no-match path, not just no-version.
    #[test]
    fn test_enrich_clears_stale_provenance_on_no_match() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_clear_no_match");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        // Valid LLBC (with a version) but NO function matching our atom.
        let llbc_json = r#"{
            "charon_version": "0.1.217",
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["other", 0]}, {"Ident": ["unrelated", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
                        "item_meta": {
                            "span": {"data": {"file_id": 0, "beg": {"line": 1, "col": 0}, "end": {"line": 2, "col": 0}}},
                            "attr_info": {"attributes": [], "inline": null, "rename": null, "public": true}
                        }
                    }
                ],
                "files": [{"name": {"Local": "src/other.rs"}}]
            }
        }"#;
        std::fs::write(&llbc_path, llbc_json).unwrap();

        // Atom carries stale provenance and lives in a file the LLBC doesn't cover.
        let mut atom = test_atom();
        atom.charon_def_id = Some(999);
        atom.charon_version = Some("0.0.1-stale".to_string());
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let count = enrich_atoms_with_charon_names(&mut atoms, &llbc_path, false).unwrap();
        assert_eq!(count, 0, "no function matches this atom");

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        assert_eq!(
            atom.charon_def_id, None,
            "stale def-id must be cleared even when nothing matched"
        );
        assert_eq!(atom.charon_version, None, "stale version must be cleared");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `from_translation_json` errors (not panics) on an unreadable path and on
    /// malformed JSON — the CLI relies on this to warn-and-skip rather than
    /// crash.
    #[test]
    fn from_translation_json_errors_on_bad_path_and_bad_json() {
        let dir = std::env::temp_dir().join("probe_rust_test_manifest_errors");
        std::fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("does_not_exist.json");
        assert!(Enrichment::from_translation_json(&missing).is_err());

        let bad = dir.join("bad.json");
        std::fs::write(&bad, r#"{ this is not valid json "#).unwrap();
        assert!(Enrichment::from_translation_json(&bad).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A `functions[]` entry with a null/absent `rust_name` is silently skipped
    /// (no match key to build), and a manifest with no `functions` key yields an
    /// empty map but still carries the top-level `charon_version`.
    #[test]
    fn from_translation_json_skips_null_rust_name_and_missing_functions() {
        let dir = std::env::temp_dir().join("probe_rust_test_manifest_skip");
        std::fs::create_dir_all(&dir).unwrap();

        // One usable function + one with rust_name: null (must be skipped).
        let with_null = dir.join("with_null.json");
        std::fs::write(
            &with_null,
            r#"{
                "charon_version": "0.1.217",
                "functions": [
                    {"def_id": 1, "rust_name": null,
                     "source": {"file": "src/m.rs", "begin_line": 5, "end_line": 15}},
                    {"def_id": 2, "rust_name": "my_crate::m::keep",
                     "source": {"file": "src/m.rs", "begin_line": 20, "end_line": 30}}
                ]
            }"#,
        )
        .unwrap();
        let e = Enrichment::from_translation_json(&with_null).unwrap();
        let all: Vec<_> = e.by_match_key.values().flatten().collect();
        assert_eq!(all.len(), 1, "the null-rust_name entry must be dropped");
        assert_eq!(all[0].def_id, 2);

        // No `functions` key at all -> empty map, version preserved.
        let no_functions = dir.join("no_functions.json");
        std::fs::write(&no_functions, r#"{"charon_version": "0.1.217"}"#).unwrap();
        let e = Enrichment::from_translation_json(&no_functions).unwrap();
        assert!(e.by_match_key.is_empty());
        assert_eq!(e.charon_version.as_deref(), Some("0.1.217"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Guard: a manifest single candidate with no usable `source` (file_path
    /// `None`) must be REJECTED — a match-key-only hit is not enough evidence to
    /// stamp a `charon-def-id` that feeds an integer Rust↔Lean join. Contrast
    /// with the LLBC path, which accepts span-less candidates (see
    /// `test_resolve_single_candidate_no_span_preserved`).
    #[test]
    fn manifest_single_candidate_without_source_is_rejected() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_manifest_no_source");
        std::fs::create_dir_all(&dir).unwrap();
        let tj = dir.join("translation.json");
        // Function shares the atom's match key (`m::do_stuff`) but has no source.
        std::fs::write(
            &tj,
            r#"{
                "charon_version": "0.1.217",
                "functions": [
                    {"def_id": 82, "rust_name": "my_crate::m::do_stuff"}
                ]
            }"#,
        )
        .unwrap();

        let atom = test_atom();
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let e = Enrichment::from_translation_json(&tj).unwrap();
        // The candidate is present in the map, keyed the same as the atom...
        assert!(e.by_match_key.contains_key("m::do_stuff"));
        let count = enrich_atoms(&mut atoms, &e, false);
        // ...but it is refused for lack of file-path proof.
        assert_eq!(count, 0, "sourceless manifest candidate must not enrich");

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        assert_eq!(atom.charon_def_id, None);
        assert_eq!(atom.charon_version, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Guard: a manifest single candidate whose `source.file` points at a
    /// different file than the atom must be rejected (cross-file collision),
    /// even though the coarse match key collides.
    #[test]
    fn manifest_single_candidate_wrong_file_is_rejected() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_manifest_wrong_file");
        std::fs::create_dir_all(&dir).unwrap();
        let tj = dir.join("translation.json");
        std::fs::write(
            &tj,
            r#"{
                "charon_version": "0.1.217",
                "functions": [
                    {"def_id": 82, "rust_name": "my_crate::m::do_stuff",
                     "source": {"file": "src/other.rs", "begin_line": 5, "end_line": 15}}
                ]
            }"#,
        )
        .unwrap();

        // Atom lives in src/m.rs (see `test_atom`), candidate in src/other.rs.
        let atom = test_atom();
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let e = Enrichment::from_translation_json(&tj).unwrap();
        let count = enrich_atoms(&mut atoms, &e, false);
        assert_eq!(count, 0, "cross-file manifest candidate must not enrich");

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        assert_eq!(atom.charon_def_id, None);
        assert_eq!(atom.charon_version, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Re-enrichment safety on the MANIFEST path: an atom carrying stale
    /// provenance that matches nothing in the current manifest must have both
    /// fields cleared (the coupling invariant is enforced source-blind).
    #[test]
    fn manifest_re_enrich_clears_stale_provenance_on_no_match() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_manifest_clear_stale");
        std::fs::create_dir_all(&dir).unwrap();
        let tj = dir.join("translation.json");
        // A manifest that covers an unrelated function only.
        std::fs::write(
            &tj,
            r#"{
                "charon_version": "0.1.217",
                "functions": [
                    {"def_id": 3, "rust_name": "my_crate::other::unrelated",
                     "source": {"file": "src/other.rs", "begin_line": 1, "end_line": 2}}
                ]
            }"#,
        )
        .unwrap();

        let mut atom = test_atom();
        atom.charon_def_id = Some(999);
        atom.charon_version = Some("0.0.1-stale".to_string());
        let mut atoms = BTreeMap::new();
        atoms.insert(atom.code_name.clone(), atom);

        let e = Enrichment::from_translation_json(&tj).unwrap();
        let count = enrich_atoms(&mut atoms, &e, false);
        assert_eq!(count, 0, "no manifest function matches this atom");

        let atom = atoms.get("probe:c/1.0/m/do_stuff()").unwrap();
        assert_eq!(
            atom.charon_def_id, None,
            "stale def-id must be cleared on the manifest no-match path"
        );
        assert_eq!(atom.charon_version, None, "stale version must be cleared");

        std::fs::remove_dir_all(&dir).ok();
    }
}
