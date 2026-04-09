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

use crate::{AtomWithLines, ProbeError, ProbeResult};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
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
// RQN-based enrichment
// =============================================================================

/// Override `is-public-api` for all atoms that have a `rust-qualified-name`.
///
/// For each atom:
/// - If `rust-qualified-name` is `None` (external stubs) → leave `is-public-api` unchanged
/// - If RQN is in `public_names` → `is-public-api = Some(true)`
/// - Otherwise → `is-public-api = Some(false)`
///
/// Returns `(overridden_true, overridden_false)` counts.
pub fn enrich_atoms_with_public_api(
    atoms: &mut BTreeMap<String, AtomWithLines>,
    public_names: &HashSet<String>,
) -> (usize, usize) {
    let mut overridden_true = 0;
    let mut overridden_false = 0;

    for atom in atoms.values_mut() {
        let Some(rqn) = &atom.rust_qualified_name else {
            continue;
        };

        if public_names.contains(rqn) {
            atom.is_public_api = Some(true);
            overridden_true += 1;
        } else {
            atom.is_public_api = Some(false);
            overridden_false += 1;
        }
    }

    (overridden_true, overridden_false)
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

    // ---- enrich_atoms_with_public_api ----

    #[test]
    fn test_enrich_rqn_based() {
        let mut atoms = BTreeMap::new();

        // Atom with RQN in public set → true
        atoms.insert(
            "probe:c/1.0/m/public_fn()".to_string(),
            make_atom("public_fn", "src/m.rs", Some("c::m::public_fn")),
        );

        // Atom with RQN not in public set → false
        atoms.insert(
            "probe:c/1.0/m/private_fn()".to_string(),
            make_atom("private_fn", "src/m.rs", Some("c::m::private_fn")),
        );

        // External stub (no RQN) → unchanged (None)
        atoms.insert(
            "probe:ext/1.0/lib/ext_fn()".to_string(),
            make_atom("ext_fn", "", None),
        );

        // Trait impl with RQN in public set → true
        atoms.insert(
            "probe:c/1.0/m/MyType.ct_eq()".to_string(),
            make_atom("MyType::ct_eq", "src/m.rs", Some("c::m::MyType::ct_eq")),
        );

        let public_names: HashSet<String> = ["c::m::public_fn", "c::m::MyType::ct_eq"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let (overridden_true, overridden_false) =
            enrich_atoms_with_public_api(&mut atoms, &public_names);

        assert_eq!(overridden_true, 2);
        assert_eq!(overridden_false, 1);

        assert_eq!(atoms["probe:c/1.0/m/public_fn()"].is_public_api, Some(true));
        assert_eq!(
            atoms["probe:c/1.0/m/private_fn()"].is_public_api,
            Some(false)
        );
        assert_eq!(atoms["probe:ext/1.0/lib/ext_fn()"].is_public_api, None);
        assert_eq!(
            atoms["probe:c/1.0/m/MyType.ct_eq()"].is_public_api,
            Some(true)
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
            is_disabled: false,
            is_public: None,
            is_public_api: None,
        }
    }
}
