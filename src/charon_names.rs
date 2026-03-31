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
//! - From the Charon name: strip crate prefix and `{...}` blocks to get the
//!   same `scalar::from_bytes_mod_order` key.

use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CharonFunInfo {
    pub qualified_name: String,
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

/// Parse an LLBC JSON file and return Charon function info grouped by match key.
pub fn parse_llbc_names(llbc_path: &Path) -> Result<HashMap<String, Vec<CharonFunInfo>>, String> {
    let contents =
        std::fs::read_to_string(llbc_path).map_err(|e| format!("failed to read LLBC file: {e}"))?;
    let root: serde_json::Value = {
        let mut deserializer = serde_json::Deserializer::from_str(&contents);
        deserializer.disable_recursion_limit();
        let stacked = serde_stacker::Deserializer::new(&mut deserializer);
        serde::Deserialize::deserialize(stacked)
            .map_err(|e| format!("failed to parse LLBC JSON: {e}"))?
    };

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
                match_key,
                file_path: meta.map(|m| m.file_path.clone()),
                line_start: meta.map(|m| m.line_start),
                line_end: meta.map(|m| m.line_end),
                is_public: meta.and_then(|m| m.is_public),
            });
    }

    Ok(result)
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
fn make_match_key_from_charon(qualified_name: &str, crate_name: &str) -> String {
    let without_crate = qualified_name
        .strip_prefix(crate_name)
        .and_then(|s| s.strip_prefix("::"))
        .unwrap_or(qualified_name);

    strip_impl_blocks(without_crate)
}

/// From an atom's `code_path` and `display_name`, produce a match key
/// like `scalar::from_bytes_mod_order`.
fn make_match_key_from_atom(code_path: &str, display_name: &str) -> String {
    let module = module_from_code_path(code_path);
    let bare_fn = bare_function_name(display_name);

    if module.is_empty() || module == "lib" {
        bare_fn.to_string()
    } else {
        format!("{module}::{bare_fn}")
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

/// Pick the Charon candidate whose span best matches the atom's location.
/// Returns `None` if no candidate has a positive line overlap.
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
        let overlap =
            std::cmp::min(atom_end, c_end) as i64 - std::cmp::max(atom_start, c_start) as i64;
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
fn resolve_charon_candidate<'a>(
    candidates: &'a [CharonFunInfo],
    atom: &crate::AtomWithLines,
) -> Option<&'a CharonFunInfo> {
    if candidates.len() == 1 {
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

/// Enrich atoms by matching their `code_path` + `display_name` against
/// Charon LLBC names. Returns the number of atoms enriched.
///
/// Uses a two-key strategy: first tries `code_path`-based match key, then
/// falls back to `code_module`-based key which captures parent-function
/// nesting (e.g. `decompress::step_2`) that file paths cannot express.
pub fn enrich_atoms_with_charon_names(
    atoms: &mut std::collections::BTreeMap<String, crate::AtomWithLines>,
    llbc_path: &Path,
    verbose: bool,
) -> Result<usize, String> {
    let charon_map = parse_llbc_names(llbc_path)?;

    if verbose {
        eprintln!(
            "  Charon LLBC has {} unique match-keys for functions",
            charon_map.len()
        );
    }

    let mut enriched = 0;

    for atom in atoms.values_mut() {
        if atom.code_path.is_empty() {
            continue;
        }

        let match_key = make_match_key_from_atom(&atom.code_path, &atom.display_name);

        let candidates = charon_map.get(&match_key).or_else(|| {
            let module_key =
                make_match_key_from_code_module(&atom.code_module, &atom.display_name)?;
            if module_key == match_key {
                return None;
            }
            charon_map.get(&module_key)
        });

        if let Some(candidates) = candidates {
            if let Some(best) = resolve_charon_candidate(candidates, atom) {
                atom.rust_qualified_name = Some(best.qualified_name.clone());
                atom.is_public = best.is_public;
                enriched += 1;
            }
        }
    }

    Ok(enriched)
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
    }

    #[test]
    fn test_make_match_key_from_atom() {
        assert_eq!(
            make_match_key_from_atom("src/scalar.rs", "Scalar::from_bytes_mod_order"),
            "scalar::from_bytes_mod_order"
        );
        assert_eq!(
            make_match_key_from_atom("src/backend.rs", "get_selected_backend"),
            "backend::get_selected_backend"
        );
        assert_eq!(
            make_match_key_from_atom(
                "curve25519-dalek/src/backend/serial/u64/field.rs",
                "FieldElement51::reduce"
            ),
            "backend::serial::u64::field::reduce"
        );
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

        let charon_map = parse_llbc_names(&llbc_path).unwrap();

        let pub_entries = charon_map.get("public_fn").unwrap();
        assert_eq!(pub_entries.len(), 1);
        assert_eq!(pub_entries[0].is_public, Some(true));
        assert_eq!(pub_entries[0].qualified_name, "my_crate::public_fn");

        let priv_entries = charon_map.get("private_fn").unwrap();
        assert_eq!(priv_entries.len(), 1);
        assert_eq!(priv_entries[0].is_public, Some(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enrich_propagates_visibility() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("probe_rust_test_enrich_vis");
        std::fs::create_dir_all(&dir).unwrap();
        let llbc_path = dir.join("test.llbc");

        let llbc_json = r#"{
            "translated": {
                "crate_name": "my_crate",
                "item_names": [
                    {"key": {"Fun": 0}, "value": [{"Ident": ["my_crate", 0]}, {"Ident": ["module", 0]}, {"Ident": ["do_stuff", 0]}]}
                ],
                "trait_impls": [],
                "fun_decls": [
                    {
                        "def_id": 0,
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
                is_disabled: false,
                is_public: None,
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
                is_disabled: false,
                is_public: None,
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
                is_disabled: false,
                is_public: None,
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
                is_disabled: false,
                is_public: None,
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
            is_disabled: false,
            is_public: Some(true),
        };

        let json = serde_json::to_value(&atom).unwrap();
        assert_eq!(json.get("is-public"), Some(&serde_json::json!(true)));
        assert!(
            json.get("rust-qualified-name").is_none(),
            "None fields should be omitted"
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
                is_disabled: false,
                is_public: None,
            }
        };

        let json_none = serde_json::to_value(&atom_none).unwrap();
        assert!(
            json_none.get("is-public").is_none(),
            "None is_public should be omitted from JSON"
        );

        let roundtripped: crate::AtomWithLines = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(roundtripped.is_public, Some(true));
    }
}
