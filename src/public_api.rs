//! Public API detection via `cargo-public-api`.
//!
//! Runs `cargo public-api -sss` to list the crate's public API surface,
//! then matches atoms against the output via `rust-qualified-name` (RQN)
//! to override `is-public-api`.
//!
//! When `--with-public-api` is used, every atom with a non-`None` RQN gets a
//! definitive `is-public-api` value (`true` if the RQN appears in the
//! `cargo public-api` output, `false` otherwise). Atoms without an RQN keep
//! `is-public-api: None`, with one exception: an analyzed-crate atom that
//! impl-descriptor resolution proves public (see
//! [`PublicNameForms::resolve_unmatched_to_atoms`]) is set `true` even
//! without an RQN — macro-generated impls leave exactly such atoms.
//!
//! `cargo public-api` names items by their public *re-export* path (following
//! `pub use`, applying `as` renames), whereas atom RQNs use the *definition*
//! path. To reconcile them, crate-root `pub use` declarations are parsed into an
//! `alias -> definition_path` map and each public name is rewritten to its
//! definition form before matching (see `collect_reexport_aliases` /
//! [`PublicNameForms::expand_with_aliases`]). The rewrite is additive: it only
//! *adds* definition-form candidates, and a match still requires a real atom
//! carrying that RQN. Given a correctly resolved crate root (see
//! `resolve_crate_root_file`), the added candidates are genuine public
//! re-exports, so they cannot mislabel a private atom.
//!
//! Matching runs in additive passes over per-entry candidate forms
//! (`PublicNameForms`): the `pub use` rewrite, inherited-default-trait-method
//! resolution, RQN matching, then `resolve_unmatched_to_atoms` for entries no
//! atom name matched (macro-generated and blanket impls), which marks the
//! implementing atom directly. Every pass is additive — an entry can gain
//! candidate forms but never lose one — no pass resolves an ambiguity, and
//! the resolution passes act only on still-unmatched entries. The canonical
//! specification of the pass
//! sequence and its guards is KB property P11
//! (`kb/engineering/properties.md`); `PublicNameForms` and
//! `resolve_unmatched_to_atoms` document the pass-local details.

