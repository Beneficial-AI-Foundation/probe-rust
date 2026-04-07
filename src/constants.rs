//! Shared constants for probe-rust.
//!
//! This module centralizes magic numbers and configuration values
//! to improve readability and maintainability.

use std::collections::HashSet;

// =============================================================================
// SCIP Symbol Kinds
// =============================================================================

/// SCIP kind for method definitions (instance methods)
pub const SCIP_KIND_METHOD: i32 = 6;

/// SCIP kind for function definitions
pub const SCIP_KIND_FUNCTION: i32 = 17;

/// SCIP kind for constructor definitions
pub const SCIP_KIND_CONSTRUCTOR: i32 = 26;

/// SCIP kind for macro definitions (used by rust-analyzer for some functions)
pub const SCIP_KIND_MACRO: i32 = 80;

/// SCIP kind for module definitions (used for module-chain visibility walk)
pub const SCIP_KIND_MODULE: i32 = 29;

/// Check if a SCIP symbol kind represents a function-like entity.
#[inline]
pub fn is_function_like_kind(kind: i32) -> bool {
    matches!(
        kind,
        SCIP_KIND_METHOD | SCIP_KIND_FUNCTION | SCIP_KIND_CONSTRUCTOR | SCIP_KIND_MACRO
    )
}

// =============================================================================
// SCIP Symbol Roles
// =============================================================================

/// Symbol role bit indicating this occurrence is a definition
pub const SYMBOL_ROLE_DEFINITION: i32 = 1;

/// Check if a symbol_roles value indicates a definition.
#[inline]
pub fn is_definition(symbol_roles: Option<i32>) -> bool {
    symbol_roles.unwrap_or(0) & SYMBOL_ROLE_DEFINITION != 0
}

// =============================================================================
// Matching Tolerances
// =============================================================================

/// Line number tolerance for matching functions between different tools.
pub const LINE_TOLERANCE: usize = 5;

/// Number of lines to look back from a definition for type context.
pub const TYPE_CONTEXT_LOOKBACK_LINES: i32 = 5;

// =============================================================================
// Cache Configuration
// =============================================================================

/// Directory name for cached data within a project
pub const DATA_DIR: &str = "data";

/// Filename for the SCIP index binary
pub const SCIP_INDEX_FILE: &str = "index.scip";

/// Filename for the SCIP index JSON
pub const SCIP_INDEX_JSON_FILE: &str = "index.scip.json";

// =============================================================================
// SCIP Symbol Prefixes
// =============================================================================

/// Expected prefix for SCIP symbols from rust-analyzer
pub const SCIP_SYMBOL_PREFIX: &str = "rust-analyzer cargo ";

/// Suffix pattern that identifies function/method symbols in SCIP notation.
pub const SCIP_FUNCTION_SUFFIX: &str = "().";

/// Check if a SCIP symbol string represents an external function reference.
#[inline]
pub fn is_external_function_symbol(symbol: &str, known_symbols: &HashSet<String>) -> bool {
    !known_symbols.contains(symbol)
        && symbol.starts_with(SCIP_SYMBOL_PREFIX)
        && symbol.ends_with(SCIP_FUNCTION_SUFFIX)
}

/// Prefix for probe-style URIs
pub const PROBE_URI_PREFIX: &str = "probe:";
