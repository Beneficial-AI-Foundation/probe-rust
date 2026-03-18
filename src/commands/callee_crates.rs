//! Callee-crates command - Find which crates a function's callees belong to.
//!
//! Given a function and a depth N, traverses the call graph (BFS) up to
//! depth N and reports which crates the discovered callees belong to,
//! grouped by crate name and version.

use crate::metadata::unwrap_envelope;
use crate::{AtomWithLines, ProbeError, ProbeResult};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::io::Read;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct CalleeCratesOutput {
    pub function: String,
    pub depth: usize,
    pub crates: Vec<CrateEntry>,
}

#[derive(Serialize)]
pub struct CrateEntry {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub version: String,
    pub functions: Vec<String>,
}

/// Extract (crate_name, version) from a `probe:` code-name.
///
/// Standard library crates use a GitHub URL instead of a semver version
/// (e.g. `probe:core/https://github.com/rust-lang/rust/...`), so we
/// return `"stdlib"` for those.
#[must_use]
pub fn extract_crate_info(code_name: &str) -> Option<(&str, &str)> {
    let rest = code_name.strip_prefix("probe:")?;
    let mut parts = rest.splitn(3, '/');
    let crate_name = parts.next()?;
    let version = parts.next()?;
    if version.starts_with("https:") {
        Some((crate_name, "stdlib"))
    } else {
        Some((crate_name, version))
    }
}

/// BFS traversal collecting all callees reachable within depth 1..=max_depth.
/// Returns the set of callee code-names (excluding the root function itself).
#[must_use]
pub fn collect_callees_up_to_depth(
    atoms: &BTreeMap<String, AtomWithLines>,
    root: &str,
    max_depth: usize,
) -> BTreeSet<String> {
    let mut visited = HashSet::new();
    visited.insert(root.to_string());

    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((root.to_string(), 0));

    let mut result = BTreeSet::new();

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(atom) = atoms.get(&current) {
            for dep in &atom.dependencies {
                if visited.insert(dep.clone()) {
                    result.insert(dep.clone());
                    queue.push_back((dep.clone(), depth + 1));
                }
            }
        }
    }

    result
}

const STDLIB_CRATES: &[&str] = &["core", "alloc", "std"];

/// Group a set of code-names by (crate, version), returning sorted CrateEntry list.
/// Optionally excludes stdlib crates and/or a custom set of crate names.
#[must_use]
pub fn group_by_crate(
    code_names: &BTreeSet<String>,
    exclude_stdlib: bool,
    exclude_crates: &[String],
) -> Vec<CrateEntry> {
    let mut groups: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for name in code_names {
        if let Some((crate_name, version)) = extract_crate_info(name) {
            if exclude_stdlib && STDLIB_CRATES.contains(&crate_name) {
                continue;
            }
            if exclude_crates.iter().any(|e| e == crate_name) {
                continue;
            }
            groups
                .entry((crate_name.to_string(), version.to_string()))
                .or_default()
                .insert(name.clone());
        }
    }

    groups
        .into_iter()
        .map(|((crate_name, version), functions)| CrateEntry {
            crate_name,
            version,
            functions: functions.into_iter().collect(),
        })
        .collect()
}

/// Resolve a function argument to a code-name key in the atoms map.
///
/// If the argument starts with `probe:`, it is used as-is.
/// Otherwise, search for keys whose display-name matches the argument.
fn resolve_function(
    atoms: &BTreeMap<String, AtomWithLines>,
    function_arg: &str,
) -> ProbeResult<String> {
    if function_arg.starts_with("probe:") {
        if atoms.contains_key(function_arg) {
            return Ok(function_arg.to_string());
        }
        return Err(ProbeError::ProjectValidation(format!(
            "Function '{}' not found in atoms data",
            function_arg
        )));
    }

    let matches: Vec<&String> = atoms
        .iter()
        .filter(|(_, atom)| atom.display_name == function_arg)
        .map(|(key, _)| key)
        .collect();

    match matches.len() {
        0 => {
            let partial: Vec<&String> = atoms
                .iter()
                .filter(|(key, atom)| {
                    atom.display_name.contains(function_arg) || key.contains(function_arg)
                })
                .map(|(key, _)| key)
                .collect();
            if partial.len() == 1 {
                return Ok(partial[0].clone());
            }
            if partial.is_empty() {
                Err(ProbeError::ProjectValidation(format!(
                    "No function matching '{}' found in atoms data",
                    function_arg
                )))
            } else {
                let mut msg = format!(
                    "Ambiguous function '{}'. {} matches found:\n",
                    function_arg,
                    partial.len()
                );
                for (i, key) in partial.iter().enumerate().take(10) {
                    msg.push_str(&format!("  {}. {}\n", i + 1, key));
                }
                if partial.len() > 10 {
                    msg.push_str(&format!("  ... and {} more\n", partial.len() - 10));
                }
                Err(ProbeError::ProjectValidation(msg))
            }
        }
        1 => Ok(matches[0].clone()),
        _ => {
            let mut msg = format!(
                "Ambiguous display-name '{}'. {} matches found:\n",
                function_arg,
                matches.len()
            );
            for (i, key) in matches.iter().enumerate().take(10) {
                msg.push_str(&format!("  {}. {}\n", i + 1, key));
            }
            if matches.len() > 10 {
                msg.push_str(&format!("  ... and {} more\n", matches.len() - 10));
            }
            Err(ProbeError::ProjectValidation(msg))
        }
    }
}