use crate::{AtomWithLines, ProbeError, ProbeResult};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use syn::spanned::Spanned;

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
/// - If `rust-qualified-name` is `None` → leave `is-public-api` unchanged
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
    let mut conflicting: BTreeSet<String> = BTreeSet::new();
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
                // Ambiguity skips: an alias claimed for two different
                // definitions resolves to neither.
                match aliases.get(&alias) {
                    Some(prev) if *prev != def => {
                        conflicting.insert(alias);
                    }
                    _ => {
                        aliases.insert(alias, def);
                    }
                }
            }
        }
    }

    for alias in conflicting {
        aliases.remove(&alias);
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
// Public-name candidate forms
// =============================================================================

/// Candidate match forms for each `cargo public-api` entry.
///
/// Key = the entry exactly as `cargo public-api` reported it; value = every name
/// that entry may be matched under (always including the entry itself). The
/// expansion passes are strictly additive, so an entry can gain candidates but
/// never lose its original name.
///
/// Keeping the per-entry grouping (rather than one flat set) is what makes
/// *entry-level* reporting possible: an entry is "matched" when at least one of
/// its forms names a real atom. That is a different metric from the count of
/// atoms marked `is-public-api: true` (several entries can share one atom, e.g.
/// macro-generated types, and one atom can be reached by several forms).
#[derive(Debug, Default)]
pub struct PublicNameForms {
    forms: BTreeMap<String, BTreeSet<String>>,
    /// Entries resolved straight to an implementing atom (see
    /// [`PublicNameForms::resolve_unmatched_to_atoms`]) rather than by name.
    resolved: BTreeSet<String>,
}

impl PublicNameForms {
    /// Start with each entry mapping to just itself.
    pub fn new(entries: &HashSet<String>) -> Self {
        Self {
            forms: entries
                .iter()
                .map(|e| (e.clone(), BTreeSet::from([e.clone()])))
                .collect(),
            resolved: BTreeSet::new(),
        }
    }

    /// Number of `cargo public-api` entries tracked.
    pub fn len(&self) -> usize {
        self.forms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forms.is_empty()
    }

    /// All candidate forms of all entries, flattened for the matcher.
    pub fn flat(&self) -> HashSet<String> {
        self.forms.values().flatten().cloned().collect()
    }

    /// Number of entries backed by a real atom: at least one form naming one
    /// (`atom_names` from [`atom_candidate_names`]), or a direct resolution to
    /// an implementing atom.
    pub fn matched_count(&self, atom_names: &HashSet<String>) -> usize {
        self.forms
            .iter()
            .filter(|(entry, forms)| {
                forms.iter().any(|f| atom_names.contains(f)) || self.resolved.contains(*entry)
            })
            .count()
    }

    /// Add re-export definition forms (see [`collect_reexport_aliases`]).
    pub fn expand_with_aliases(&mut self, crate_name: &str, aliases: &BTreeMap<String, String>) {
        if aliases.is_empty() {
            return;
        }
        let crate_name = crate_name.replace('-', "_");
        let additions: Vec<(String, String)> = self
            .forms
            .keys()
            .filter_map(|entry| {
                rewrite_public_name(entry, &crate_name, aliases).map(|def| (entry.clone(), def))
            })
            .collect();
        for (entry, def) in additions {
            self.add(&entry, def);
        }
    }

    /// Add inherited-default-trait-method forms.
    ///
    /// A `cargo public-api` entry `path::Type::method` where `Type` never
    /// defines `method` itself is an *inherited* default: the body a public
    /// caller reaches lives in the trait, and that trait's atom is the only
    /// atom there is. This pass adds that trait atom's RQN as a candidate form
    /// for the entry, under uniqueness guards (see the module docs).
    pub fn expand_with_trait_defaults(
        &mut self,
        atoms: &BTreeMap<String, AtomWithLines>,
        atom_names: &HashSet<String>,
        crate_name: &str,
    ) {
        // Impl-chain truth straight from the atom keys, whose SCIP descriptors
        // embed `impl#[SelfType][Trait]method()`. Stubs count: an impl-evidence
        // key with no body still proves `Type: Trait`, and a key for
        // `Type::method` still proves an override. Only the analyzed crate's
        // keys are evidence: a dependency's impls neither prove nor veto.
        let crate_name = crate_name.replace('-', "_");
        let mut impl_traits: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut overridden: HashSet<(String, String)> = HashSet::new();
        for key in atoms.keys() {
            if key_crate(key).map(|c| c.replace('-', "_")).as_deref() != Some(&crate_name) {
                continue;
            }
            let Some((self_type, trait_name, method)) = parse_impl_key(key) else {
                continue;
            };
            overridden.insert((self_type.clone(), method));
            if let Some(trait_name) = trait_name {
                impl_traits.entry(self_type).or_default().insert(trait_name);
            }
        }

        // (owner, method) -> distinct RQNs of atoms whose RQN ends `owner::method`.
        // Needs no crate scoping: the public-output guard below already rejects
        // any RQN absent from this crate's `cargo public-api` surface.
        let mut trait_atoms: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for atom in atoms.values() {
            let Some(rqn) = &atom.rust_qualified_name else {
                continue;
            };
            if let Some((owner, method)) = split_last_two(rqn) {
                trait_atoms
                    .entry((owner.to_string(), method.to_string()))
                    .or_default()
                    .insert(rqn.clone());
            }
        }

        let public = self.flat();
        let mut additions = Vec::new();
        for (entry, forms) in &self.forms {
            if forms.iter().any(|f| atom_names.contains(f)) {
                continue; // already matched — leave it alone
            }
            let Some((self_type, method)) = split_last_two(entry) else {
                continue;
            };
            if overridden.contains(&(self_type.to_string(), method.to_string())) {
                continue; // the type defines it itself
            }
            let Some(traits) = impl_traits.get(self_type) else {
                continue; // no impl evidence for this type
            };
            let candidates: Vec<&BTreeSet<String>> = traits
                .iter()
                .filter_map(|t| trait_atoms.get(&(t.clone(), method.to_string())))
                .collect();
            // Ambiguity always skips: several traits could provide `method`, or
            // several atoms answer to the same `Trait::method`.
            let [rqns] = candidates[..] else {
                continue;
            };
            let [rqn] = &rqns.iter().collect::<Vec<_>>()[..] else {
                continue;
            };
            // The default body must itself be public API.
            if !public.contains(*rqn) && !public.contains(&normalize_rqn_for_public_api(rqn)) {
                continue;
            }
            additions.push((entry.clone(), (*rqn).clone()));
        }
        for (entry, form) in additions {
            self.add(&entry, form);
        }
    }

    /// Mark the atom that *implements* each still-unmatched entry.
    ///
    /// The name-based passes can only match an entry to an atom carrying that
    /// name. Some public functions have no such atom: macro-generated impls
    /// yield bodyless impl-evidence atoms with no RQN, and a blanket impl's
    /// atom is named after the impl's generic parameter, not the trait. Those
    /// atoms are nonetheless the crate's only atom for the entry, so a public
    /// entry that resolves to exactly one of them proves it public.
    ///
    /// Resolution is by SCIP impl descriptor from the atom key, restricted to
    /// `crate_name`'s own atoms, and only for entries no name matched. A
    /// qualified entry's module segments must also equal the atom key's module
    /// path (see [`key_module_path`]):
    ///
    /// - `path::Type::method` → the crate's single `impl#[Type][…]method()`
    ///   atom. A bare two-segment entry (`T::method`, printed by
    ///   `cargo public-api` for a blanket impl) additionally requires the
    ///   syn-verified blanket check, since a concrete type really named `T`
    ///   must not be resolved this way.
    /// - `path::Trait::method` → the crate's single `impl#[…][Trait]method()`
    ///   atom, always syn-verified as a blanket impl (`impl<T> Trait for T`),
    ///   whose one body serves every implementing type.
    ///
    /// Ambiguity (zero or several candidate atoms) always skips. Returns the
    /// number of atoms whose `is-public-api` changed to `true`.
    pub fn resolve_unmatched_to_atoms(
        &mut self,
        atoms: &mut BTreeMap<String, AtomWithLines>,
        atom_names: &HashSet<String>,
        crate_name: &str,
        project_path: &Path,
    ) -> usize {
        let crate_name = crate_name.replace('-', "_");
        let mut by_self: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut by_trait: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for key in atoms.keys() {
            if key_crate(key).map(|c| c.replace('-', "_")).as_deref() != Some(&crate_name) {
                continue; // never speak for a dependency's atoms
            }
            let Some((self_type, trait_name, method)) = parse_impl_key(key) else {
                continue;
            };
            by_self
                .entry((self_type, method.clone()))
                .or_default()
                .push(key.clone());
            if let Some(trait_name) = trait_name {
                by_trait
                    .entry((trait_name, method))
                    .or_default()
                    .push(key.clone());
            }
        }

        let mut resolutions: Vec<(String, String)> = Vec::new();
        for (entry, forms) in &self.forms {
            if forms.iter().any(|f| atom_names.contains(f)) {
                continue; // already matched by name
            }
            let Some((self_type, method)) = split_last_two(entry) else {
                continue;
            };
            let index_key = (self_type.to_string(), method.to_string());
            // `T::method`: no module path, so the receiver may be a generic param.
            let bare = entry.split("::").count() < 3;

            // The entry's module segments (between crate and type/trait) must
            // equal the atom key's module path: a lone `bar::Table::new` atom
            // must not answer for a public `foo::Table::new`. Bare entries have
            // no path; the blanket check guards them instead.
            let entry_mods: Option<String> = (!bare).then(|| {
                let segs: Vec<&str> = entry.split("::").collect();
                segs[1..segs.len() - 2].join("/")
            });
            let path_ok = |key: &String| match &entry_mods {
                None => true,
                Some(mods) => key_module_path(key) == Some(mods.as_str()),
            };
            let blanket = |key: &String| {
                atoms
                    .get(key)
                    .is_some_and(|a| is_blanket_impl_atom(a, project_path))
            };
            if let [key] = one(by_self.get(&index_key)) {
                if path_ok(key) && (!bare || blanket(key)) {
                    resolutions.push((entry.clone(), key.clone()));
                    continue;
                }
            }
            if let [key] = one(by_trait.get(&index_key)) {
                if path_ok(key) && blanket(key) {
                    resolutions.push((entry.clone(), key.clone()));
                }
            }
        }

        let mut marked = 0;
        for (entry, key) in resolutions {
            if let Some(atom) = atoms.get_mut(&key) {
                if atom.is_public_api != Some(true) {
                    atom.is_public_api = Some(true);
                    marked += 1;
                }
            }
            self.resolved.insert(entry);
        }
        marked
    }

    fn add(&mut self, entry: &str, form: String) {
        if let Some(forms) = self.forms.get_mut(entry) {
            forms.insert(form);
        }
    }
}

/// The crate segment of an atom key (`probe:<crate>/<version>/…`).
fn key_crate(key: &str) -> Option<&str> {
    key.strip_prefix("probe:")?.split('/').next()
}

/// The module path of an atom key: the segments between `probe:<crate>/<version>/`
/// and the final descriptor segment. Empty for crate-root items; multi-segment
/// (`backend/serial/u64/field`) stays `/`-joined.
///
/// The boundary is the *last `/` at top level*: `/` also occurs inside the
/// descriptor's bracketed generics (`impl<[u8;/{const}]>#…`) and backticked
/// types (`` [`&'a/Table`] ``), which must not count. A key this scan misparses
/// can only fail the caller's equality check and skip — never resolve wrongly.
fn key_module_path(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("probe:")?;
    let rest = &rest[rest.find('/')? + 1..]; // past <crate>/
    let tail = &rest[rest.find('/')? + 1..]; // past <version>/
    let mut depth = 0usize;
    let mut in_backtick = false;
    let mut last_slash = None;
    for (i, c) in tail.char_indices() {
        match c {
            '`' => in_backtick = !in_backtick,
            '[' | '<' | '{' | '(' if !in_backtick => depth += 1,
            ']' | '>' | '}' | ')' if !in_backtick => depth = depth.saturating_sub(1),
            '/' if !in_backtick && depth == 0 => last_slash = Some(i),
            _ => {}
        }
    }
    Some(last_slash.map_or("", |i| &tail[..i]))
}

/// Borrow an index bucket as a slice (empty when absent), for slice patterns.
fn one(bucket: Option<&Vec<String>>) -> &[String] {
    bucket.map_or(&[], Vec::as_slice)
}

/// Whether the atom's enclosing `impl` implements its trait for one of the
/// impl's own generic parameters (`impl<T> Trait for T`) — a blanket impl.
///
/// Verified with syn against the real source, because the atom key alone cannot
/// tell a generic parameter named `T` from a concrete type named `T`. An atom
/// with no usable span (a bodyless stub) is never treated as blanket.
fn is_blanket_impl_atom(atom: &AtomWithLines, project_path: &Path) -> bool {
    let (start, end) = (atom.code_text.lines_start, atom.code_text.lines_end);
    if start == 0 || end < start {
        return false;
    }
    let Ok(src) = std::fs::read_to_string(project_path.join(&atom.code_path)) else {
        return false;
    };
    let Ok(file) = syn::parse_file(&src) else {
        return false;
    };
    let mut impls = Vec::new();
    collect_impls(&file.items, &mut impls);
    impls.into_iter().any(|imp| {
        let span = imp.span();
        span.start().line <= start && end <= span.end().line && impl_self_is_generic_param(imp)
    })
}

/// Collect `impl` blocks, descending into inline `mod` bodies.
fn collect_impls<'a>(items: &'a [syn::Item], out: &mut Vec<&'a syn::ItemImpl>) {
    for item in items {
        match item {
            syn::Item::Impl(imp) => out.push(imp),
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_impls(inner, out);
                }
            }
            _ => {}
        }
    }
}

