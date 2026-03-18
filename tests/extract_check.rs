//! Integration tests that validate probe-rust extract output using probe-extract-check.

use probe_extract_check::{check_all, load_extract_json};
use std::path::Path;

/// Validate the real curve25519-dalek example JSON structurally.
///
/// This loads the shipped example output and runs structural checks
/// (envelope fields, line ranges, referential integrity).
/// Source-grounded checks are skipped since we don't have the source project.
#[test]
fn example_json_structural_check() {
    let json_path = Path::new("examples/rust_curve25519-dalek_4.1.3.json");
    let envelope =
        load_extract_json(json_path).unwrap_or_else(|e| panic!("failed to load example JSON: {e}"));

    // Structural checks only (no project path → no source validation).
    let report = check_all(&envelope, None);

    for d in report.errors() {
        eprintln!("{d}");
    }
    assert!(
        report.is_ok(),
        "structural check found {} error(s)",
        report.error_count()
    );

    // Sanity: the file should have a non-trivial number of atoms.
    assert!(
        envelope.data.len() > 10,
        "expected many atoms in curve25519-dalek, got {}",
        envelope.data.len()
    );
}

/// Validate that the example JSON has well-formed atom keys matching the probe: prefix convention.
#[test]
fn example_json_atom_keys_have_probe_prefix() {
    let json_path = Path::new("examples/rust_curve25519-dalek_4.1.3.json");
    let envelope = load_extract_json(json_path).unwrap();

    let non_probe_keys: Vec<_> = envelope
        .data
        .keys()
        .filter(|k| !k.starts_with("probe:"))
        .collect();
    assert!(
        non_probe_keys.is_empty(),
        "found atom keys without 'probe:' prefix: {:?}",
        non_probe_keys
    );
}

/// Validate that all non-stub atoms have a non-empty display-name and kind.
#[test]
fn example_json_atoms_have_required_fields() {
    let json_path = Path::new("examples/rust_curve25519-dalek_4.1.3.json");
    let envelope = load_extract_json(json_path).unwrap();

    for (key, atom) in &envelope.data {
        assert!(
            !atom.display_name.is_empty(),
            "atom {key} has empty display-name"
        );
        assert!(!atom.kind.is_empty(), "atom {key} has empty kind");
        assert!(!atom.language.is_empty(), "atom {key} has empty language");
    }
}

/// Run extraction via the library API and validate the output.
///
/// Requires `scip` and `rust-analyzer` to be installed.
#[test]
#[ignore]
fn live_extract_structural_check() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("atoms.json");

    let fixture = Path::new("../probe/probe-extract-check/tests/fixtures/rust_micro");
    if !fixture.exists() {
        panic!("rust_micro fixture not found at {}", fixture.display());
    }

    probe_rust::commands::cmd_extract(
        fixture.to_path_buf(),
        Some(output_path.clone()),
        false,
        false,
        false,
        true,
        false,
    )
    .expect("probe-rust extract failed");

    let envelope = load_extract_json(&output_path).unwrap();
    let report = check_all(&envelope, Some(fixture));

    report.print_summary();
    assert!(
        report.is_ok(),
        "live extract check found {} error(s)",
        report.error_count()
    );
}
