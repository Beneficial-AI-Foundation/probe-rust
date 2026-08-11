//! SCIP index caching and generation module.
//!
//! Handles the generation and caching of SCIP indexes from rust-analyzer.
//! SCIP generation can be slow for large projects, so caching is important.
//!
//! Tool resolution uses the tool manager: managed directory (~/.probe-rust/tools/)
//! is checked first, then PATH. If `auto_install` is enabled, scip is downloaded
//! automatically (rust-analyzer must be installed separately).

use crate::constants::{DATA_DIR, SCIP_INDEX_FILE, SCIP_INDEX_JSON_FILE};
use crate::tool_manager::{self, Tool};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;
use thiserror::Error;
use walkdir::WalkDir;

/// Error types for SCIP operations
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ScipError {
    #[error("rust-analyzer not found. {0}")]
    AnalyzerNotFound(String),

    #[error("scip not found. {0}")]
    ScipCliNotFound(String),

    #[error("rust-analyzer scip failed: {0}")]
    AnalyzerFailed(String),

    #[error("scip print failed: {0}")]
    ScipPrintFailed(String),

    #[error("index.scip not generated (rust-analyzer may have failed silently)")]
    IndexNotGenerated,

    #[error("failed to create data directory")]
    CreateDirFailed(#[source] std::io::Error),

    #[error("failed to move index.scip")]
    MoveFileFailed(#[source] std::io::Error),

    #[error("failed to write SCIP JSON")]
    WriteJsonFailed(#[source] std::io::Error),
}

/// Manager for SCIP index caching.
///
/// SCIP indexes are stored in `<project>/data/` directory:
/// - `index.scip`: Binary SCIP index from rust-analyzer
/// - `index.scip.json`: JSON representation for parsing
pub struct ScipCache {
    project_path: PathBuf,
    auto_install: bool,
    /// Resolved path to rust-analyzer binary
    analyzer_path: Option<PathBuf>,
    /// Resolved path to the scip binary
    scip_path_resolved: Option<PathBuf>,
}

impl ScipCache {
    pub fn new(project_path: impl Into<PathBuf>) -> Self {
        Self {
            project_path: project_path.into(),
            auto_install: false,
            analyzer_path: None,
            scip_path_resolved: None,
        }
    }

    /// Enable auto-install: download missing scip tool automatically.
    pub fn with_auto_install(mut self, auto_install: bool) -> Self {
        self.auto_install = auto_install;
        self
    }

    pub fn data_dir(&self) -> PathBuf {
        self.project_path.join(DATA_DIR)
    }

    pub fn scip_path(&self) -> PathBuf {
        self.data_dir().join(SCIP_INDEX_FILE)
    }

    pub fn json_path(&self) -> PathBuf {
        self.data_dir().join(SCIP_INDEX_JSON_FILE)
    }

    pub fn has_cached_json(&self) -> bool {
        self.json_path().exists()
    }

    /// Whether the cached SCIP JSON is stale: some source input (`*.rs` file,
    /// `Cargo.toml`, or directory — directories catch deletions and renames —
    /// outside `target/`, `.git/`, `.verilib/`, and the cache dir itself) was
    /// modified at or after the moment the cache was written. A stale index carries line
    /// numbers that no longer match the sources, which silently corrupts the
    /// span-map join downstream (missing `cfg`, `lines-end`) — so staleness
    /// must force regeneration, never a warning alone.
    ///
    /// The comparison is `>=`: on filesystems with coarse mtime granularity an
    /// edit in the same tick as the cache write would otherwise pass as
    /// fresh. The cost is at most one redundant regeneration. An unreadable
    /// cache mtime counts as stale; unreadable source mtimes are skipped (a
    /// weakening this check accepts — such entries cannot mark the cache
    /// stale).
    pub fn is_cache_stale(&self) -> bool {
        let Ok(cache_meta) = std::fs::metadata(self.json_path()) else {
            return true;
        };
        let Ok(cache_mtime) = cache_meta.modified() else {
            return true;
        };
        match newest_source_mtime(&self.project_path) {
            Some(newest) => newest >= cache_mtime,
            None => false,
        }
    }

    /// Get the path to the SCIP JSON, generating it if necessary.
    ///
    /// The cache is used only when it exists **and** is not stale (see
    /// [`Self::is_cache_stale`]); `regenerate` forces generation regardless.
    pub fn get_or_generate(
        &mut self,
        regenerate: bool,
        verbose: bool,
    ) -> Result<PathBuf, ScipError> {
        let json_path = self.json_path();

        if json_path.exists() && !regenerate {
            if !self.is_cache_stale() {
                return Ok(json_path);
            }
            if verbose {
                println!(
                    "  Cached SCIP data is older than the sources — regenerating {}",
                    json_path.display()
                );
            }
        }

        self.check_prerequisites()?;
        self.generate_scip_index(verbose)?;
        self.convert_to_json(verbose)?;

        Ok(json_path)
    }

    fn check_prerequisites(&mut self) -> Result<(), ScipError> {
        let analyzer_path = tool_manager::resolve_or_install(Tool::RustAnalyzer, false)
            .map_err(|e| ScipError::AnalyzerNotFound(e.to_string()))?;
        self.analyzer_path = Some(analyzer_path);

        let scip_path = tool_manager::resolve_or_install(Tool::Scip, self.auto_install)
            .map_err(|e| ScipError::ScipCliNotFound(e.to_string()))?;
        self.scip_path_resolved = Some(scip_path);

        Ok(())
    }

    fn generate_scip_index(&self, verbose: bool) -> Result<(), ScipError> {
        let analyzer_bin = self
            .analyzer_path
            .as_ref()
            .ok_or_else(|| ScipError::AnalyzerNotFound("check_prerequisites not called".into()))?;

        if verbose {
            println!(
                "Generating SCIP index for {} (using rust-analyzer)...",
                self.project_path.display(),
            );
        }

        let status = Command::new(analyzer_bin)
            .args(["scip", "."])
            .current_dir(&self.project_path)
            .stdout(if verbose {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .stderr(if verbose {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                return Err(ScipError::AnalyzerFailed(format!("exit status: {}", s)));
            }
            Err(e) => {
                return Err(ScipError::AnalyzerFailed(e.to_string()));
            }
        }

        let generated_path = self.project_path.join("index.scip");
        if !generated_path.exists() {
            return Err(ScipError::IndexNotGenerated);
        }

        let data_dir = self.data_dir();
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir).map_err(ScipError::CreateDirFailed)?;
        }

        let cached_path = self.scip_path();
        std::fs::rename(&generated_path, &cached_path).map_err(ScipError::MoveFileFailed)?;

        if verbose {
            println!("  Saved index.scip to {}", cached_path.display());
        }

        Ok(())
    }

    fn convert_to_json(&self, verbose: bool) -> Result<(), ScipError> {
        let scip_bin = self
            .scip_path_resolved
            .as_ref()
            .ok_or_else(|| ScipError::ScipCliNotFound("check_prerequisites not called".into()))?;

        if verbose {
            println!("Converting index.scip to JSON...");
        }

        let scip_index_path = self.scip_path();
        let scip_index_str = scip_index_path.to_string_lossy();
        let output = Command::new(scip_bin)
            .args(["print", "--json", scip_index_str.as_ref()])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let json_path = self.json_path();
                // Publish atomically: a crash mid-write must not leave a
                // truncated JSON behind with a fresh mtime (it would be
                // served as a valid cache until the next source edit).
                let tmp_path = json_path.with_extension("json.tmp");
                std::fs::write(&tmp_path, o.stdout).map_err(ScipError::WriteJsonFailed)?;
                std::fs::rename(&tmp_path, &json_path).map_err(ScipError::WriteJsonFailed)?;

                if verbose {
                    println!("  Saved SCIP JSON to {}", json_path.display());
                }

                Ok(())
            }
            Ok(o) => Err(ScipError::ScipPrintFailed(format!(
                "exit status: {}",
                o.status
            ))),
            Err(e) => Err(ScipError::ScipPrintFailed(e.to_string())),
        }
    }

    pub fn generation_reason(&self, regenerate: bool) -> &'static str {
        if regenerate {
            "(regeneration requested)"
        } else if self.has_cached_json() {
            "(cached SCIP data older than sources)"
        } else {
            "(no existing SCIP data found)"
        }
    }
}

