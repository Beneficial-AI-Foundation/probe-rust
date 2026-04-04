//! `setup` subcommand: install and manage external tool dependencies.

use crate::tool_manager;
use crate::ProbeResult;

/// Entry point for the `setup` subcommand.
pub fn cmd_setup(status: bool) -> ProbeResult<()> {
    if status {
        tool_manager::print_status();
        return Ok(());
    }

    eprintln!("Installing external tools for probe-rust...\n");

    let (errors, warnings) = tool_manager::install_all();

    for w in &warnings {
        eprintln!("Warning: {w}");
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("Error: {e}");
        }
        eprintln!(
            "\n{} tool(s) failed to install. See errors above.",
            errors.len()
        );
        std::process::exit(1);
    }

    if warnings.is_empty() {
        eprintln!("\nAll tools installed successfully.");
    } else {
        eprintln!("\nSetup finished with warnings. See above.");
    }
    tool_manager::print_status();
    Ok(())
}
