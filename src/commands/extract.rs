//! Extract command - Generate call graph atoms from SCIP indexes.

use crate::{
    add_external_stubs, build_call_graph,
    charon_cache::CharonCache,
    charon_names, convert_to_atoms_with_parsed_spans, find_duplicate_code_names,
    metadata::{gather_metadata, get_default_output_path, wrap_in_envelope},
    parse_scip_json,
    scip_cache::ScipCache,
    AtomWithLines, ProbeError, ProbeResult,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Execute the extract command.
pub fn cmd_extract(
    project_path: PathBuf,
    output: Option<PathBuf>,
    regenerate_scip: bool,
    with_locations: bool,
    allow_duplicates: bool,
    auto_install: bool,
    with_charon: bool,
) -> ProbeResult<()> {
    println!("═══════════════════════════════════════════════════════════");
    println!("  Probe Rust - Extract: Generate Call Graph Data");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    let project_path = validate_project(&project_path)?;
    println!("  ✓ Valid Rust project found");

    let mut scip_cache = ScipCache::new(&project_path).with_auto_install(auto_install);
    let json_path = get_scip_json(&mut scip_cache, regenerate_scip)?;

    println!("Parsing SCIP JSON and building call graph...");

    let scip_index = parse_scip_json(&json_path)?;

    let (call_graph, symbol_to_display_name) = build_call_graph(&scip_index);
    println!("  ✓ Call graph built with {} functions", call_graph.len());
    println!();

    let metadata = gather_metadata(&project_path);

    println!("Converting to atoms format with accurate line numbers...");
    println!("  Parsing source files with syn for accurate function spans...");

    let atoms = convert_to_atoms_with_parsed_spans(
        &call_graph,
        &symbol_to_display_name,
        &project_path,
        with_locations,
        Some(&metadata.pkg_name),
    );
    println!("  ✓ Converted {} functions to atoms format", atoms.len());
    if with_locations {
        println!("    (including dependencies-with-locations)");
    }

    let duplicates = find_duplicate_code_names(&atoms);
    if !duplicates.is_empty() {
        let report = format_duplicate_report(&duplicates);
        if allow_duplicates {
            eprintln!();
            eprintln!("{}", report);
            eprintln!(
                "    Continuing because --allow-duplicates was specified.\n    \
                 Duplicate entries will be dropped (first occurrence kept)."
            );
        } else {
            eprintln!();
            eprintln!("{}", report);
            let names = duplicates
                .into_iter()
                .map(|d| d.code_name)
                .collect::<Vec<_>>();
            let count = names.len();
            return Err(ProbeError::DuplicateCodeNames { count, names });
        }
    }

    let mut atoms_dict: BTreeMap<String, AtomWithLines> = BTreeMap::new();
    for atom in atoms {
        atoms_dict.entry(atom.code_name.clone()).or_insert(atom);
    }

    if with_charon {
        enrich_with_charon(&project_path, auto_install, &mut atoms_dict);
    }

    let stub_count = add_external_stubs(&mut atoms_dict);
    if stub_count > 0 {
        println!("  ✓ Added {} external function stub(s)", stub_count);
    }
    let output = output.unwrap_or_else(|| get_default_output_path(&project_path, &metadata, ""));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProbeError::file_io(parent, e))?;
    }

    let envelope = wrap_in_envelope("probe-rust/extract", "extract", &atoms_dict, &metadata);
    let json = serde_json::to_string_pretty(&envelope)?;
    std::fs::write(&output, &json).map_err(|e| ProbeError::file_io(&output, e))?;

    print_success_summary(&output, &atoms_dict);
    Ok(())
}

/// Validate the given project path contains a `Cargo.toml`.
/// If not found at the top level, searches subdirectories (up to 2 levels deep).
/// Returns the validated project root (which may differ from the input).
fn validate_project(project_path: &Path) -> ProbeResult<PathBuf> {
    if !project_path.exists() {
        return Err(ProbeError::ProjectValidation(format!(
            "Project path does not exist: {}",
            project_path.display()
        )));
    }

    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        return Ok(project_path.to_path_buf());
    }

    let candidates = find_cargo_tomls(project_path, 2);
    match candidates.len() {
        0 => Err(ProbeError::ProjectValidation(format!(
            "Not a valid Rust project (no Cargo.toml found in {} or its subdirectories)",
            project_path.display()
        ))),
        1 => {
            let found = &candidates[0];
            println!(
                "  ℹ No Cargo.toml at top level; found one in: {}",
                found.display()
            );
            Ok(found.clone())
        }
        _ => {
            let mut msg = format!(
                "No Cargo.toml at top level of {}.\n\
                 Found {} Rust projects in subdirectories:\n",
                project_path.display(),
                candidates.len()
            );
            for c in &candidates {
                msg.push_str(&format!("    {}\n", c.display()));
            }
            msg.push_str("\nPlease specify the exact project path, e.g.:\n");
            msg.push_str(&format!(
                "    probe-rust extract {}",
                candidates[0].display()
            ));
            Err(ProbeError::ProjectValidation(msg))
        }
    }
}

