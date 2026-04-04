use clap::{Parser, Subcommand};
use probe_rust::commands;
use std::path::PathBuf;
use std::process::ExitCode;

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

        /// Output file path (default: .verilib/probes/rust_<pkg>_<ver>.json)
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

        /// Automatically download missing tools (scip, and charon when --with-charon is set)
        #[arg(long)]
        auto_install: bool,

        /// Enrich atoms with Charon-derived rust-qualified-names (for Aeneas integration)
        #[arg(long)]
        with_charon: bool,
    },

    /// Find which crates a function's callees belong to
    ///
    /// Given a function and a depth N, traverses the call graph up to depth N
    /// and reports which crates the discovered callees belong to.
    #[command(name = "callee-crates")]
    CalleeCrates {
        /// Function code-name (probe:...) or display-name to search for
        function: String,

        /// Maximum traversal depth (1 = direct callees, 2 = callees of callees, etc.)
        #[arg(short, long)]
        depth: usize,

        /// Path to atoms.json file (reads from stdin if omitted)
        #[arg(short, long)]
        atoms_file: Option<PathBuf>,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Exclude standard library crates (core, alloc, std) from output
        #[arg(long)]
        exclude_stdlib: bool,

        /// Exclude specific crates from output (comma-separated list)
        #[arg(long, value_delimiter = ',')]
        exclude_crates: Vec<String>,
    },

    /// List all functions in a Rust project
    #[command(name = "list-functions")]
    ListFunctions {
        /// Path to search (file or directory)
        path: PathBuf,

        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: commands::OutputFormat,

        /// Exclude trait and impl methods
        #[arg(long)]
        exclude_methods: bool,

        /// Show function visibility (pub/private) in detailed output
        #[arg(long)]
        show_visibility: bool,

        /// Output JSON to specified file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Install or check status of external tools (rust-analyzer, scip).
    ///
    /// Resolves and installs scip into ~/.probe-rust/tools/.
    /// rust-analyzer must be installed via rustup; setup checks its status
    /// and provides installation instructions if missing.
    Setup {
        /// Show installation status instead of installing
        #[arg(long)]
        status: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Extract {
            project_path,
            output,
            regenerate_scip,
            with_locations,
            allow_duplicates,
            auto_install,
            with_charon,
        } => commands::cmd_extract(
            project_path,
            output,
            regenerate_scip,
            with_locations,
            allow_duplicates,
            auto_install,
            with_charon,
        ),
        Commands::CalleeCrates {
            function,
            depth,
            atoms_file,
            output,
            exclude_stdlib,
            exclude_crates,
        } => commands::cmd_callee_crates(
            function,
            depth,
            atoms_file,
            output,
            exclude_stdlib,
            exclude_crates,
        ),
        Commands::ListFunctions {
            path,
            format,
            exclude_methods,
            show_visibility,
            output,
        } => commands::cmd_list_functions(path, format, exclude_methods, show_visibility, output),
        Commands::Setup { status } => commands::cmd_setup(status),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