/// Whether an impl's self type is one of the impl's own type parameters.
fn impl_self_is_generic_param(imp: &syn::ItemImpl) -> bool {
    if imp.trait_.is_none() {
        return false;
    }
    let syn::Type::Path(path) = &*imp.self_ty else {
        return false;
    };
    let Some(ident) = path.path.get_ident() else {
        return false;
    };
    imp.generics.type_params().any(|p| p.ident == *ident)
}

/// Every name an atom can be matched under: its RQN and the normalized form.
pub fn atom_candidate_names(atoms: &BTreeMap<String, AtomWithLines>) -> HashSet<String> {
    let mut names = HashSet::new();
    for atom in atoms.values() {
        if let Some(rqn) = &atom.rust_qualified_name {
            names.insert(rqn.clone());
            names.insert(normalize_rqn_for_public_api(rqn));
        }
    }
    names
}

/// Split a `::`-path into `(second_to_last, last)` segments.
fn split_last_two(path: &str) -> Option<(&str, &str)> {
    let (head, last) = path.rsplit_once("::")?;
    let owner = head.rsplit("::").next()?;
    if owner.is_empty() || last.is_empty() {
        None
    } else {
        Some((owner, last))
    }
}

/// Parse an atom key's impl descriptor into `(self_type, trait, method)`.
///
/// `…/edwards/impl#[EdwardsBasepointTable][BasepointTable]basepoint()` yields
/// `("EdwardsBasepointTable", Some("BasepointTable"), "basepoint")`; an inherent
/// impl (`impl#[EdwardsPoint]mul_base_clamped()`) yields `None` for the trait.
/// Returns `None` for keys with no impl descriptor (free functions, trait-side
/// declarations).
fn parse_impl_key(key: &str) -> Option<(String, Option<String>, String)> {
    let start = key.rfind("#[")?;
    let rest = &key[start + 1..]; // starts at the '['
    let self_type = strip_lifetime(&crate::extract_bracket_type(rest)?);
    let after_self = &rest[rest.find(']')? + 1..];
    let (trait_name, tail) = if after_self.starts_with('[') {
        let t = strip_lifetime(&crate::extract_bracket_type(after_self)?);
        (Some(t), &after_self[after_self.find(']')? + 1..])
    } else {
        (None, after_self)
    };
    let method = tail.strip_suffix("()")?;
    if self_type.is_empty() || method.is_empty() {
        None
    } else {
        Some((self_type, trait_name, method.to_string()))
    }
}

