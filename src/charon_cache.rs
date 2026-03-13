//! Charon LLBC generation and caching module.
//!
//! Manages running charon on a Rust project and caching the resulting LLBC file.
//! LLBC files contain Charon's structured representation of the crate, including
//! accurate item names that match what Aeneas uses for Lean translation.
//!
//! Tool resolution uses the tool manager: managed directory (~/.probe-rust/tools/)
//! is checked first, then PATH. If `auto_install` is enabled, charon is built
//! from source automatically.

use crate::constants::DATA_DIR;
use crate::tool_manager::{self, Tool};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const LLBC_FILE: &str = "charon.llbc";

#[derive(Debug)]
pub enum CharonError {
    CharonNotFound(String),
    CharonFailed(String),
    LlbcNotGenerated,
    CreateDirFailed(std::io::Error),
}

impl std::fmt::Display for CharonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharonError::CharonNotFound(detail) => {
                write!(f, "charon not found. {detail}")
            }
            CharonError::CharonFailed(msg) => {
                write!(f, "charon failed: {msg}")
            }
            CharonError::LlbcNotGenerated => {
                write!(f, "LLBC file not generated (charon may have failed)")
            }
            CharonError::CreateDirFailed(e) => {
                write!(f, "failed to create data directory: {e}")
            }
        }
    }
}

impl std::error::Error for CharonError {}

/// Manager for Charon LLBC generation and caching.
///
/// LLBC files are stored in `<project>/data/charon.llbc`.
pub struct CharonCache {
    project_path: PathBuf,
    auto_install: bool,
    charon_path: Option<PathBuf>,
}

impl CharonCache {
    pub fn new(project_path: impl Into<PathBuf>) -> Self {
        Self {
            project_path: project_path.into(),
            auto_install: false,
            charon_path: None,
        }
    }

    pub fn with_auto_install(mut self, auto_install: bool) -> Self {
        self.auto_install = auto_install;
        self
    }

    pub fn data_dir(&self) -> PathBuf {
        self.project_path.join(DATA_DIR)
    }

    pub fn llbc_path(&self) -> PathBuf {
        self.data_dir().join(LLBC_FILE)
    }

    pub fn has_cached_llbc(&self) -> bool {
        self.llbc_path().exists()
    }

    /// Get the path to the LLBC file, generating it if necessary.
    pub fn get_or_generate(
        &mut self,
        regenerate: bool,
        verbose: bool,
    ) -> Result<PathBuf, CharonError> {
        let llbc_path = self.llbc_path();

        if llbc_path.exists() && !regenerate {
            return Ok(llbc_path);
        }

        self.check_prerequisites()?;
        self.run_charon(verbose)?;

        Ok(llbc_path)
    }

    fn check_prerequisites(&mut self) -> Result<(), CharonError> {
        let charon_path = tool_manager::resolve_or_install(Tool::Charon, self.auto_install)
            .map_err(|e| CharonError::CharonNotFound(e.to_string()))?;
        self.charon_path = Some(charon_path);

        // charon-driver must also be available (charon invokes it)
        tool_manager::resolve_or_install(Tool::CharonDriver, self.auto_install)
            .map_err(|e| CharonError::CharonNotFound(e.to_string()))?;

        Ok(())
    }

    fn run_charon(&self, verbose: bool) -> Result<(), CharonError> {
        let charon_bin = self
            .charon_path
            .as_ref()
            .expect("check_prerequisites must be called first");

        let data_dir = self.data_dir();
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir).map_err(CharonError::CreateDirFailed)?;
        }

        let dest_file = self.llbc_path();

        if verbose {
            eprintln!("Running charon on {} ...", self.project_path.display(),);
        }

        // charon-driver needs to be on PATH or in the same directory as charon.
        // We add the managed tools directory to PATH so charon can find charon-driver.
        let mut path_env = std::env::var("PATH").unwrap_or_default();
        if let Some(parent) = charon_bin.parent() {
            path_env = format!("{}:{}", parent.display(), path_env);
        }

        let dest_file_str = dest_file.to_string_lossy();
        let output = Command::new(charon_bin)
            .args([
                "cargo",
                "--preset",
                "aeneas",
                "--dest-file",
                dest_file_str.as_ref(),
                "--no-dedup-serialized-ast",
            ])
            .current_dir(&self.project_path)
            .env("PATH", &path_env)
            .stdout(if verbose {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .stderr(if verbose {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .output();

        match output {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                return Err(CharonError::CharonFailed(format!(
                    "exit status: {} stderr: {}",
                    o.status,
                    stderr.chars().take(500).collect::<String>()
                )));
            }
            Err(e) => {
                return Err(CharonError::CharonFailed(e.to_string()));
            }
        }

        if !dest_file.exists() {
            return Err(CharonError::LlbcNotGenerated);
        }

        if verbose {
            eprintln!("  Saved LLBC to {}", dest_file.display());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_charon_cache_paths() {
        let cache = CharonCache::new("/path/to/project");
        assert_eq!(cache.data_dir(), PathBuf::from("/path/to/project/data"));
        assert_eq!(
            cache.llbc_path(),
            PathBuf::from("/path/to/project/data/charon.llbc")
        );
    }

    #[test]
    fn test_charon_error_display() {
        let err = CharonError::CharonNotFound("not installed".into());
        assert!(err.to_string().contains("charon not found"));
    }

    #[test]
    fn test_charon_cache_auto_install() {
        let cache = CharonCache::new("/path/to/project").with_auto_install(true);
        assert!(cache.auto_install);

        let cache = CharonCache::new("/path/to/project").with_auto_install(false);
        assert!(!cache.auto_install);
    }
}