/// Search for directories containing `Cargo.toml` under `root`, up to `max_depth` levels.
fn find_cargo_tomls(root: &Path, max_depth: u32) -> Vec<PathBuf> {
    let mut results = Vec::new();
    find_cargo_tomls_recursive(root, max_depth, &mut results);
    results.sort();
    results
}

fn find_cargo_tomls_recursive(dir: &Path, remaining_depth: u32, results: &mut Vec<PathBuf>) {
    if remaining_depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            if path.join("Cargo.toml").exists() {
                results.push(path.clone());
            }
            find_cargo_tomls_recursive(&path, remaining_depth - 1, results);
        }
    }
}

fn get_scip_json(cache: &mut ScipCache, regenerate: bool) -> ProbeResult<PathBuf> {
    if cache.has_cached_json() && !regenerate {
        println!(
            "  ✓ Found existing SCIP JSON at {}",
            cache.json_path().display()
        );
        println!("    (use --regenerate-scip to force regeneration)");
        println!();
        return Ok(cache.json_path());
    }

    let reason = cache.generation_reason(regenerate);
    println!("Generating SCIP index {}...", reason);
    println!("  (This may take a while for large projects)");

    let path = cache.get_or_generate(regenerate, true)?;
    println!();
    Ok(path)
}

fn format_duplicate_report(duplicates: &[crate::DuplicateCodeName]) -> String {
    let mut msg = format!(
        "WARNING: Found {} duplicate code_name(s):\n",
        duplicates.len()
    );
    for dup in duplicates {
        msg.push_str(&format!("    - '{}'\n", dup.code_name));
        for occ in &dup.occurrences {
            msg.push_str(&format!(
                "      at {}:{} ({})\n",
                occ.code_path, occ.lines_start, occ.display_name
            ));
        }
    }
    msg.push_str("\n    Duplicate code_names cannot be used as dictionary keys.\n");
    msg.push_str("    This may indicate trait implementations that cannot be distinguished.\n");
    msg.push_str("    Use --allow-duplicates to continue anyway (first occurrence kept).");
    msg
}

fn enrich_with_charon(
    project_path: &Path,
    auto_install: bool,
    atoms_dict: &mut BTreeMap<String, AtomWithLines>,
) {
    println!();
    println!("Enriching rust-qualified-names with Charon (Aeneas-compatible)...");

    let mut charon_cache = CharonCache::new(project_path).with_auto_install(auto_install);

    let regenerate = false;
    let llbc_path = match charon_cache.get_or_generate(regenerate, true) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "  ⚠ Charon enrichment skipped: {}\n    \
                 rust-qualified-name will use heuristic (file path + display name)",
                e
            );
            return;
        }
    };

    match charon_names::enrich_atoms_with_charon_names(atoms_dict, &llbc_path, true) {
        Ok(count) => {
            let total = atoms_dict.len();
            println!("  ✓ Enriched {count}/{total} atoms with Charon-derived rust-qualified-name");
        }
        Err(e) => {
            eprintln!("  ⚠ Charon LLBC parsing failed: {e}");
            eprintln!("    rust-qualified-name will use heuristic");
        }
    }
}

fn print_success_summary(output: &Path, atoms_dict: &BTreeMap<String, AtomWithLines>) {
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  ✓ SUCCESS");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Output written to: {}", output.display());
    println!();
    println!("Summary:");
    println!("  - Total functions: {}", atoms_dict.len());
    println!(
        "  - Total dependencies: {}",
        atoms_dict
            .values()
            .map(|a| a.dependencies.len())
            .sum::<usize>()
    );
    println!("  - Output format: dictionary keyed by code_name");
    println!();
}