/// Drop a leading lifetime from a SCIP bracket type (`'a/Type` → `Type`).
fn strip_lifetime(t: &str) -> String {
    t.strip_prefix('\'')
        .and_then(|rest| rest.split_once('/'))
        .map_or_else(|| t.to_string(), |(_, ty)| ty.to_string())
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

    // ---- PublicNameForms::resolve_unmatched_to_atoms ----

    fn stub_atom(rqn: Option<&str>, path: &str, lines: (usize, usize)) -> AtomWithLines {
        let mut a = make_atom("f", path, rqn);
        a.code_text.lines_start = lines.0;
        a.code_text.lines_end = lines.1;
        a
    }

    /// A macro-generated impl leaves a bodyless atom with no RQN — the only atom
    /// the crate has for that public function, so the entry marks it public.
    #[test]
    fn test_resolve_marks_impl_atom_without_rqn() {
        let mut atoms = BTreeMap::from([(
            "probe:c/1.0/edwards/impl#[Table][BasepointTable]create()".to_string(),
            stub_atom(None, "", (0, 0)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["c::edwards::Table::create"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", Path::new("."));

        assert_eq!(marked, 1);
        assert_eq!(forms.matched_count(&names), 1);
        assert_eq!(
            atoms.values().next().unwrap().is_public_api,
            Some(true),
            "the implementing atom carries the tag"
        );
    }

    /// Two atoms answer to the same `Type::method` — cannot tell which, so skip.
    #[test]
    fn test_resolve_skips_ambiguous_impl_atoms() {
        let mut atoms = BTreeMap::from([
            (
                "probe:c/1.0/a/impl#[Table][BasepointTable]create()".to_string(),
                stub_atom(None, "", (0, 0)),
            ),
            (
                "probe:c/1.0/b/impl#[Table][BasepointTable]create()".to_string(),
                stub_atom(None, "", (0, 0)),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["c::edwards::Table::create"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", Path::new("."));

        assert_eq!(marked, 0);
        assert_eq!(forms.matched_count(&names), 0);
        assert!(atoms.values().all(|a| a.is_public_api.is_none()));
    }

    /// Another crate's atom is never marked from this crate's public API.
    #[test]
    fn test_resolve_ignores_other_crates_atoms() {
        let mut atoms = BTreeMap::from([(
            "probe:other/1.0/edwards/impl#[Table][BasepointTable]create()".to_string(),
            stub_atom(None, "", (0, 0)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["c::edwards::Table::create"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", Path::new("."));

        assert_eq!(marked, 0);
        assert!(atoms.values().all(|a| a.is_public_api.is_none()));
    }

    const BLANKET_SRC: &str = "\
pub trait IsIdentity {
    fn is_identity(&self) -> bool;
}
impl<T> IsIdentity for T
where
    T: Default,
{
    fn is_identity(&self) -> bool {
        true
    }
}
";

    const CONCRETE_T_SRC: &str = "\
pub trait IsIdentity {
    fn is_identity(&self) -> bool;
}
pub struct T;
impl IsIdentity for T {
    fn is_identity(&self) -> bool {
        true
    }
}
";

    /// `pub fn T::is_identity(...)`: the bare receiver is the blanket impl's
    /// generic parameter, syn-verified against the source.
    #[test]
    fn test_resolve_blanket_impl_generic_self_matches() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(dir.path(), "c", "src/traits.rs", BLANKET_SRC);
        let mut atoms = BTreeMap::from([(
            "probe:c/1.0/traits/&T#impl<bool>#[T][IsIdentity]is_identity()".to_string(),
            stub_atom(Some("c::traits::T::is_identity"), "src/traits.rs", (8, 10)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["T::is_identity"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", dir.path());

        assert_eq!(marked, 1);
        assert_eq!(atoms.values().next().unwrap().is_public_api, Some(true));
    }

    /// Same atom key, but `T` is a concrete struct — must not be resolved from
    /// the bare `T::method` form.
    #[test]
    fn test_resolve_blanket_impl_concrete_self_type_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(dir.path(), "c", "src/traits.rs", CONCRETE_T_SRC);
        let mut atoms = BTreeMap::from([(
            "probe:c/1.0/traits/&T#impl<bool>#[T][IsIdentity]is_identity()".to_string(),
            stub_atom(Some("c::traits::T::is_identity"), "src/traits.rs", (6, 8)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["T::is_identity"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", dir.path());

        assert_eq!(marked, 0, "a concrete type named T is not a blanket impl");
        assert_eq!(atoms.values().next().unwrap().is_public_api, None);
    }

    /// A trait-level entry resolves to the blanket impl's atom.
    #[test]
    fn test_resolve_trait_level_entry_via_blanket_impl() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(dir.path(), "c", "src/traits.rs", BLANKET_SRC);
        let mut atoms = BTreeMap::from([(
            "probe:c/1.0/traits/&T#impl<bool>#[T][IsIdentity]is_identity()".to_string(),
            stub_atom(Some("c::traits::T::is_identity"), "src/traits.rs", (8, 10)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["c::traits::IsIdentity::is_identity"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", dir.path());

        assert_eq!(marked, 1);
        assert_eq!(forms.matched_count(&names), 1);
    }

    /// A lone atom in another module must not answer for the entry: the
    /// entry's module segments and the key's module path have to agree.
    #[test]
    fn test_resolve_module_path_mismatch_skipped() {
        let mut atoms = BTreeMap::from([(
            "probe:c/1.0/bar/impl#[Table][BasepointTable]create()".to_string(),
            stub_atom(None, "", (0, 0)),
        )]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&["c::foo::Table::create"]));

        let marked = forms.resolve_unmatched_to_atoms(&mut atoms, &names, "c", Path::new("."));

        assert_eq!(marked, 0);
        assert!(atoms.values().all(|a| a.is_public_api.is_none()));
    }

    #[test]
    fn test_key_module_path_shapes() {
        // Plain and multi-segment paths.
        assert_eq!(
            key_module_path("probe:c/1.0/edwards/impl#[Table][BasepointTable]create()"),
            Some("edwards")
        );
        assert_eq!(
            key_module_path("probe:c/1.0/backend/serial/u64/field/&F#impl<X>#[F][Debug]fmt()"),
            Some("backend/serial/u64/field")
        );
        // Crate-root item: empty module path.
        assert_eq!(
            key_module_path("probe:c/1.0/impl<&Formatter<'_>>#[DalekBits][Display]fmt()"),
            Some("")
        );
        // Receiver-decorated descriptor with no `/impl#` to search for.
        assert_eq!(
            key_module_path("probe:c/1.0/traits/&T#impl<bool>#[T][IsIdentity]is_identity()"),
            Some("traits")
        );
        // `/` inside bracketed generics and backticked types must not count.
        assert_eq!(
            key_module_path("probe:c/1.0/edwards/impl<[u8;/{const}]>#[Point]mul_base_clamped()"),
            Some("edwards")
        );
        assert_eq!(
            key_module_path("probe:c/1.0/edwards/impl#[`&'a/Table`][`Mul<&'b/Scalar>`]mul()"),
            Some("edwards")
        );
    }

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

    /// An alias claimed for two different definitions resolves to neither.
    #[test]
    fn test_collect_reexport_aliases_conflicting_alias_dropped() {
        let dir = tempfile::tempdir().unwrap();
        write_crate(
            dir.path(),
            "spqr",
            "src/lib.rs",
            "\
pub use crate::a::Foo;
pub use crate::b::Foo;
pub use crate::c::Bar;
",
        );

        let aliases = collect_reexport_aliases(dir.path(), "spqr");

        assert!(
            !aliases.contains_key("Foo"),
            "conflicting alias must be dropped, not last-write-win"
        );
        assert_eq!(aliases.get("Bar").map(String::as_str), Some("spqr::c::Bar"));
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

    /// Test-only convenience: run the `pub use` expansion pass and flatten.
    /// Production code drives `PublicNameForms` directly (`extract.rs`).
    fn expanded_with_aliases(
        public: &HashSet<String>,
        crate_name: &str,
        aliases: &BTreeMap<String, String>,
    ) -> HashSet<String> {
        let mut forms = PublicNameForms::new(public);
        forms.expand_with_aliases(crate_name, aliases);
        forms.flat()
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

        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

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

        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

        assert!(expanded.contains("spqr::serialize::Error::from"));
    }

    #[test]
    fn test_expand_empty_aliases_is_noop() {
        let aliases = BTreeMap::new();
        let public: HashSet<String> = ["spqr::send", "spqr::recv"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

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

        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

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
        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

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
        let expanded = expanded_with_aliases(&public, "spqr", &aliases);

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

    #[test]
    fn test_parse_impl_key_descriptor_forms() {
        assert_eq!(
            parse_impl_key("probe:c/1.0/edwards/impl#[Table][BasepointTable]basepoint()"),
            Some((
                "Table".to_string(),
                Some("BasepointTable".to_string()),
                "basepoint".to_string()
            ))
        );
        // Inherent impl: no trait segment.
        assert_eq!(
            parse_impl_key("probe:c/1.0/edwards/impl<[u8;/{const}]>#[Point]mul_base_clamped()"),
            Some(("Point".to_string(), None, "mul_base_clamped".to_string()))
        );
        // Backticks and the `'a/` lifetime form are stripped from the self type.
        assert_eq!(
            parse_impl_key("probe:c/1.0/edwards/impl#[`&\'a/Table`][`Mul<&\'b/Scalar>`]mul()")
                .map(|(t, _, m)| (t, m)),
            Some(("Table".to_string(), "mul".to_string()))
        );
        // A receiver-prefixed key still parses from its impl descriptor.
        assert_eq!(
            parse_impl_key("probe:c/1.0/traits/&T#impl<bool>#[T][IsIdentity]is_identity()"),
            Some((
                "T".to_string(),
                Some("IsIdentity".to_string()),
                "is_identity".to_string()
            ))
        );
        // Trait-side declarations and free functions carry no impl descriptor.
        assert_eq!(
            parse_impl_key("probe:c/1.0/traits/BasepointTable#mul_base()"),
            None
        );
        assert_eq!(parse_impl_key("probe:c/1.0/edwards/free_fn()"), None);
    }

    // ---- PublicNameForms: entry-level bookkeeping ----

    fn atom_map(entries: &[(&str, Option<&str>)]) -> BTreeMap<String, AtomWithLines> {
        entries
            .iter()
            .map(|(key, rqn)| ((*key).to_string(), make_atom("f", "src/lib.rs", *rqn)))
            .collect()
    }

    fn public(entries: &[&str]) -> HashSet<String> {
        entries.iter().map(|s| (*s).to_string()).collect()
    }

    /// Impl evidence + no override + one public trait body ⇒ entry resolves to
    /// the trait atom.
    #[test]
    fn test_trait_default_inherited_by_concrete_type() {
        let atoms = atom_map(&[
            (
                "probe:c/1.0/edwards/impl#[Table][BasepointTable]create()",
                Some("c::edwards::Table::create"),
            ),
            (
                "probe:c/1.0/traits/&Self#BasepointTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::BasepointTable::mul_base_clamped"),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&[
            "c::traits::BasepointTable::mul_base_clamped",
            "c::edwards::Table::mul_base_clamped",
        ]));
        assert_eq!(forms.matched_count(&names), 1);

        forms.expand_with_trait_defaults(&atoms, &names, "c");

        assert_eq!(forms.matched_count(&names), 2);
        assert_eq!(forms.len(), 2);
    }

    /// A type that defines the method itself is not inheriting the default.
    #[test]
    fn test_trait_default_skipped_when_override_exists() {
        let atoms = atom_map(&[
            (
                "probe:c/1.0/edwards/impl#[Table][BasepointTable]create()",
                Some("c::edwards::Table::create"),
            ),
            (
                "probe:c/1.0/other/impl#[Table]mul_base_clamped()",
                Some("c::other::Table::mul_base_clamped"),
            ),
            (
                "probe:c/1.0/traits/&Self#BasepointTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::BasepointTable::mul_base_clamped"),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&[
            "c::traits::BasepointTable::mul_base_clamped",
            "c::edwards::Table::mul_base_clamped",
        ]));

        forms.expand_with_trait_defaults(&atoms, &names, "c");

        assert_eq!(
            forms.matched_count(&names),
            1,
            "override evidence must block the trait-default form"
        );
    }

    /// Two traits could supply the method — ambiguous, so skip.
    #[test]
    fn test_trait_default_ambiguous_traits_skipped() {
        let atoms = atom_map(&[
            (
                "probe:c/1.0/edwards/impl#[Table][BasepointTable]create()",
                Some("c::edwards::Table::create"),
            ),
            (
                "probe:c/1.0/edwards/impl#[Table][OtherTable]other()",
                Some("c::edwards::Table::other"),
            ),
            (
                "probe:c/1.0/traits/&Self#BasepointTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::BasepointTable::mul_base_clamped"),
            ),
            (
                "probe:c/1.0/traits/&Self#OtherTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::OtherTable::mul_base_clamped"),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&[
            "c::traits::BasepointTable::mul_base_clamped",
            "c::traits::OtherTable::mul_base_clamped",
            "c::edwards::Table::mul_base_clamped",
        ]));

        forms.expand_with_trait_defaults(&atoms, &names, "c");

        assert_eq!(
            forms.matched_count(&names),
            2,
            "ambiguous providing trait must skip"
        );
    }

    /// A default body that is not itself public API cannot make an entry match.
    #[test]
    fn test_trait_default_requires_public_trait_method() {
        let atoms = atom_map(&[
            (
                "probe:c/1.0/edwards/impl#[Table][BasepointTable]create()",
                Some("c::edwards::Table::create"),
            ),
            (
                "probe:c/1.0/traits/&Self#BasepointTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::BasepointTable::mul_base_clamped"),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        // The trait method itself is absent from the public API surface.
        let mut forms = PublicNameForms::new(&public(&["c::edwards::Table::mul_base_clamped"]));

        forms.expand_with_trait_defaults(&atoms, &names, "c");

        assert_eq!(forms.matched_count(&names), 0);
        assert!(!forms
            .flat()
            .contains("c::traits::BasepointTable::mul_base_clamped"));
    }

    /// A dependency's impl keys are not evidence: they neither prove the
    /// `Type: Trait` link nor veto with a phantom override.
    #[test]
    fn test_trait_default_ignores_dependency_impl_evidence() {
        let atoms = atom_map(&[
            // The only impl evidence for `Table: BasepointTable` is a
            // dependency's — it must not enable the match.
            (
                "probe:dep/1.0/edwards/impl#[Table][BasepointTable]create()",
                Some("dep::edwards::Table::create"),
            ),
            (
                "probe:c/1.0/traits/&Self#BasepointTable<[u8;/{const}]>#mul_base_clamped()",
                Some("c::traits::BasepointTable::mul_base_clamped"),
            ),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&[
            "c::traits::BasepointTable::mul_base_clamped",
            "c::edwards::Table::mul_base_clamped",
        ]));

        forms.expand_with_trait_defaults(&atoms, &names, "c");

        assert_eq!(
            forms.matched_count(&names),
            1,
            "dependency impl evidence must not resolve the concrete-type entry"
        );
    }

    #[test]
    fn test_matched_count_reporting() {
        let atoms = atom_map(&[
            ("probe:spqr/1.0/chain/new()", Some("spqr::chain::new")),
            ("probe:spqr/1.0/gone()", None),
        ]);
        let names = atom_candidate_names(&atoms);
        let mut forms = PublicNameForms::new(&public(&[
            "spqr::Chain::new", // reachable only via the re-export alias
            "spqr::missing::thing",
        ]));
        assert_eq!(forms.len(), 2);
        assert_eq!(forms.matched_count(&names), 0);

        let aliases = BTreeMap::from([("Chain".to_string(), "spqr::chain".to_string())]);
        forms.expand_with_aliases("spqr", &aliases);

        assert_eq!(forms.matched_count(&names), 1);
        assert_eq!(forms.len(), 2, "expansion never adds or drops entries");
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
