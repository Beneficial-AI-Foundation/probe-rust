//! Error types for probe-rust.

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for probe-rust operations.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// Error parsing SCIP index data
    #[error("SCIP parsing error: {0}")]
    ScipParse(String),

    /// Error with SCIP symbol format
    #[error("Invalid SCIP symbol format: {message}")]
    InvalidSymbol { message: String, symbol: String },

    /// File I/O error
    #[error("File I/O error for {path}: {source}")]
    FileIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Source file parsing error
    #[error("Failed to parse source file {path}: {message}")]
    SourceParse { path: PathBuf, message: String },

    /// Project validation error
    #[error("Project validation error: {0}")]
    ProjectValidation(String),

    /// Duplicate code-names detected
    #[error("Found {count} duplicate code-name(s): {names:?}")]
    DuplicateCodeNames { count: usize, names: Vec<String> },

    /// External tool error
    #[error("External tool '{tool}' error: {message}")]
    ExternalTool { tool: String, message: String },

    /// SCIP index generation/caching error
    #[error("SCIP generation error: {0}")]
    ScipGeneration(String),

    /// Charon LLBC generation error
    #[error("Charon error: {0}")]
    CharonGeneration(String),

    /// I/O error (non-path-specific)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ProbeError {
    pub fn file_io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ProbeError::FileIo {
            path: path.into(),
            source,
        }
    }

    pub fn invalid_symbol(message: impl Into<String>, symbol: impl Into<String>) -> Self {
        ProbeError::InvalidSymbol {
            message: message.into(),
            symbol: symbol.into(),
        }
    }

    pub fn source_parse(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        ProbeError::SourceParse {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn external_tool(tool: impl Into<String>, message: impl Into<String>) -> Self {
        ProbeError::ExternalTool {
            tool: tool.into(),
            message: message.into(),
        }
    }
}

impl From<crate::scip_cache::ScipError> for ProbeError {
    fn from(e: crate::scip_cache::ScipError) -> Self {
        ProbeError::ScipGeneration(e.to_string())
    }
}

impl From<crate::charon_cache::CharonError> for ProbeError {
    fn from(e: crate::charon_cache::CharonError) -> Self {
        ProbeError::CharonGeneration(e.to_string())
    }
}

impl From<crate::tool_manager::ToolError> for ProbeError {
    fn from(e: crate::tool_manager::ToolError) -> Self {
        ProbeError::ExternalTool {
            tool: match &e {
                crate::tool_manager::ToolError::PlatformNotSupported(t, _)
                | crate::tool_manager::ToolError::DownloadFailed(t, _)
                | crate::tool_manager::ToolError::DecompressFailed(t, _)
                | crate::tool_manager::ToolError::IoError(t, _)
                | crate::tool_manager::ToolError::NotInstalled(t) => t.to_string(),
            },
            message: e.to_string(),
        }
    }
}

/// Result type alias for probe-rust operations.
pub type ProbeResult<T> = Result<T, ProbeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ProbeError::ScipParse("invalid format".to_string());
        assert_eq!(err.to_string(), "SCIP parsing error: invalid format");

        let err = ProbeError::invalid_symbol("missing prefix", "bad_symbol");
        assert!(err.to_string().contains("Invalid SCIP symbol format"));

        let err = ProbeError::ProjectValidation("Cargo.toml not found".to_string());
        assert!(err.to_string().contains("Cargo.toml not found"));
    }

    #[test]
    fn test_error_from_json() {
        let json_err: Result<String, serde_json::Error> =
            serde_json::from_str::<String>("invalid json");
        let probe_err: ProbeError = json_err.unwrap_err().into();
        assert!(matches!(probe_err, ProbeError::Json(_)));
    }
}
