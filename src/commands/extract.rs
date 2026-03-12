//! Extract command - Generate call graph atoms from SCIP indexes.

use probe_rust::{
    add_external_stubs, build_call_graph, convert_to_atoms_with_parsed_spans,
    find_duplicate_code_names,
    metadata::{gather_metadata, get_default_output_path, wrap_in_envelope},
    parse_scip_json,
    scip_cache::ScipCache,
    AtomWithLines,
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
) {
    println!("═══════════════════════════════════════════════════════════");
    println!("  Probe Rust - Extract: Generate Call Graph Data");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    if let Err(msg) = validate_project(&project_path) {
        eprintln!("✗ Error: {}", msg);
        std::process::exit(1);
    }
    println!("  ✓ Valid Rust project found");

    let mut scip_cache = ScipCache::new(&project_path).with_auto_install(auto_install);
    let json_path = get_scip_json(&mut scip_cache, regenerate_scip);

    println!("Parsing SCIP JSON and building call graph...");

    let scip_index = match parse_scip_json(json_path.to_str().unwrap()) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("✗ Failed to parse SCIP JSON: {}", e);
            std::process::exit(1);
        }
    };

    let (call_graph, symbol_to_display_name) = build_call_graph(&scip_index);
    println!("  ✓ Call graph built with {} functions", call_graph.len());
    println!();

    println!("Converting to atoms format with accurate line numbers...");
    println!("  Parsing source files with syn for accurate function spans...");

    let atoms = convert_to_atoms_with_parsed_spans(
        &call_graph,
        &symbol_to_display_name,
        &project_path,
        with_locations,
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
            std::process::exit(1);
        }
    }

    let mut atoms_dict: BTreeMap<String, AtomWithLines> = BTreeMap::new();
    for atom in atoms {
        atoms_dict.entry(atom.code_name.clone()).or_insert(atom);
    }

    let stub_count = add_external_stubs(&mut atoms_dict);
    if stub_count > 0 {
        println!("  ✓ Added {} external function stub(s)", stub_count);
    }

    let metadata = gather_metadata(&project_path);
    let output =
        output.unwrap_or_else(|| get_default_output_path(&project_path, &metadata, "atoms"));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create output directory");
    }

    let envelope = wrap_in_envelope("probe-rust/atoms", "extract", &atoms_dict, &metadata);
    let json = serde_json::to_string_pretty(&envelope).expect("Failed to serialize JSON");
    std::fs::write(&output, &json).expect("Failed to write output file");

    print_success_summary(&output, &atoms_dict);
}

fn validate_project(project_path: &Path) -> Result<(), String> {
    if !project_path.exists() {
        return Err(format!(
            "Project path does not exist: {}",
            project_path.display()
        ));
    }

    let cargo_toml = project_path.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(format!(
            "Not a valid Rust project (Cargo.toml not found): {}",
            project_path.display()
        ));
    }

    Ok(())
}

fn get_scip_json(cache: &mut ScipCache, regenerate: bool) -> PathBuf {
    if cache.has_cached_json() && !regenerate {
        println!(
            "  ✓ Found existing SCIP JSON at {}",
            cache.json_path().display()
        );
        println!("    (use --regenerate-scip to force regeneration)");
        println!();
        return cache.json_path();
    }

    let reason = cache.generation_reason(regenerate);
    println!("Generating SCIP index {}...", reason);
    println!("  (This may take a while for large projects)");

    match cache.get_or_generate(regenerate, true) {
        Ok(path) => {
            println!();
            path
        }
        Err(e) => {
            eprintln!("✗ Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn format_duplicate_report(duplicates: &[probe_rust::DuplicateCodeName]) -> String {
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
