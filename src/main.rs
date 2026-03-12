use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;

#[derive(Parser)]
#[command(name = "probe-rust")]
#[command(about = "Generate compact function call graph data from Rust projects")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate call graph atoms from a Rust project's SCIP index
    Extract {
        /// Path to the Rust project root (must contain Cargo.toml)
        project_path: PathBuf,

        /// Output file path (default: .verilib/probes/rust_<pkg>_<ver>_atoms.json)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Force regeneration of the SCIP index even if cached
        #[arg(long)]
        regenerate_scip: bool,

        /// Include per-call location data (dependencies-with-locations)
        #[arg(long)]
        with_locations: bool,

        /// Allow duplicate code_names instead of failing
        #[arg(long)]
        allow_duplicates: bool,

        /// Automatically download missing tools (scip)
        #[arg(long)]
        auto_install: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Extract {
            project_path,
            output,
            regenerate_scip,
            with_locations,
            allow_duplicates,
            auto_install,
        } => {
            commands::cmd_extract(
                project_path,
                output,
                regenerate_scip,
                with_locations,
                allow_duplicates,
                auto_install,
            );
        }
    }
}
