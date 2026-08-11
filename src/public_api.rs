//! Public API detection via `cargo-public-api`.
//!
//! Runs `cargo public-api -sss` to list the crate's public API surface,
//! then matches atoms against the output via `rust-qualified-name` (RQN)
//! to override `is-public-api`.
//!
//! When `--with-public-api` is used, every atom with a non-`None` RQN gets a
//! definitive `is-public-api` value (`true` if the RQN appears in the
//! `cargo public-api` output, `false` otherwise). Atoms without an RQN
//! (external stubs) keep `is-public-api: None`.
//!
//! `cargo public-api` names items by their public *re-export* path (following
//! `pub use`, applying `as` renames), whereas atom RQNs use the *definition*
//! path. To reconcile them, crate-root `pub use` declarations are parsed into an
//! `alias -> definition_path` map and each public name is rewritten to its
//! definition form before matching (see `collect_reexport_aliases` /
//! `expand_public_names_with_aliases`).
//!
//! The rewrite is additive: it only *adds* definition-form candidates, and a
//! match still requires a real atom carrying that RQN. Given a correctly
//! resolved crate root (see `resolve_crate_root_file`), the added candidates are
//! genuine public re-exports, so they cannot mislabel a private atom. If the
//! wrong crate root were parsed, that guarantee would not hold — which is why
//! root resolution is package-name checked.

use crate::{AtomWithLines, ProbeError, ProbeResult};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PUBLIC_API_CACHE_FILE: &str = "public-api.txt";
const DATA_DIR: &str = "data";

/// Std blanket impl traits whose `cargo public-api` entries have no
/// corresponding atoms and should be filtered out.
const BLANKET_IMPL_TRAITS: &[&str] = &[
    "Into",
    "TryFrom",
    "TryInto",
    "Borrow",
    "BorrowMut",
    "Any",
    "ToOwned",
    "CloneInto",
    "From",
];

// =============================================================================
// Tool detection and installation
// =============================================================================