/// Load atoms from a file or stdin, supporting both bare-dict and enveloped formats.
fn load_atoms(atoms_file: Option<PathBuf>) -> ProbeResult<BTreeMap<String, AtomWithLines>> {
    let json: serde_json::Value = match atoms_file {
        Some(path) => {
            let file = std::fs::File::open(&path).map_err(|e| ProbeError::file_io(&path, e))?;
            let reader = std::io::BufReader::new(file);
            serde_json::from_reader(reader)?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            serde_json::from_str(&buf)?
        }
    };

    let data = unwrap_envelope(json);
    let mut atoms: BTreeMap<String, AtomWithLines> = serde_json::from_value(data)?;
    for (key, atom) in &mut atoms {
        if atom.code_name.is_empty() {
            atom.code_name = key.clone();
        }
    }
    Ok(atoms)
}

/// Execute the callee-crates command.
pub fn cmd_callee_crates(
    function: String,
    depth: usize,
    atoms_file: Option<PathBuf>,
    output: Option<PathBuf>,
    exclude_stdlib: bool,
    exclude_crates: Vec<String>,
) -> ProbeResult<()> {
    let atoms = load_atoms(atoms_file)?;
    let resolved = resolve_function(&atoms, &function)?;

    let callees = collect_callees_up_to_depth(&atoms, &resolved, depth);
    let crates = group_by_crate(&callees, exclude_stdlib, &exclude_crates);

    let output_data = CalleeCratesOutput {
        function: resolved,
        depth,
        crates,
    };

    let json = serde_json::to_string_pretty(&output_data)?;

    match output {
        Some(path) => {
            std::fs::write(&path, &json).map_err(|e| ProbeError::file_io(&path, e))?;
            eprintln!("Output written to {}", path.display());
        }
        None => {
            println!("{}", json);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CodeTextInfo, DeclKind};

    fn make_atom(name: &str, deps: &[&str]) -> AtomWithLines {
        AtomWithLines {
            display_name: name.to_string(),
            code_name: String::new(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            dependencies_with_locations: Vec::new(),
            code_module: String::new(),
            code_path: String::new(),
            code_text: CodeTextInfo {
                lines_start: 1,
                lines_end: 10,
            },
            kind: DeclKind::Exec,
            language: "rust".to_string(),
            rust_qualified_name: None,
            is_disabled: false,
        }
    }

    #[test]
    fn test_extract_crate_info() {
        assert_eq!(
            extract_crate_info("probe:my-crate/1.0.0/module/func()"),
            Some(("my-crate", "1.0.0"))
        );
        assert_eq!(
            extract_crate_info("probe:core/https://github.com/rust-lang/rust/lib/func()"),
            Some(("core", "stdlib"))
        );
        assert_eq!(extract_crate_info("not-a-probe-name"), None);
    }

    #[test]
    fn test_collect_callees_depth_1() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:a/1.0/f()".to_string(),
            make_atom("f", &["probe:a/1.0/g()", "probe:b/2.0/h()"]),
        );
        atoms.insert("probe:a/1.0/g()".to_string(), make_atom("g", &[]));
        atoms.insert(
            "probe:b/2.0/h()".to_string(),
            make_atom("h", &["probe:c/3.0/i()"]),
        );
        atoms.insert("probe:c/3.0/i()".to_string(), make_atom("i", &[]));

        let callees = collect_callees_up_to_depth(&atoms, "probe:a/1.0/f()", 1);
        assert_eq!(callees.len(), 2);
        assert!(callees.contains("probe:a/1.0/g()"));
        assert!(callees.contains("probe:b/2.0/h()"));
        assert!(!callees.contains("probe:c/3.0/i()"));
    }

    #[test]
    fn test_collect_callees_depth_2() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:a/1.0/f()".to_string(),
            make_atom("f", &["probe:b/2.0/h()"]),
        );
        atoms.insert(
            "probe:b/2.0/h()".to_string(),
            make_atom("h", &["probe:c/3.0/i()"]),
        );
        atoms.insert("probe:c/3.0/i()".to_string(), make_atom("i", &[]));

        let callees = collect_callees_up_to_depth(&atoms, "probe:a/1.0/f()", 2);
        assert_eq!(callees.len(), 2);
        assert!(callees.contains("probe:b/2.0/h()"));
        assert!(callees.contains("probe:c/3.0/i()"));
    }

    #[test]
    fn test_collect_callees_handles_cycles() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:a/1.0/f()".to_string(),
            make_atom("f", &["probe:a/1.0/g()"]),
        );
        atoms.insert(
            "probe:a/1.0/g()".to_string(),
            make_atom("g", &["probe:a/1.0/f()"]),
        );

        let callees = collect_callees_up_to_depth(&atoms, "probe:a/1.0/f()", 10);
        assert_eq!(callees.len(), 1);
        assert!(callees.contains("probe:a/1.0/g()"));
    }

    #[test]
    fn test_collect_callees_depth_0() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:a/1.0/f()".to_string(),
            make_atom("f", &["probe:a/1.0/g()"]),
        );

        let callees = collect_callees_up_to_depth(&atoms, "probe:a/1.0/f()", 0);
        assert!(callees.is_empty());
    }

    #[test]
    fn test_group_by_crate() {
        let mut names = BTreeSet::new();
        names.insert("probe:my-crate/1.0.0/module/func()".to_string());
        names.insert("probe:my-crate/1.0.0/other/helper()".to_string());
        names.insert("probe:dep/2.0.0/lib/thing()".to_string());

        let groups = group_by_crate(&names, false, &[]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].crate_name, "dep");
        assert_eq!(groups[0].functions.len(), 1);
        assert_eq!(groups[1].crate_name, "my-crate");
        assert_eq!(groups[1].functions.len(), 2);
    }

    #[test]
    fn test_group_by_crate_exclude_stdlib() {
        let mut names = BTreeSet::new();
        names.insert("probe:my-crate/1.0.0/f()".to_string());
        names.insert("probe:core/https://github.com/rust-lang/rust/lib/clone()".to_string());

        let groups = group_by_crate(&names, true, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].crate_name, "my-crate");
    }

    #[test]
    fn test_group_by_crate_exclude_custom() {
        let mut names = BTreeSet::new();
        names.insert("probe:my-crate/1.0.0/f()".to_string());
        names.insert("probe:dep/2.0.0/g()".to_string());

        let groups = group_by_crate(&names, false, &["dep".to_string()]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].crate_name, "my-crate");
    }

    #[test]
    fn test_resolve_function_exact_code_name() {
        let mut atoms = BTreeMap::new();
        atoms.insert("probe:a/1.0/f()".to_string(), make_atom("f", &[]));

        let result = resolve_function(&atoms, "probe:a/1.0/f()");
        assert_eq!(result.unwrap(), "probe:a/1.0/f()");
    }

    #[test]
    fn test_resolve_function_by_display_name() {
        let mut atoms = BTreeMap::new();
        atoms.insert("probe:a/1.0/f()".to_string(), make_atom("my_func", &[]));

        let result = resolve_function(&atoms, "my_func");
        assert_eq!(result.unwrap(), "probe:a/1.0/f()");
    }

    #[test]
    fn test_resolve_function_not_found() {
        let atoms: BTreeMap<String, AtomWithLines> = BTreeMap::new();
        let result = resolve_function(&atoms, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_function_ambiguous() {
        let mut atoms = BTreeMap::new();
        atoms.insert("probe:a/1.0/f()".to_string(), make_atom("dup", &[]));
        atoms.insert("probe:b/1.0/f()".to_string(), make_atom("dup", &[]));

        let result = resolve_function(&atoms, "dup");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Ambiguous"));
    }

    #[test]
    fn test_resolve_function_partial_match_unique() {
        let mut atoms = BTreeMap::new();
        atoms.insert(
            "probe:a/1.0/MyStruct#process()".to_string(),
            make_atom("MyStruct::process", &[]),
        );

        let result = resolve_function(&atoms, "process");
        assert_eq!(result.unwrap(), "probe:a/1.0/MyStruct#process()");
    }
}