/// Newest modification time among the project's source inputs: `*.rs` files,
/// `Cargo.toml`, and the directories containing them — a file deletion or
/// rename bumps no surviving file's mtime, but it does bump the parent
/// directory's. Skips `target/`, `.git/`, `.verilib/` (probe's own output —
/// counting it would invalidate the cache on every run), and the SCIP cache
/// dir itself (`<project>/data/` would otherwise make the freshly written
/// cache count as "newer sources"). `None` when nothing readable was found.
fn newest_source_mtime(project_path: &Path) -> Option<SystemTime> {
    let cache_dir = project_path.join(DATA_DIR);
    let root = project_path.to_path_buf();
    WalkDir::new(project_path)
        .into_iter()
        .filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (name == "target"
                    || name == ".git"
                    || name == ".verilib"
                    || e.path() == cache_dir))
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            // The project root's own mtime is excluded: index generation
            // creates a transient `index.scip` there (bumping the root every
            // run), which on coarse-mtime filesystems would make each fresh
            // cache immediately count as stale again. Subdirectory mtimes
            // still catch deletions everywhere below the root.
            (path.is_dir() && path != root)
                || path.extension().and_then(|x| x.to_str()) == Some("rs")
                || matches!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("Cargo.toml") | Some("Cargo.lock")
                )
        })
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scip_cache_paths() {
        let cache = ScipCache::new("/path/to/project");
        assert_eq!(cache.data_dir(), PathBuf::from("/path/to/project/data"));
        assert_eq!(
            cache.scip_path(),
            PathBuf::from("/path/to/project/data/index.scip")
        );
        assert_eq!(
            cache.json_path(),
            PathBuf::from("/path/to/project/data/index.scip.json")
        );
    }

    #[test]
    fn test_scip_error_display() {
        let err = ScipError::AnalyzerNotFound("not installed".into());
        assert!(err.to_string().contains("rust-analyzer not found"));

        let err = ScipError::ScipCliNotFound("not installed".into());
        assert!(err.to_string().contains("scip not found"));
    }

    #[test]
    fn test_scip_cache_auto_install() {
        let cache = ScipCache::new("/path/to/project").with_auto_install(true);
        assert!(cache.auto_install);

        let cache = ScipCache::new("/path/to/project").with_auto_install(false);
        assert!(!cache.auto_install);
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn set_mtime(root: &Path, rel: &str, secs_ago: u64) {
        let t = filetime::FileTime::from_system_time(
            SystemTime::now() - std::time::Duration::from_secs(secs_ago),
        );
        filetime::set_file_mtime(root.join(rel), t).unwrap();
    }

    #[test]
    fn test_missing_cache_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ScipCache::new(tmp.path());
        assert!(cache.is_cache_stale());
    }

    /// Backdate every source file and directory (the project root and `src/`)
    /// so only the entries a test explicitly touches afterwards are newer
    /// than the cache.
    fn backdate_sources(root: &Path) {
        set_mtime(root, "src", 100);
        set_mtime(root, ".", 100);
    }

    #[test]
    fn test_cache_newer_than_sources_is_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "src/lib.rs", 100);
        set_mtime(tmp.path(), "Cargo.toml", 100);
        backdate_sources(tmp.path());

        let cache = ScipCache::new(tmp.path());
        assert!(!cache.is_cache_stale());
    }

    #[test]
    fn test_file_deletion_is_stale() {
        // Deleting a source file bumps no surviving file's mtime — only its
        // parent directory's. The directory scan must catch it.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "src/gone.rs", "pub fn g() {}\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "src/lib.rs", 100);
        set_mtime(tmp.path(), "src/gone.rs", 100);
        backdate_sources(tmp.path());
        set_mtime(tmp.path(), "data/index.scip.json", 50);

        let cache = ScipCache::new(tmp.path());
        assert!(!cache.is_cache_stale(), "precondition: fresh before delete");

        std::fs::remove_file(tmp.path().join("src/gone.rs")).unwrap();
        assert!(cache.is_cache_stale(), "deletion must invalidate the cache");
    }

    #[test]
    fn test_source_newer_than_cache_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "data/index.scip.json", 100);

        let cache = ScipCache::new(tmp.path());
        assert!(cache.is_cache_stale());
    }

    #[test]
    fn test_manifest_newer_than_cache_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "data/index.scip.json", 100);

        let cache = ScipCache::new(tmp.path());
        assert!(cache.is_cache_stale());
    }

    #[test]
    fn test_target_git_and_cache_dirs_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "src/lib.rs", 100);
        // Newer than the cache, but none of these may count as sources.
        // `.verilib/` is probe's own default output location: counting it
        // would re-invalidate the cache on every extract run.
        write(tmp.path(), "target/debug/build.rs", "fn main() {}\n");
        write(tmp.path(), ".git/hook.rs", "fn main() {}\n");
        write(tmp.path(), "data/scratch.rs", "fn main() {}\n");
        write(tmp.path(), ".verilib/probes/rust_x_0.1.0.json", "{}");
        // Creating those dirs bumped the project root's mtime; backdate it so
        // the assertion isolates the pruned CONTENTS. Backdate the cache less
        // than the decoys' age so the decoys are STRICTLY newer than the
        // cache even on coarse-mtime filesystems — a broken prune must fail
        // this test.
        backdate_sources(tmp.path());
        set_mtime(tmp.path(), "target/debug/build.rs", 20);
        set_mtime(tmp.path(), ".git/hook.rs", 20);
        set_mtime(tmp.path(), "data/scratch.rs", 20);
        set_mtime(tmp.path(), ".verilib/probes/rust_x_0.1.0.json", 20);
        set_mtime(tmp.path(), "data/index.scip.json", 50);

        let cache = ScipCache::new(tmp.path());
        assert!(!cache.is_cache_stale());
    }

    #[test]
    fn test_cargo_lock_newer_than_cache_is_stale() {
        // A lockfile change can swap dependency sources, which changes what
        // rust-analyzer indexed.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "Cargo.lock", "");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "src/lib.rs", 100);
        backdate_sources(tmp.path());
        set_mtime(tmp.path(), "data/index.scip.json", 50);
        assert!(ScipCache::new(tmp.path()).is_cache_stale());

        set_mtime(tmp.path(), "Cargo.lock", 100);
        assert!(!ScipCache::new(tmp.path()).is_cache_stale());
    }

    #[test]
    fn test_project_root_mtime_is_ignored() {
        // Index generation creates a transient file at the project root
        // every run; if the root's own mtime counted, each fresh cache would
        // immediately be stale again on coarse-mtime filesystems.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "pub fn f() {}\n");
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "src/lib.rs", 100);
        set_mtime(tmp.path(), "src", 100);
        set_mtime(tmp.path(), "data/index.scip.json", 50);
        // Root deliberately left at "now" (newer than the cache).
        assert!(!ScipCache::new(tmp.path()).is_cache_stale());
    }

    #[test]
    fn test_generation_reason_arms() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ScipCache::new(tmp.path());
        assert_eq!(cache.generation_reason(true), "(regeneration requested)");
        assert_eq!(
            cache.generation_reason(false),
            "(no existing SCIP data found)"
        );
        write(tmp.path(), "data/index.scip.json", "{}");
        assert_eq!(
            cache.generation_reason(false),
            "(cached SCIP data older than sources)"
        );
    }

    #[test]
    fn test_nested_data_dir_is_not_ignored() {
        // Only the top-level cache dir is exempt; a source directory that
        // happens to be called `data/` must still be able to mark the cache
        // stale.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "data/index.scip.json", "{}");
        set_mtime(tmp.path(), "data/index.scip.json", 100);
        write(tmp.path(), "src/data/tables.rs", "pub const T: u8 = 0;\n");

        let cache = ScipCache::new(tmp.path());
        assert!(cache.is_cache_stale());
    }
}
