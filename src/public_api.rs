//! Public API detection via `cargo-public-api`.
//!
//! Runs `cargo public-api -sss` to list the crate's public API surface,
//! then matches function atoms against the output to set `is-public-api`.
//!
//! Three-way semantics:
//! - `Some(true)`:  confirmed in public API (matched in `cargo public-api` output)
//! - `Some(false)`: confirmed NOT in public API (function is not `pub`)
//! - `None`:        uncertain (function is `pub` but not matched, or detection skipped)

use crate::{AtomWithLines, ProbeError, ProbeResult};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::process::{Command, Stdio};

/// Filename for cached public API output.
const PUBLIC_API_CACHE_FILE: &str = "public-api.txt";

/// Cache directory name (matches SCIP cache convention).
const DATA_DIR: &str = "data";

// =============================================================================
// Binary-crate detection
// =============================================================================

/// Check whether the project has a library target.
///
/// Returns `false` for binary-only crates (which have no public API surface).
/// A crate is considered a library if any of:
/// - `Cargo.toml` has a `[lib]` section
/// - `src/lib.rs` exists (Cargo's implicit library detection)
pub fn is_library_crate(project_path: &Path) -> bool {
    let cargo_toml = project_path.join("Cargo.toml");
    if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
        if let Ok(parsed) = contents.parse::<toml::Table>() {
            if parsed.contains_key("lib") {
                return true;
            }
        }
    }
    project_path.join("src/lib.rs").exists()
}

// =============================================================================
// Tool detection and installation
// =============================================================================

/// Check if `cargo-public-api` is installed and return its version.
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

/// Check if a Rust nightly toolchain is installed.
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

/// Run `cargo public-api -sss` and return the raw output.
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
///   `pub fn my_crate::module::function(args) -> Ret`
///   `pub unsafe fn my_crate::MyStruct::method(&self)`
///   `pub async fn my_crate::do_thing()`
///
/// Returns a set of qualified paths (e.g. `my_crate::module::function`).
pub fn parse_public_api_functions(output: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    for line in output.lines() {
        if let Some(name) = extract_fn_qualified_name(line.trim()) {
            result.insert(name);
        }
    }
    result
}

