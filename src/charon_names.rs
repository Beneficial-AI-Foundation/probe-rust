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
}

/// Parse an LLBC JSON file and return Charon function info grouped by match key.
pub fn parse_llbc_names(llbc_path: &Path) -> Result<HashMap<String, Vec<CharonFunInfo>>, String> {
    let file =
        std::fs::File::open(llbc_path).map_err(|e| format!("failed to read LLBC file: {e}"))?;
    let reader = std::io::BufReader::new(file);
    let root: serde_json::Value =
        serde_json::from_reader(reader).map_err(|e| format!("failed to parse LLBC JSON: {e}"))?;

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

    let mut result: HashMap<String, Vec<CharonFunInfo>> = HashMap::new();

    for entry in item_names {
        let key = match entry.get("key") {
            Some(k) => k,
            None => continue,
        };

        if key.get("Fun").is_none() {
            continue;
        }

        let path_elems = match entry.get("value").and_then(|v| v.as_array()) {
            Some(v) => v,
            None => continue,
        };

        let qualified_name = format_name(
            path_elems,
            &trait_decl_names,
            &trait_impl_to_decl,
            &type_decl_names,
        );

        let match_key = make_match_key_from_charon(&qualified_name, crate_name);

        result
            .entry(match_key.clone())
            .or_default()
            .push(CharonFunInfo {
                qualified_name,
                match_key,
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
                parts.push(format!("{{impl {trait_name}}}"));
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
    let untagged = skip_binder.get("Untagged")?;
    let adt = untagged.get("Adt")?;
    let adt_id = adt.get("id")?.get("Adt")?.as_u64()?;
    type_decl_names.get(&adt_id).cloned()
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

use crate::bare_function_name;

// ---------------------------------------------------------------------------
// Lookup table builders
// ---------------------------------------------------------------------------

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
// Enrichment: cross-reference atoms with Charon names
// ---------------------------------------------------------------------------

/// Enrich atoms by matching their `code_path` + `display_name` against
/// Charon LLBC names. Returns the number of atoms enriched.
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

        if let Some(candidates) = charon_map.get(&match_key) {
            if candidates.len() == 1 {
                atom.rust_qualified_name = Some(candidates[0].qualified_name.clone());
                enriched += 1;
            } else {
                // Multiple Charon functions with the same module::fn_name.
                // Use the heuristic RQN to pick the best one.
                let heuristic = atom.rust_qualified_name.as_deref().unwrap_or("");
                if let Some(best) = candidates.iter().find(|c| {
                    let simplified = strip_impl_blocks(&c.qualified_name);
                    simplified == heuristic
                }) {
                    atom.rust_qualified_name = Some(best.qualified_name.clone());
                    enriched += 1;
                } else if let Some(first) = candidates.first() {
                    atom.rust_qualified_name = Some(first.qualified_name.clone());
                    enriched += 1;
                }
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
        let name = format_name(&elems, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert_eq!(name, "my_crate::module::func");
    }

    #[test]
    fn test_format_name_with_trait_impl() {
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
        );
        assert_eq!(name, "crate::{impl core::clone::Clone}::method");
    }
}
