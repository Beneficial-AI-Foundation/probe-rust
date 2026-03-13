//! List-functions command - List all functions in a Rust project.

use probe_rust::{rust_parser, ProbeError, ProbeResult};
use std::path::PathBuf;

/// Output format for function listing.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Function names, one per line
    Text,
    /// Full JSON output with all details
    Json,
    /// Detailed text with file locations and context
    Detailed,
}

/// Execute the list-functions command.
pub fn cmd_list_functions(
    path: PathBuf,
    format: OutputFormat,
    exclude_methods: bool,
    show_visibility: bool,
    output: Option<PathBuf>,
) -> ProbeResult<()> {
    if !path.exists() {
        return Err(ProbeError::ProjectValidation(format!(
            "Path does not exist: {}",
            path.display()
        )));
    }

    let mut listing = rust_parser::list_all_functions(&path);

    if exclude_methods {
        listing.functions.retain(|f| !f.is_method);
        listing.summary.total_functions = listing.functions.len();
    }

    let actual_format = if output.is_some() {
        OutputFormat::Json
    } else {
        format
    };

    match actual_format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&listing)?;
            if let Some(output_path) = output {
                std::fs::write(&output_path, &json)
                    .map_err(|e| ProbeError::file_io(&output_path, e))?;
                eprintln!("Output written to {}", output_path.display());
            } else {
                println!("{}", json);
            }
        }
        OutputFormat::Text => {
            let mut names: Vec<&str> = listing.functions.iter().map(|f| f.name.as_str()).collect();
            names.sort();
            names.dedup();
            for name in names {
                println!("{}", name);
            }
        }
        OutputFormat::Detailed => {
            for func in &listing.functions {
                print!("{}", func.name);
                if show_visibility {
                    if let Some(ref vis) = func.visibility {
                        print!(" [{}]", vis);
                    } else {
                        print!(" [private]");
                    }
                }
                if let Some(ref file) = func.file {
                    print!(" @ {}:{}:{}", file, func.start_line, func.end_line);
                }
                if let Some(ref context) = func.context {
                    print!(" in {}", context);
                }
                println!();
            }
            println!(
                "\nSummary: {} functions in {} files",
                listing.summary.total_functions, listing.summary.total_files
            );
        }
    }

    Ok(())
}