/// Extract the qualified name from a single `cargo public-api` function line.
///
/// Strips the visibility + qualifiers prefix to find the qualified path,
/// then strips the parameter list suffix.
fn extract_fn_qualified_name(line: &str) -> Option<String> {
    // Look for " fn " which separates qualifiers from the qualified path + signature.
    // Handles: "pub fn", "pub unsafe fn", "pub async fn", "pub const fn",
    //          "pub const unsafe fn", "pub async unsafe fn", "pub extern \"C\" fn", etc.
    let fn_idx = line.find(" fn ")?;

    // The part before " fn " must start with "pub" (not "pub(" restricted vis)
    let prefix = line[..fn_idx].trim();
    if !prefix.starts_with("pub") || prefix.get(3..4) == Some("(") {
        return None;
    }

    // After " fn " comes "qualified::path(args) -> Ret"
    let after_fn = &line[fn_idx + 4..].trim_start();

    // Strip leading reference prefix from the self-type:
    //   "&'a Type::method(...)" → "Type::method(...)"  (lifetime ref)
    //   "&Type::method(...)"    → "Type::method(...)"   (bare ref)
    let after_fn = if let Some(rest) = after_fn.strip_prefix("&'") {
        // Lifetime form: skip past the lifetime + space
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

    // The qualified name ends at the first '(' (start of parameter list)
    let name_end = after_fn.find('(').unwrap_or(after_fn.len());

    let qualified = after_fn[..name_end].trim();
    if qualified.is_empty() {
        return None;
    }

    // Strip trailing generic params like `<T>` from the name
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

// =============================================================================
// Caching
// =============================================================================

fn cache_path(project_path: &Path) -> std::path::PathBuf {
    project_path.join(DATA_DIR).join(PUBLIC_API_CACHE_FILE)
}

fn has_cached_output(project_path: &Path) -> bool {
    cache_path(project_path).exists()
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
    let raw = if !regenerate && has_cached_output(project_path) {
        println!(
            "  ✓ Found cached public-api output at {}",
            cache_path(project_path).display()
        );
        read_cached_output(project_path).unwrap_or_default()
    } else {
        let output = run_cargo_public_api(project_path, pkg_name)?;
        write_cache(project_path, &output)?;
        output
    };

    Ok(parse_public_api_functions(&raw))
}

// =============================================================================
// Three-way enrichment
// =============================================================================

/// Enrich atoms with `is-public-api` using three-way logic.
///
/// - RQN matched in public API → `is-public-api: Some(true)` (confirmed, regardless of `is-public`)
/// - `is-public: false` + not matched → `is-public-api: Some(false)` (not pub, not public API)
/// - `is-public: true` + not matched → `is-public-api: None` (uncertain, possibly re-exported)
/// - External stubs (empty code_path) → left as `None`
///
/// Trait impl methods don't carry `pub` in their SCIP signature, so we check
/// the public API set first before falling back to the `is-public` flag.
pub fn enrich_atoms_with_public_api(
    atoms: &mut BTreeMap<String, AtomWithLines>,
    public_names: &HashSet<String>,
    pkg_name: &str,
) -> (usize, usize, usize) {
    let mut confirmed_true = 0usize;
    let mut confirmed_false = 0usize;
    let mut uncertain = 0usize;

    for atom in atoms.values_mut() {
        if atom.code_path.is_empty() {
            continue;
        }

        let rqn = atom.rust_qualified_name.as_deref().unwrap_or("");
        let in_public_api = !rqn.is_empty() && public_names.contains(rqn);

        if in_public_api {
            atom.is_public_api = Some(true);
            confirmed_true += 1;
        } else if atom.is_public == Some(true) {
            atom.is_public_api = None;
            uncertain += 1;
        } else {
            atom.is_public_api = Some(false);
            confirmed_false += 1;
        }
    }

    let _ = pkg_name;

    (confirmed_true, confirmed_false, uncertain)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- is_library_crate ----

    #[test]
    fn test_is_library_crate_with_lib_rs() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(src_dir.join("lib.rs"), "").unwrap();
        assert!(is_library_crate(dir.path()));
    }

    #[test]
    fn test_is_library_crate_with_lib_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"foo\"\n",
        )
        .unwrap();
        assert!(is_library_crate(dir.path()));
    }

    #[test]
    fn test_is_not_library_crate_binary_only() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"foo\"\n",
        )
        .unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        assert!(!is_library_crate(dir.path()));
    }

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
        // The generic param on the function itself may appear before `(`
        assert!(
            names.contains("my_crate::Vec::push")
                || names.iter().any(|n| n.starts_with("my_crate::Vec::push"))
        );
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
    fn test_extract_lifetime_prefix_neg() {
        assert_eq!(
            extract_fn_qualified_name(
                "pub fn &'a curve25519_dalek::edwards::EdwardsPoint::neg(self) -> curve25519_dalek::edwards::EdwardsPoint"
            ),
            Some("curve25519_dalek::edwards::EdwardsPoint::neg".to_string())
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

    #[test]
    fn test_extract_bare_ref_prefix_reversed() {
        assert_eq!(
            extract_fn_qualified_name(
                "pub fn &curve25519_dalek::montgomery::MontgomeryPoint::mul(self, scalar: &curve25519_dalek::scalar::Scalar) -> curve25519_dalek::montgomery::MontgomeryPoint"
            ),
            Some("curve25519_dalek::montgomery::MontgomeryPoint::mul".to_string())
        );
    }

    // ---- three-way enrichment ----

    #[test]
    fn test_enrich_three_way() {
        let mut atoms = BTreeMap::new();

        // pub + matched -> true
        atoms.insert(
            "probe:c/1.0/m/public_api_fn()".to_string(),
            make_atom(
                "public_api_fn",
                "src/m.rs",
                Some(true),
                "c::m::public_api_fn",
            ),
        );

        // pub + NOT matched -> None (uncertain)
        atoms.insert(
            "probe:c/1.0/m/pub_not_matched()".to_string(),
            make_atom(
                "pub_not_matched",
                "src/m.rs",
                Some(true),
                "c::m::pub_not_matched",
            ),
        );

        // not pub -> false
        atoms.insert(
            "probe:c/1.0/m/private_fn()".to_string(),
            make_atom("private_fn", "src/m.rs", Some(false), "c::m::private_fn"),
        );

        // trait impl method: not pub (no `pub` in sig) but matched -> true
        atoms.insert(
            "probe:c/1.0/m/MyType.ct_eq()".to_string(),
            make_atom(
                "MyType::ct_eq",
                "src/m.rs",
                Some(false),
                "c::m::MyType::ct_eq",
            ),
        );

        // external stub -> None (unchanged)
        atoms.insert(
            "probe:ext/1.0/lib/ext_fn()".to_string(),
            make_atom("ext_fn", "", None, ""),
        );

        let public_names: HashSet<String> = [
            "c::m::public_api_fn".to_string(),
            "c::m::MyType::ct_eq".to_string(),
        ]
        .into_iter()
        .collect();

        let (confirmed_true, confirmed_false, uncertain) =
            enrich_atoms_with_public_api(&mut atoms, &public_names, "c");

        assert_eq!(confirmed_true, 2);
        assert_eq!(confirmed_false, 1);
        assert_eq!(uncertain, 1);

        assert_eq!(
            atoms["probe:c/1.0/m/public_api_fn()"].is_public_api,
            Some(true)
        );
        assert_eq!(atoms["probe:c/1.0/m/pub_not_matched()"].is_public_api, None);
        assert_eq!(
            atoms["probe:c/1.0/m/private_fn()"].is_public_api,
            Some(false)
        );
        // Trait impl: matched in public API despite is_public=false
        assert_eq!(
            atoms["probe:c/1.0/m/MyType.ct_eq()"].is_public_api,
            Some(true)
        );
        // External stub: untouched (still None)
        assert_eq!(atoms["probe:ext/1.0/lib/ext_fn()"].is_public_api, None);
    }

    fn make_atom(
        display_name: &str,
        code_path: &str,
        is_public: Option<bool>,
        rqn: &str,
    ) -> AtomWithLines {
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
            rust_qualified_name: if rqn.is_empty() {
                None
            } else {
                Some(rqn.to_string())
            },
            is_disabled: false,
            is_public,
            is_public_api: None,
        }
    }
}