fn cargo_public_api_version() -> Option<String> {
    let output = Command::new("cargo")
        .args(["public-api", "--version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout);
        Some(s.trim().to_string())
    } else {
        None
    }
}

/// Ensure `cargo-public-api` is available; install if `auto_install` is set.
pub fn ensure_cargo_public_api(auto_install: bool) -> ProbeResult<()> {
    if cargo_public_api_version().is_some() {
        return Ok(());
    }
    if !auto_install {
        return Err(ProbeError::external_tool(
            "cargo-public-api",
            "Not installed. Install with: cargo install cargo-public-api\n    \
             Or use --auto-install to install automatically.",
        ));
    }

    eprintln!("  Installing cargo-public-api...");
    let status = Command::new("cargo")
        .args(["install", "cargo-public-api"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| {
            ProbeError::external_tool("cargo-public-api", format!("Failed to run cargo: {e}"))
        })?;

    if !status.success() {
        return Err(ProbeError::external_tool(
            "cargo-public-api",
            "cargo install cargo-public-api failed",
        ));
    }
    Ok(())
}

fn has_nightly_toolchain() -> bool {
    let output = Command::new("rustup")
        .args(["toolchain", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().any(|l| l.starts_with("nightly"))
        }
        Err(_) => false,
    }
}

/// Ensure a nightly toolchain is available; install if `auto_install` is set.
pub fn ensure_nightly_toolchain(auto_install: bool) -> ProbeResult<()> {
    if has_nightly_toolchain() {
        return Ok(());
    }
    if !auto_install {
        return Err(ProbeError::external_tool(
            "rustup",
            "Nightly toolchain required by cargo-public-api but not installed.\n    \
             Install with: rustup install nightly --profile minimal\n    \
             Or use --auto-install to install automatically.",
        ));
    }

    eprintln!("  Installing nightly toolchain (required by cargo-public-api)...");
    let status = Command::new("rustup")
        .args(["install", "nightly", "--profile", "minimal"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| ProbeError::external_tool("rustup", format!("Failed to run rustup: {e}")))?;

    if !status.success() {
        return Err(ProbeError::external_tool(
            "rustup",
            "rustup install nightly failed",
        ));
    }
    Ok(())
}

// =============================================================================
// Run cargo-public-api and parse output
// =============================================================================

fn run_cargo_public_api(project_path: &Path, pkg_name: &str) -> ProbeResult<String> {
    let output = Command::new("cargo")
        .args(["public-api", "-sss", "-p", pkg_name])
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            ProbeError::external_tool(
                "cargo-public-api",
                format!("Failed to run cargo public-api: {e}"),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ProbeError::external_tool(
            "cargo-public-api",
            format!("cargo public-api failed:\n{stderr}"),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Extract function qualified names from `cargo public-api` output.
///
/// Parses lines like:
/// - `pub fn my_crate::module::function(args) -> Ret`
/// - `pub unsafe fn my_crate::MyStruct::method(&self)`
/// - `pub const fn my_crate::do_thing()`
///
/// Returns a set of qualified paths (e.g. `my_crate::module::function`),
/// with blanket impl entries filtered out.
pub fn parse_public_api_functions(output: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    for line in output.lines() {
        if let Some(name) = extract_fn_qualified_name(line.trim()) {
            if !is_blanket_impl(&name) {
                result.insert(name);
            }
        }
    }
    result
}

/// Extract the qualified name from a single `cargo public-api` function line.
///
/// Handles: `pub fn`, `pub unsafe fn`, `pub async fn`, `pub const fn`,
/// `pub const unsafe fn`, `pub extern "C" fn`, etc.
fn extract_fn_qualified_name(line: &str) -> Option<String> {
    let fn_idx = line.find(" fn ")?;

    let prefix = line[..fn_idx].trim();
    if !prefix.starts_with("pub") || prefix.get(3..4) == Some("(") {
        return None;
    }

    let after_fn = &line[fn_idx + 4..].trim_start();

    // Strip leading reference prefix from the self-type:
    //   "&'a Type::method(...)" → "Type::method(...)"
    //   "&Type::method(...)"   → "Type::method(...)"
    let after_fn = if let Some(rest) = after_fn.strip_prefix("&'") {
        if let Some(space_pos) = rest.find(' ') {
            rest[space_pos..].trim_start()
        } else {
            rest
        }
    } else if let Some(rest) = after_fn.strip_prefix('&') {
        rest
    } else {
        after_fn
    };

    let name_end = after_fn.find('(').unwrap_or(after_fn.len());
    let qualified = after_fn[..name_end].trim();
    if qualified.is_empty() {
        return None;
    }

    let name = qualified
        .find('<')
        .map_or(qualified, |i| &qualified[..i])
        .trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Check whether a qualified name corresponds to a std blanket impl.
///
/// A blanket impl entry looks like `crate::Type::trait_method` where the
/// method name matches one of the well-known blanket impl traits' methods,
/// or the last `::` segment before the method is one of the trait names.
fn is_blanket_impl(qualified_name: &str) -> bool {
    let segments: Vec<&str> = qualified_name.rsplitn(3, "::").collect();
    if segments.len() >= 2 {
        let method_or_trait = segments[0];
        let parent = segments[1];
        if BLANKET_IMPL_TRAITS.contains(&parent) || BLANKET_IMPL_TRAITS.contains(&method_or_trait) {
            return true;
        }
    }
    false
}

// =============================================================================
// Caching
// =============================================================================

fn cache_path(project_path: &Path) -> std::path::PathBuf {
    project_path.join(DATA_DIR).join(PUBLIC_API_CACHE_FILE)
}

fn read_cached_output(project_path: &Path) -> Option<String> {
    std::fs::read_to_string(cache_path(project_path)).ok()
}

fn write_cache(project_path: &Path, output: &str) -> ProbeResult<()> {
    let path = cache_path(project_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProbeError::file_io(parent, e))?;
    }
    std::fs::write(&path, output).map_err(|e| ProbeError::file_io(&path, e))?;
    Ok(())
}

// =============================================================================
// Collect public API names (with caching)
// =============================================================================

/// Collect the set of public API function qualified names for a project.
///
/// Uses cached output if available and `regenerate` is false.
pub fn collect_public_api(
    project_path: &Path,
    pkg_name: &str,
    regenerate: bool,
) -> ProbeResult<HashSet<String>> {
    let raw = if !regenerate {
        if let Some(cached) = read_cached_output(project_path) {
            println!(
                "  ✓ Found cached public-api output at {}",
                cache_path(project_path).display()
            );
            cached
        } else {
            let output = run_cargo_public_api(project_path, pkg_name)?;
            write_cache(project_path, &output)?;
            output
        }
    } else {
        let output = run_cargo_public_api(project_path, pkg_name)?;
        write_cache(project_path, &output)?;
        output
    };

    Ok(parse_public_api_functions(&raw))
}

// =============================================================================
// RQN normalization
// =============================================================================

/// Normalize a Charon-derived RQN to the flat format used by `cargo-public-api`.
///
/// Charon encodes impl blocks with braces:
///   - Inherent: `mod::{full::path::Type}::method`
///   - Trait:    `mod::{Trait for full::path::Type}::method`
///
/// `cargo-public-api` uses the flat form for both: `full::path::Type::method`.
///
/// Additionally, reference-self types `&'N (Type)` are unwrapped to just `Type`,
/// since `cargo-public-api` strips the `&'a` prefix from reference impls.
///
/// Returns the original string unchanged when no braces are present.
pub fn normalize_rqn_for_public_api(rqn: &str) -> String {
    let Some(open) = rqn.find('{') else {
        return rqn.to_string();
    };
    let Some(close) = rqn.find('}') else {
        return rqn.to_string();
    };

    let inside = &rqn[open + 1..close];
    let suffix = &rqn[close + 1..]; // e.g. "::method"

    // Trait impl: "{TraitPath for TypePath}" → take TypePath
    // Inherent:   "{TypePath}" → take as-is
    let type_path = if let Some(pos) = inside.find(" for ") {
        &inside[pos + 5..]
    } else {
        inside
    };

    let result = format!("{type_path}{suffix}");
    unwrap_ref_type(&result)
}

/// Strip a leading `&'N (...)` wrapper from a normalized RQN.
///
/// Charon represents reference-self impls as `&'1 (crate::Type)::method`.
/// `cargo-public-api` strips the `&` prefix, yielding `crate::Type::method`.
fn unwrap_ref_type(s: &str) -> String {
    if !s.starts_with("&'") {
        return s.to_string();
    }
    // Find the opening paren after the lifetime: "&'N ("
    let Some(paren_open) = s.find('(') else {
        return s.to_string();
    };
    // Find the matching close paren
    let Some(paren_close) = s.rfind(')') else {
        return s.to_string();
    };
    let inner = &s[paren_open + 1..paren_close];
    let after = &s[paren_close + 1..];
    format!("{inner}{after}")
}

// =============================================================================
// RQN-based enrichment
// =============================================================================

/// Set `is-public-api` for all atoms that have a `rust-qualified-name`.
///
/// For each atom:
/// - If `rust-qualified-name` is `None` (external stubs) → leave `is-public-api` unchanged
/// - If RQN (or its normalized form) is in `public_names` → `is-public-api = Some(true)`
/// - Otherwise → `is-public-api = Some(false)`
///
/// Returns `(set_true, set_false)` counts.
pub fn enrich_atoms_with_public_api(
    atoms: &mut BTreeMap<String, AtomWithLines>,
    public_names: &HashSet<String>,
) -> (usize, usize) {
    let mut set_true = 0;
    let mut set_false = 0;

    for atom in atoms.values_mut() {
        let Some(rqn) = &atom.rust_qualified_name else {
            continue;
        };

        let normalized = normalize_rqn_for_public_api(rqn);
        if public_names.contains(rqn) || public_names.contains(&normalized) {
            atom.is_public_api = Some(true);
            set_true += 1;
        } else {
            atom.is_public_api = Some(false);
            set_false += 1;
        }
    }

    (set_true, set_false)
}

// =============================================================================
// Re-export (`pub use`) alias resolution
// =============================================================================

/// Resolve the target crate's root source file (the file that declares the
/// crate-root `pub use` re-exports) for `crate_name`.
///
/// Fast path: when `<project>/Cargo.toml` declares a `[package]` whose name
/// matches `crate_name` (normalizing `-`/`_`), use that manifest's crate root —
/// an explicit `[lib] path` if set, otherwise `src/lib.rs`, otherwise
/// `src/main.rs`. The manifest name check is what keeps this from grabbing an
/// unrelated `src/lib.rs` (e.g. when `project_path` is a workspace root).
///
/// Fallback: when there is no matching root manifest (workspace root, or a
/// manifest for a different package), query `cargo metadata --no-deps` and use
/// the matching package's lib-target `src_path`.
///
/// Returns `None` when the root cannot be determined; callers treat that as
/// "no aliases", preserving prior behavior.
fn resolve_crate_root_file(project_path: &Path, crate_name: &str) -> Option<PathBuf> {
    if let Some(root) = crate_root_from_manifest(project_path, crate_name) {
        return Some(root);
    }

    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&output.stdout);
    lib_src_path_from_metadata(&json, crate_name)
}

/// Resolve the crate root from `<project>/Cargo.toml`, but only when it declares
/// a `[package]` whose name matches `crate_name` (normalizing `-`/`_`).
///
/// Honors an explicit `[lib] path`, then `src/lib.rs`, then `src/main.rs`.
/// Returns `None` when the manifest is absent/unparseable, declares no matching
/// `[package]`, or none of the candidate files exist — so a workspace root (or a
/// manifest for a different crate) falls through to the `cargo metadata` path.
fn crate_root_from_manifest(project_path: &Path, crate_name: &str) -> Option<PathBuf> {
    let normalize = |s: &str| s.replace('-', "_");
    let manifest = std::fs::read_to_string(project_path.join("Cargo.toml")).ok()?;
    let table: toml::Table = manifest.parse().ok()?;

    let pkg_name = table.get("package")?.as_table()?.get("name")?.as_str()?;
    if normalize(pkg_name) != normalize(crate_name) {
        return None;
    }

    // An explicit `[lib] path` wins when present and existing.
    if let Some(lib_path) = table
        .get("lib")
        .and_then(|l| l.as_table())
        .and_then(|l| l.get("path"))
        .and_then(|p| p.as_str())
    {
        let candidate = project_path.join(lib_path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    for rel in ["src/lib.rs", "src/main.rs"] {
        let candidate = project_path.join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Extract the lib-target `src_path` for `crate_name` from `cargo metadata`
/// JSON. Package-name matching normalizes `-`/`_`. Returns `None` when the
/// package has no lib-like target (e.g. a bin-only crate, which has no public
/// API surface to reconcile), rather than guessing a non-lib target.
fn lib_src_path_from_metadata(json: &str, crate_name: &str) -> Option<PathBuf> {
    const LIB_KINDS: &[&str] = &["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

    let normalize = |s: &str| s.replace('-', "_");
    let wanted = normalize(crate_name);

    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let packages = root.get("packages")?.as_array()?;
    let pkg = packages.iter().find(|p| {
        p.get("name")
            .and_then(|n| n.as_str())
            .map(normalize)
            .as_deref()
            == Some(wanted.as_str())
    })?;
    let targets = pkg.get("targets")?.as_array()?;

    let is_lib = |t: &serde_json::Value| {
        t.get("kind")
            .and_then(|k| k.as_array())
            .is_some_and(|kinds| {
                kinds
                    .iter()
                    .any(|k| k.as_str().is_some_and(|s| LIB_KINDS.contains(&s)))
            })
    };

    let target = targets.iter().find(|t| is_lib(t))?;
    let src = target.get("src_path")?.as_str()?;
    Some(PathBuf::from(src))
}

/// Build a map from a crate-root re-export alias to the item's definition path.
///
/// `cargo public-api` reports items by their *public* path, following `pub use`
/// re-exports (and applying `as` renames). probe-rust atoms carry the item's
/// *definition* path (the module where the `fn`/type is written). For an item
/// defined in a submodule but re-exported at the crate root, these differ, so a
/// naive name join misses it (a false `is-public-api: false`).
///
/// This parses the crate root for top-level `pub use` declarations and returns
/// `alias -> definition_path`, e.g. `SerializationError -> spqr::serialize::Error`
/// for `pub use crate::serialize::Error as SerializationError;`.
///
/// Only crate-root re-exports are resolved (the common case that produces the
/// short `crate::Type` paths `cargo public-api` prints); nested re-export chains
/// are not followed, and glob re-exports (`pub use foo::*`) are skipped.
/// `#[cfg(...)]`-gated `pub use` declarations are also skipped, since the active
/// feature set is not known here and a wrong alias could mislabel an atom.
/// Only `crate::`/`self::`-prefixed re-exports (Rust 2018+) are resolved; bare
/// crate-relative paths (Rust 2015) are treated as external and left alone. All
/// I/O and parse errors yield an empty map (graceful no-op).
pub fn collect_reexport_aliases(project_path: &Path, crate_name: &str) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    let crate_name = crate_name.replace('-', "_");

    let Some(root) = resolve_crate_root_file(project_path, &crate_name) else {
        return aliases;
    };
    let Ok(src) = std::fs::read_to_string(&root) else {
        return aliases;
    };
    let Ok(file) = syn::parse_file(&src) else {
        return aliases;
    };

    for item in &file.items {
        let syn::Item::Use(item_use) = item else {
            continue;
        };
        if !matches!(item_use.vis, syn::Visibility::Public(_)) {
            continue;
        }
        if has_cfg_attr(&item_use.attrs) {
            continue;
        }
        let mut leaves = Vec::new();
        collect_use_tree(&item_use.tree, Vec::new(), &mut leaves);
        for (alias, segments) in leaves {
            if let Some(def) = use_path_to_def(&segments, &crate_name) {
                aliases.insert(alias, def);
            }
        }
    }

    aliases
}

/// Whether any attribute is a `#[cfg(...)]` conditional-compilation gate.
fn has_cfg_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("cfg"))
}

/// Walk a `use` tree, collecting `(alias, full_path_segments)` for each leaf.
fn collect_use_tree(
    tree: &syn::UseTree,
    prefix: Vec<String>,
    out: &mut Vec<(String, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(p) => {
            let mut pre = prefix;
            pre.push(p.ident.to_string());
            collect_use_tree(&p.tree, pre, out);
        }
        syn::UseTree::Name(n) => {
            let mut full = prefix;
            full.push(n.ident.to_string());
            out.push((n.ident.to_string(), full));
        }
        syn::UseTree::Rename(r) => {
            let mut full = prefix;
            full.push(r.ident.to_string());
            out.push((r.rename.to_string(), full));
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                collect_use_tree(t, prefix.clone(), out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

/// Convert `use` path segments to a definition path, substituting the crate
/// name for a leading `crate`/`self`. Returns `None` for `super`-relative paths
/// (not resolvable from the crate root) or empty paths.
fn use_path_to_def(segments: &[String], crate_name: &str) -> Option<String> {
    let mut segments = segments.to_vec();
    match segments.first().map(String::as_str) {
        Some("crate") | Some("self") => segments[0] = crate_name.to_string(),
        Some("super") => return None,
        _ => {}
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("::"))
}

/// Expand a set of public-API names with their re-export definition forms.
///
/// For each public name whose first path segment after the crate is a known
/// re-export alias, add the rewritten definition-path form so it can join with
/// an atom's definition-derived `rust-qualified-name`. Original names are kept.
pub fn expand_public_names_with_aliases(
    public_names: &HashSet<String>,
    crate_name: &str,
    aliases: &BTreeMap<String, String>,
) -> HashSet<String> {
    let mut expanded = public_names.clone();
    if aliases.is_empty() {
        return expanded;
    }
    let crate_name = crate_name.replace('-', "_");
    for name in public_names {
        if let Some(def) = rewrite_public_name(name, &crate_name, aliases) {
            expanded.insert(def);
        }
    }
    expanded
}

/// Rewrite a single public name to its definition form via the alias map.
///
/// `spqr::ChainParams::default` + `{ChainParams -> spqr::chain::ChainParams}`
/// yields `spqr::chain::ChainParams::default`. Returns `None` when the first
/// segment after the crate is not a known alias.
fn rewrite_public_name(
    name: &str,
    crate_name: &str,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    let prefix = format!("{crate_name}::");
    let rest = name.strip_prefix(&prefix)?;
    let (first, tail) = match rest.split_once("::") {
        Some((f, t)) => (f, Some(t)),
        None => (rest, None),
    };
    let def = aliases.get(first)?;
    match tail {
        Some(t) => Some(format!("{def}::{t}")),
        None => Some(def.clone()),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_public_api_functions ----

    #[test]
    fn test_parse_simple_functions() {
        let output = "\
pub fn my_crate::simple_func()
pub fn my_crate::module::helper(x: u32) -> bool
pub fn my_crate::MyStruct::method(&self)
";
        let names = parse_public_api_functions(output);
        assert!(names.contains("my_crate::simple_func"));
        assert!(names.contains("my_crate::module::helper"));
        assert!(names.contains("my_crate::MyStruct::method"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn test_parse_qualified_functions() {
        let output = "\
pub unsafe fn my_crate::unsafe_thing() -> *const u8
pub async fn my_crate::async_thing()
pub const fn my_crate::const_thing() -> usize
pub extern \"C\" fn my_crate::ffi_func()
";
        let names = parse_public_api_functions(output);
        assert!(names.contains("my_crate::unsafe_thing"));
        assert!(names.contains("my_crate::async_thing"));
        assert!(names.contains("my_crate::const_thing"));
        assert!(names.contains("my_crate::ffi_func"));
    }

    #[test]
    fn test_parse_ignores_non_function_items() {
        let output = "\
pub struct my_crate::MyStruct
pub enum my_crate::MyEnum
pub trait my_crate::MyTrait
pub type my_crate::MyType = u32
pub const my_crate::MY_CONST: u32
pub fn my_crate::actual_function()
";
        let names = parse_public_api_functions(output);
        assert_eq!(names.len(), 1);
        assert!(names.contains("my_crate::actual_function"));
    }

    #[test]
    fn test_parse_ignores_restricted_visibility() {
        let output = "\
pub(crate) fn my_crate::internal()
pub fn my_crate::external()
";
        let names = parse_public_api_functions(output);
        assert_eq!(names.len(), 1);
        assert!(names.contains("my_crate::external"));
    }

    #[test]
    fn test_parse_generics_stripped() {
        let output = "pub fn my_crate::Vec::push<T>(value: T)\n";
        let names = parse_public_api_functions(output);
        assert!(names.contains("my_crate::Vec::push"));
    }

    #[test]
    fn test_blanket_impls_filtered() {
        let output = "\
pub fn my_crate::MyType::Into::into(self) -> T
pub fn my_crate::MyType::From::from(val: MyType) -> Self
pub fn my_crate::MyType::TryFrom::try_from(val: MyType) -> Result<Self, Self::Error>
pub fn my_crate::MyType::TryInto::try_into(self) -> Result<T, T::Error>
pub fn my_crate::MyType::Borrow::borrow(&self) -> &T
pub fn my_crate::MyType::BorrowMut::borrow_mut(&mut self) -> &mut T
pub fn my_crate::MyType::Any::type_id(&self) -> TypeId
pub fn my_crate::MyType::ToOwned::to_owned(&self) -> Self
pub fn my_crate::MyType::CloneInto::clone_into(&self, target: &mut Self)
pub fn my_crate::real_function()
";
        let names = parse_public_api_functions(output);
        assert_eq!(names.len(), 1);
        assert!(names.contains("my_crate::real_function"));
    }

    // ---- extract_fn_qualified_name ----

    #[test]
    fn test_extract_basic() {
        assert_eq!(
            extract_fn_qualified_name("pub fn my_crate::foo()"),
            Some("my_crate::foo".to_string())
        );
    }

    #[test]
    fn test_extract_with_args_and_return() {
        assert_eq!(
            extract_fn_qualified_name("pub fn my_crate::bar(x: u32, y: &str) -> bool"),
            Some("my_crate::bar".to_string())
        );
    }

    #[test]
    fn test_extract_unsafe() {
        assert_eq!(
            extract_fn_qualified_name("pub unsafe fn my_crate::danger()"),
            Some("my_crate::danger".to_string())
        );
    }

    #[test]
    fn test_extract_not_a_function() {
        assert_eq!(extract_fn_qualified_name("pub struct my_crate::Foo"), None);
    }

    #[test]
    fn test_extract_restricted_vis() {
        assert_eq!(
            extract_fn_qualified_name("pub(crate) fn my_crate::internal()"),
            None
        );
    }

    #[test]
    fn test_extract_lifetime_prefix() {
        assert_eq!(
            extract_fn_qualified_name(
                "pub fn &'a curve25519_dalek::edwards::EdwardsPoint::add(self, other: &'b curve25519_dalek::edwards::EdwardsPoint) -> curve25519_dalek::edwards::EdwardsPoint"
            ),
            Some("curve25519_dalek::edwards::EdwardsPoint::add".to_string())
        );
    }

    #[test]
    fn test_extract_bare_ref_prefix() {
        assert_eq!(
            extract_fn_qualified_name(
                "pub fn &curve25519_dalek::scalar::Scalar::mul(self, point: &curve25519_dalek::montgomery::MontgomeryPoint) -> curve25519_dalek::montgomery::MontgomeryPoint"
            ),
            Some("curve25519_dalek::scalar::Scalar::mul".to_string())
        );
    }

    // ---- is_blanket_impl ----

    #[test]
    fn test_blanket_impl_detection() {
        assert!(is_blanket_impl("my_crate::MyType::Into::into"));
        assert!(is_blanket_impl("my_crate::MyType::From::from"));
        assert!(is_blanket_impl("my_crate::MyType::TryFrom::try_from"));
        assert!(is_blanket_impl("my_crate::MyType::Any::type_id"));
        assert!(!is_blanket_impl("my_crate::MyType::method"));
        assert!(!is_blanket_impl("my_crate::MyType::Display::fmt"));
    }

    // ---- normalize_rqn_for_public_api ----

    #[test]
    fn test_normalize_no_braces() {
        assert_eq!(
            normalize_rqn_for_public_api("c::m::free_fn"),
            "c::m::free_fn"
        );
    }

    #[test]
    fn test_normalize_inherent_impl() {
        assert_eq!(
            normalize_rqn_for_public_api("c::edwards::{c::edwards::EdwardsPoint}::compress"),
            "c::edwards::EdwardsPoint::compress"
        );
    }

    #[test]
    fn test_normalize_trait_impl() {
        assert_eq!(
            normalize_rqn_for_public_api(
                "c::edwards::{subtle::ConstantTimeEq for c::edwards::EdwardsPoint}::ct_eq"
            ),
            "c::edwards::EdwardsPoint::ct_eq"
        );
    }

    #[test]
    fn test_normalize_generic_trait_impl() {
        // Reference-self impl: unwraps &'1 (...) to just the inner type
        assert_eq!(
            normalize_rqn_for_public_api(
                "c::edwards::{core::ops::arith::Add<&'0 (c::edwards::EdwardsPoint), c::edwards::EdwardsPoint> for &'1 (c::edwards::EdwardsPoint)}::add"
            ),
            "c::edwards::EdwardsPoint::add"
        );
    }

    #[test]
    fn test_normalize_ref_neg() {
        assert_eq!(
            normalize_rqn_for_public_api(
                "c::edwards::{core::ops::arith::Neg<c::edwards::EdwardsPoint> for &'0 (c::edwards::EdwardsPoint)}::neg"
            ),
            "c::edwards::EdwardsPoint::neg"
        );
    }

    // ---- enrich_atoms_with_public_api ----

    #[test]
    fn test_enrich_direct_match() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:c/1.0/m/public_fn()".to_string(),
            make_atom("public_fn", "src/m.rs", Some("c::m::public_fn")),
        );
        atoms.insert(
            "probe:c/1.0/m/private_fn()".to_string(),
            make_atom("private_fn", "src/m.rs", Some("c::m::private_fn")),
        );
        atoms.insert(
            "probe:ext/1.0/lib/ext_fn()".to_string(),
            make_atom("ext_fn", "", None),
        );

        let public_names: HashSet<String> =
            ["c::m::public_fn"].iter().map(|s| s.to_string()).collect();

        let (set_true, set_false) = enrich_atoms_with_public_api(&mut atoms, &public_names);

        assert_eq!(set_true, 1);
        assert_eq!(set_false, 1);
        assert_eq!(atoms["probe:c/1.0/m/public_fn()"].is_public_api, Some(true));
        assert_eq!(
            atoms["probe:c/1.0/m/private_fn()"].is_public_api,
            Some(false)
        );
        assert_eq!(atoms["probe:ext/1.0/lib/ext_fn()"].is_public_api, None);
    }

    #[test]
    fn test_enrich_normalized_match() {
        let mut atoms = BTreeMap::new();

        // Charon RQN with braces
        atoms.insert(
            "probe:c/1.0/edwards/compress()".to_string(),
            make_atom(
                "compress",
                "src/edwards.rs",
                Some("c::edwards::{c::edwards::EdwardsPoint}::compress"),
            ),
        );

        // Trait impl Charon RQN
        atoms.insert(
            "probe:c/1.0/edwards/ct_eq()".to_string(),
            make_atom(
                "ct_eq",
                "src/edwards.rs",
                Some("c::edwards::{subtle::ConstantTimeEq for c::edwards::EdwardsPoint}::ct_eq"),
            ),
        );

        // cargo-public-api uses flat format
        let public_names: HashSet<String> = [
            "c::edwards::EdwardsPoint::compress",
            "c::edwards::EdwardsPoint::ct_eq",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let (set_true, set_false) = enrich_atoms_with_public_api(&mut atoms, &public_names);

        assert_eq!(set_true, 2);
        assert_eq!(set_false, 0);
        assert_eq!(
            atoms["probe:c/1.0/edwards/compress()"].is_public_api,
            Some(true)
        );
        assert_eq!(
            atoms["probe:c/1.0/edwards/ct_eq()"].is_public_api,
            Some(true)
        );
    }

    // ---- re-export alias resolution ----

    /// Write a minimal single-crate project (`Cargo.toml` + `src/<file>`) into
    /// `dir` so `resolve_crate_root_file` takes the manifest fast path.
    fn write_crate(dir: &Path, pkg_name: &str, lib_rel: &str, lib_src: &str) {
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{pkg_name}\"\nversion = \"0.1.0\"\n"),
        )
        .unwrap();
        let lib_path = dir.join(lib_rel);
        std::fs::create_dir_all(lib_path.parent().unwrap()).unwrap();
        std::fs::write(lib_path, lib_src).unwrap();
    }

    #[test]
    fn test_collect_reexport_aliases_from_source() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(
            dir.path(),
            "spqr",
            "src/lib.rs",
            "\
pub use crate::chain::ChainParams;
pub use crate::proto::pq_ratchet::{Direction, Version};
pub use crate::serialize::Error as SerializationError;
pub use self::io::Reader;
use crate::internal::Hidden;
pub use crate::glob_mod::*;
#[cfg(feature = \"extra\")]
pub use crate::extra::Gated;
pub fn top() {}
",
        );

        let aliases = collect_reexport_aliases(dir.path(), "spqr");

        assert_eq!(
            aliases.get("ChainParams").map(String::as_str),
            Some("spqr::chain::ChainParams")
        );
        assert_eq!(
            aliases.get("Direction").map(String::as_str),
            Some("spqr::proto::pq_ratchet::Direction")
        );
        assert_eq!(
            aliases.get("Version").map(String::as_str),
            Some("spqr::proto::pq_ratchet::Version")
        );
        // Rename: alias is the `as` name, path is the definition.
        assert_eq!(
            aliases.get("SerializationError").map(String::as_str),
            Some("spqr::serialize::Error")
        );
        // `self::` at the crate root resolves to the crate name.
        assert_eq!(
            aliases.get("Reader").map(String::as_str),
            Some("spqr::io::Reader")
        );
        // Non-`pub` use is ignored; glob re-export is skipped; cfg-gated skipped.
        assert!(!aliases.contains_key("Hidden"));
        assert!(!aliases.contains_key("Gated"));
        assert_eq!(aliases.len(), 5);
    }

    #[test]
    fn test_collect_reexport_aliases_uses_main_rs() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(
            dir.path(),
            "bin_crate",
            "src/main.rs",
            "pub use crate::app::App;\nfn main() {}\n",
        );

        let aliases = collect_reexport_aliases(dir.path(), "bin_crate");

        assert_eq!(
            aliases.get("App").map(String::as_str),
            Some("bin_crate::app::App")
        );
    }

    #[test]
    fn test_collect_reexport_aliases_honors_custom_lib_path() {
        let dir = tempfile::tempdir().unwrap();
        // Manifest points the lib at a non-default path; a decoy `src/lib.rs`
        // must NOT be picked.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"custom\"\nversion = \"0.1.0\"\n[lib]\npath = \"src/root.rs\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/root.rs"),
            "pub use crate::real::Thing;\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub use crate::decoy::Wrong;\n",
        )
        .unwrap();

        let aliases = collect_reexport_aliases(dir.path(), "custom");

        assert_eq!(
            aliases.get("Thing").map(String::as_str),
            Some("custom::real::Thing")
        );
        assert!(!aliases.contains_key("Wrong"));
    }

    #[test]
    fn test_collect_reexport_aliases_name_mismatch_skips_fast_path() {
        // A decoy `src/lib.rs` exists, but the manifest is for a different
        // package, so the fast path must not parse it. With no valid workspace
        // metadata in the tempdir, the result is an empty map (not the decoy).
        let dir = tempfile::tempdir().unwrap();
        write_crate(
            dir.path(),
            "other_crate",
            "src/lib.rs",
            "pub use crate::decoy::Wrong;\n",
        );

        let aliases = collect_reexport_aliases(dir.path(), "spqr");

        assert!(aliases.is_empty());
    }

    #[test]
    fn test_collect_reexport_aliases_missing_root_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let aliases = collect_reexport_aliases(dir.path(), "spqr");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_expand_module_lift() {
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "ChainParams".to_string(),
            "spqr::chain::ChainParams".to_string(),
        );
        let public: HashSet<String> = ["spqr::ChainParams::default"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        assert!(expanded.contains("spqr::chain::ChainParams::default"));
        assert!(expanded.contains("spqr::ChainParams::default"));
    }

    #[test]
    fn test_expand_rename() {
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "SerializationError".to_string(),
            "spqr::serialize::Error".to_string(),
        );
        let public: HashSet<String> = ["spqr::SerializationError::from"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        assert!(expanded.contains("spqr::serialize::Error::from"));
    }

    #[test]
    fn test_expand_empty_aliases_is_noop() {
        let aliases = BTreeMap::new();
        let public: HashSet<String> = ["spqr::send", "spqr::recv"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        assert_eq!(expanded, public);
    }

    #[test]
    fn test_expand_non_alias_untouched() {
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "ChainParams".to_string(),
            "spqr::chain::ChainParams".to_string(),
        );
        // `send` is not an alias, so no rewritten form is added.
        let public: HashSet<String> = ["spqr::send"].iter().map(|s| s.to_string()).collect();

        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        assert_eq!(expanded.len(), 1);
        assert!(expanded.contains("spqr::send"));
    }

    #[test]
    fn test_lib_src_path_from_metadata() {
        let json = r#"{
            "packages": [
                {
                    "name": "other",
                    "targets": [{ "kind": ["lib"], "src_path": "/other/src/lib.rs" }]
                },
                {
                    "name": "my-crate",
                    "targets": [
                        { "kind": ["custom-build"], "src_path": "/my/build.rs" },
                        { "kind": ["lib"], "src_path": "/my/src/lib.rs" }
                    ]
                }
            ]
        }"#;

        // Name normalization: query with underscores, manifest uses hyphens.
        assert_eq!(
            lib_src_path_from_metadata(json, "my_crate"),
            Some(PathBuf::from("/my/src/lib.rs"))
        );
        assert_eq!(lib_src_path_from_metadata(json, "absent"), None);
    }

    #[test]
    fn test_lib_src_path_from_metadata_no_lib_is_none() {
        // A bin-only package (custom-build + bin, no lib target) must resolve to
        // None rather than guessing build.rs or the bin.
        let json = r#"{
            "packages": [
                {
                    "name": "bin-only",
                    "targets": [
                        { "kind": ["custom-build"], "src_path": "/b/build.rs" },
                        { "kind": ["bin"], "src_path": "/b/src/main.rs" }
                    ]
                }
            ]
        }"#;
        assert_eq!(lib_src_path_from_metadata(json, "bin_only"), None);
    }

    // ---- use_path_to_def ----

    #[test]
    fn test_use_path_to_def_crate_and_self() {
        assert_eq!(
            use_path_to_def(&["crate".into(), "m".into(), "T".into()], "spqr"),
            Some("spqr::m::T".to_string())
        );
        assert_eq!(
            use_path_to_def(&["self".into(), "T".into()], "spqr"),
            Some("spqr::T".to_string())
        );
    }

    #[test]
    fn test_use_path_to_def_super_is_none() {
        assert_eq!(use_path_to_def(&["super".into(), "T".into()], "spqr"), None);
    }

    #[test]
    fn test_use_path_to_def_external_left_alone() {
        // Non-crate/self leading segment (e.g. an external crate) is not
        // rewritten with the local crate name.
        assert_eq!(
            use_path_to_def(&["other".into(), "T".into()], "spqr"),
            Some("other::T".to_string())
        );
    }

    #[test]
    fn test_collect_reexport_aliases_nested_group() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(
            dir.path(),
            "spqr",
            "src/lib.rs",
            "pub use crate::a::{B, c::{D, E as F}};\n",
        );

        let aliases = collect_reexport_aliases(dir.path(), "spqr");

        assert_eq!(aliases.get("B").map(String::as_str), Some("spqr::a::B"));
        assert_eq!(aliases.get("D").map(String::as_str), Some("spqr::a::c::D"));
        // Rename inside a nested group.
        assert_eq!(aliases.get("F").map(String::as_str), Some("spqr::a::c::E"));
    }

    #[test]
    fn test_alias_rewrite_with_matching_atom_marks_public() {
        // End-to-end: a public re-export path rewritten to its definition form
        // matches a real atom and marks it public.
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "ChainParams".to_string(),
            "spqr::chain::ChainParams".to_string(),
        );
        let public: HashSet<String> = ["spqr::ChainParams::default"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:spqr/1.0/chain/default()".to_string(),
            make_atom(
                "default",
                "src/chain.rs",
                Some("spqr::chain::ChainParams::default"),
            ),
        );

        let (set_true, set_false) = enrich_atoms_with_public_api(&mut atoms, &expanded);

        assert_eq!(set_true, 1);
        assert_eq!(set_false, 0);
        assert_eq!(
            atoms["probe:spqr/1.0/chain/default()"].is_public_api,
            Some(true)
        );
    }

    #[test]
    fn test_reexport_rewrite_without_atom_stays_unmatched() {
        // A generated-style public name whose rewritten definition form has no
        // atom must not be marked public (guards against over-matching).
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "Direction".to_string(),
            "spqr::proto::pq_ratchet::Direction".to_string(),
        );
        let public: HashSet<String> = ["spqr::Direction::from_i32"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let expanded = expand_public_names_with_aliases(&public, "spqr", &aliases);

        // Only a hand-written atom in a different module exists; no proto atom.
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:spqr/1.0/lib/switch()".to_string(),
            make_atom("switch", "src/lib.rs", Some("spqr::Direction::switch")),
        );

        let (set_true, set_false) = enrich_atoms_with_public_api(&mut atoms, &expanded);

        assert_eq!(set_true, 0);
        assert_eq!(set_false, 1);
        assert_eq!(
            atoms["probe:spqr/1.0/lib/switch()"].is_public_api,
            Some(false)
        );
    }

    fn make_atom(display_name: &str, code_path: &str, rqn: Option<&str>) -> AtomWithLines {
        AtomWithLines {
            display_name: display_name.to_string(),
            code_name: String::new(),
            dependencies: std::collections::BTreeSet::new(),
            dependencies_with_locations: Vec::new(),
            code_module: String::new(),
            code_path: code_path.to_string(),
            code_text: crate::CodeTextInfo {
                lines_start: 0,
                lines_end: 0,
            },
            kind: crate::DeclKind::Exec,
            language: "rust".to_string(),
            rust_qualified_name: rqn.map(|s| s.to_string()),
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
}
