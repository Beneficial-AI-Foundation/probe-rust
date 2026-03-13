use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;

pub mod charon_cache;
pub mod charon_names;
pub mod constants;
pub mod error;
pub mod metadata;
pub mod path_utils;
pub mod rust_parser;
pub mod scip_cache;
pub mod tool_manager;

pub use error::{ProbeError, ProbeResult};

use constants::{
    is_definition, is_external_function_symbol, is_function_like_kind, PROBE_URI_PREFIX,
    SCIP_SYMBOL_PREFIX, TYPE_CONTEXT_LOOKBACK_LINES,
};

// =============================================================================
// Declaration Kind Enum
// =============================================================================

/// Declaration kind. For standard Rust this is always `Exec`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DeclKind {
    #[default]
    Exec,
}

impl DeclKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeclKind::Exec => "exec",
        }
    }
}

impl fmt::Display for DeclKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// SCIP data structures
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ScipIndex {
    pub metadata: ScipMetadata,
    pub documents: Vec<Document>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScipMetadata {
    pub tool_info: ScipToolInfo,
    pub project_root: String,
    pub text_document_encoding: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScipToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub language: String,
    pub relative_path: String,
    pub occurrences: Vec<Occurrence>,
    #[serde(default)]
    pub symbols: Vec<Symbol>,
    pub position_encoding: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Occurrence {
    pub range: Vec<i32>,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_roles: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Symbol {
    pub symbol: String,
    pub kind: i32,
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<Vec<String>>,
    pub signature_documentation: SignatureDocumentation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosing_symbol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureDocumentation {
    pub language: String,
    pub text: String,
    pub position_encoding: i32,
}

// =============================================================================
// Call graph types
// =============================================================================

/// A call from one function to another, with optional type context for disambiguation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CalleeInfo {
    pub symbol: String,
    pub type_hints: Vec<String>,
    pub line: i32,
}

/// Location where a function call occurs (always Inner for standard Rust)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum CallLocation {
    Inner,
}

/// A dependency with its call location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyWithLocation {
    #[serde(rename = "code-name")]
    pub code_name: String,
    pub location: CallLocation,
    pub line: usize,
}

/// Function node in the call graph
#[derive(Debug, Clone)]
pub struct FunctionNode {
    pub symbol: String,
    pub display_name: String,
    pub signature_text: String,
    pub relative_path: String,
    pub callees: HashSet<CalleeInfo>,
    pub range: Vec<i32>,
    pub self_type: Option<String>,
    pub definition_type_context: Vec<String>,
}

fn default_language() -> String {
    "rust".to_string()
}

/// Output format: Atom with line numbers
#[derive(Debug, Serialize, Deserialize)]
pub struct AtomWithLines {
    #[serde(rename = "display-name")]
    pub display_name: String,
    #[serde(skip_serializing, default)]
    pub code_name: String,
    pub dependencies: BTreeSet<String>,
    #[serde(
        rename = "dependencies-with-locations",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub dependencies_with_locations: Vec<DependencyWithLocation>,
    #[serde(rename = "code-module")]
    pub code_module: String,
    #[serde(rename = "code-path")]
    pub code_path: String,
    #[serde(rename = "code-text")]
    pub code_text: CodeTextInfo,
    pub kind: DeclKind,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(
        rename = "rust-qualified-name",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub rust_qualified_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTextInfo {
    #[serde(rename = "lines-start")]
    pub lines_start: usize,
    #[serde(rename = "lines-end")]
    pub lines_end: usize,
}

/// Parse a SCIP JSON file
#[must_use = "parsing result should be checked"]
pub fn parse_scip_json(file_path: &Path) -> ProbeResult<ScipIndex> {
    let file = std::fs::File::open(file_path).map_err(|e| ProbeError::file_io(file_path, e))?;
    let reader = std::io::BufReader::new(file);
    let index: ScipIndex = serde_json::from_reader(reader)?;
    Ok(index)
}

fn make_unique_key(
    symbol: &str,
    signature: &str,
    self_type: Option<&str>,
    line: Option<i32>,
) -> String {
    let base = match self_type {
        Some(st) => format!("{}|{}|{}", symbol, signature, st),
        None => format!("{}|{}", symbol, signature),
    };
    match line {
        Some(l) => format!("{}@{}", base, l),
        None => base,
    }
}

/// Derive a Rust-style qualified name from the code-path (file) and SCIP symbol.
///
/// When `pkg_name` is provided it is used as the crate prefix for bare `src/`
/// paths (i.e. SCIP indexes generated from within a single crate rather than a
/// workspace root).
#[must_use]
pub fn derive_rust_qualified_name(
    code_path: &str,
    display_name: &str,
    pkg_name: Option<&str>,
) -> Option<String> {
    if code_path.is_empty() {
        return None;
    }

    // SCIP relative paths come in two forms:
    //   1. "crate-name/src/lib.rs" (multi-crate workspace or external dep)
    //   2. "src/lib.rs" (single crate, rust-analyzer default)
    let (crate_name, file_path) = if let Some(pos) = code_path.find("/src/") {
        let crate_part = &code_path[..pos];
        let crate_name = crate_part
            .rsplit('/')
            .next()
            .unwrap_or(crate_part)
            .replace('-', "_");
        (crate_name, &code_path[pos + 5..])
    } else if let Some(rest) = code_path.strip_prefix("src/") {
        let crate_name = pkg_name
            .filter(|n| !n.is_empty())
            .map(|n| n.replace('-', "_"))
            .unwrap_or_default();
        (crate_name, rest)
    } else {
        return None;
    };

    let module_path = file_path
        .trim_end_matches(".rs")
        .trim_end_matches("/mod")
        .replace('/', "::");

    if crate_name.is_empty() {
        if module_path.is_empty() || module_path == "lib" {
            Some(display_name.to_string())
        } else {
            Some(format!("{}::{}", module_path, display_name))
        }
    } else if module_path.is_empty() || module_path == "lib" {
        Some(format!("{}::{}", crate_name, display_name))
    } else {
        Some(format!("{}::{}::{}", crate_name, module_path, display_name))
    }
}

/// Extract the bare function/method name from a possibly qualified display name.
///
/// E.g. `"EdwardsPoint::eq"` -> `"eq"`, `"simple_func"` -> `"simple_func"`.
#[must_use]
pub fn bare_function_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// For impl methods, prepend the Self type to produce "Type::method" display names.
fn enrich_display_name(scip_symbol: &str, base_display_name: &str) -> String {
    let s = scip_symbol
        .strip_prefix(SCIP_SYMBOL_PREFIX)
        .unwrap_or(scip_symbol);
    let parts: Vec<&str> = s.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return base_display_name.to_string();
    }
    let path_part = parts[2].trim_end_matches('.');
    let last_segment = path_part.rsplit('/').next().unwrap_or(path_part);
    if let Some(hash_pos) = last_segment.find('#') {
        let self_type = &last_segment[..hash_pos];
        let self_type = self_type.strip_prefix('&').unwrap_or(self_type);
        if !self_type.is_empty() {
            return format!("{}::{}", self_type, base_display_name);
        }
    }
    base_display_name.to_string()
}

/// Extract the base function/method name from a raw SCIP symbol.
fn extract_function_name_from_symbol(symbol: &str) -> String {
    let s = symbol.strip_prefix(SCIP_SYMBOL_PREFIX).unwrap_or(symbol);
    let without_suffix = s.strip_suffix("().").unwrap_or(s);
    without_suffix
        .rsplit_once('#')
        .map(|(_, n)| n)
        .or_else(|| without_suffix.rsplit_once('/').map(|(_, n)| n))
        .unwrap_or(without_suffix)
        .to_string()
}

/// Collect all symbol definition locations from SCIP data, sorted by line number.
fn collect_symbol_definitions(scip_data: &ScipIndex) -> HashMap<String, Vec<(String, i32)>> {
    let mut symbol_to_definitions: HashMap<String, Vec<(String, i32)>> = HashMap::new();
    for doc in &scip_data.documents {
        let rel_path = doc.relative_path.trim_start_matches('/').to_string();
        for occurrence in &doc.occurrences {
            if is_definition(occurrence.symbol_roles) && !occurrence.range.is_empty() {
                let line = occurrence.range[0];
                symbol_to_definitions
                    .entry(occurrence.symbol.clone())
                    .or_default()
                    .push((rel_path.clone(), line));
            }
        }
    }
    for defs in symbol_to_definitions.values_mut() {
        defs.sort_by_key(|(_, line)| *line);
    }
    symbol_to_definitions
}

/// Collect type context (nearby type references) for each definition site.
fn collect_definition_type_contexts(scip_data: &ScipIndex) -> HashMap<(String, i32), Vec<String>> {
    let mut contexts: HashMap<(String, i32), Vec<String>> = HashMap::new();
    for doc in &scip_data.documents {
        let rel_path = doc.relative_path.trim_start_matches('/').to_string();

        let mut type_refs_by_line: HashMap<i32, Vec<String>> = HashMap::new();
        for occ in &doc.occurrences {
            if !is_definition(occ.symbol_roles)
                && !occ.range.is_empty()
                && occ.symbol.ends_with('#')
            {
                let line = occ.range[0];
                if let Some(type_name) = extract_type_name_from_symbol(&occ.symbol) {
                    type_refs_by_line.entry(line).or_default().push(type_name);
                }
            }
        }

        for occ in &doc.occurrences {
            if is_definition(occ.symbol_roles) && !occ.range.is_empty() {
                let def_line = occ.range[0];
                let mut nearby_types = Vec::new();

                for offset in 0..=TYPE_CONTEXT_LOOKBACK_LINES {
                    let check_line = def_line - offset;
                    if check_line >= 0 {
                        if let Some(types) = type_refs_by_line.get(&check_line) {
                            for t in types {
                                if !nearby_types.contains(t) {
                                    nearby_types.push(t.clone());
                                }
                            }
                        }
                    }
                }

                if !nearby_types.is_empty() {
                    contexts.insert((rel_path.clone(), def_line), nearby_types);
                }
            }
        }
    }
    contexts
}

/// Collect self-type information from `method().(self)` symbol entries.
fn collect_self_types(scip_data: &ScipIndex) -> HashMap<String, Vec<String>> {
    let mut enclosing_to_self_types: HashMap<String, Vec<String>> = HashMap::new();
    for doc in &scip_data.documents {
        for symbol in &doc.symbols {
            if let Some(ref display_name) = symbol.display_name {
                if display_name == "self" {
                    if let Some(ref enclosing) = symbol.enclosing_symbol {
                        let self_sig = &symbol.signature_documentation.text;
                        if let Some(self_type) = extract_self_type(self_sig) {
                            enclosing_to_self_types
                                .entry(enclosing.clone())
                                .or_default()
                                .push(self_type);
                        }
                    }
                }
            }
        }
    }
    enclosing_to_self_types
}

/// Build a call graph from SCIP data.
#[must_use]
pub fn build_call_graph(
    scip_data: &ScipIndex,
) -> (HashMap<String, FunctionNode>, HashMap<String, String>) {
    let mut call_graph: HashMap<String, FunctionNode> = HashMap::new();
    let mut all_function_symbols: HashSet<String> = HashSet::new();
    let mut symbol_to_display_name: HashMap<String, String> = HashMap::new();

    let symbol_to_definitions = collect_symbol_definitions(scip_data);
    let definition_type_contexts = collect_definition_type_contexts(scip_data);
    let enclosing_to_self_types = collect_self_types(scip_data);

    let mut symbol_self_type_idx: HashMap<String, usize> = HashMap::new();
    let mut symbol_seen_count: HashMap<String, usize> = HashMap::new();
    let mut symbol_line_to_key: HashMap<(String, i32), String> = HashMap::new();

    // Register all function symbols, build call graph nodes, and build the
    // symbol_line_to_key lookup in a single pass over the SCIP symbol tables.
    for doc in &scip_data.documents {
        for symbol in &doc.symbols {
            if is_function_like_kind(symbol.kind) {
                let signature = &symbol.signature_documentation.text;
                let base_display_name = symbol
                    .display_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let display_name = enrich_display_name(&symbol.symbol, &base_display_name);

                all_function_symbols.insert(symbol.symbol.clone());
                symbol_to_display_name.insert(symbol.symbol.clone(), display_name.clone());

                let def_index = *symbol_seen_count.get(&symbol.symbol).unwrap_or(&0);
                symbol_seen_count
                    .entry(symbol.symbol.clone())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);

                let self_type =
                    if let Some(self_types) = enclosing_to_self_types.get(&symbol.symbol) {
                        let idx = *symbol_self_type_idx.get(&symbol.symbol).unwrap_or(&0);
                        symbol_self_type_idx
                            .entry(symbol.symbol.clone())
                            .and_modify(|i| *i += 1)
                            .or_insert(1);
                        self_types.get(idx).cloned()
                    } else {
                        None
                    };

                if let Some(defs) = symbol_to_definitions.get(&symbol.symbol) {
                    if let Some((rel_path, line)) = defs.get(def_index) {
                        let unique_key = make_unique_key(
                            &symbol.symbol,
                            signature,
                            self_type.as_deref(),
                            Some(*line),
                        );

                        symbol_line_to_key
                            .insert((symbol.symbol.clone(), *line), unique_key.clone());

                        let def_type_context = definition_type_contexts
                            .get(&(rel_path.clone(), *line))
                            .cloned()
                            .unwrap_or_default();

                        call_graph.insert(
                            unique_key,
                            FunctionNode {
                                symbol: symbol.symbol.clone(),
                                display_name,
                                signature_text: signature.clone(),
                                relative_path: rel_path.clone(),
                                callees: HashSet::new(),
                                range: Vec::new(),
                                self_type,
                                definition_type_context: def_type_context,
                            },
                        );
                    }
                }
            }
        }
    }

    populate_call_relationships(
        scip_data,
        &symbol_line_to_key,
        &mut call_graph,
        &mut all_function_symbols,
        &mut symbol_to_display_name,
    );

    (call_graph, symbol_to_display_name)
}

/// Walk through occurrences to assign ranges and connect callers to callees.
fn populate_call_relationships(
    scip_data: &ScipIndex,
    symbol_line_to_key: &HashMap<(String, i32), String>,
    call_graph: &mut HashMap<String, FunctionNode>,
    all_function_symbols: &mut HashSet<String>,
    symbol_to_display_name: &mut HashMap<String, String>,
) {
    for doc in &scip_data.documents {
        let mut current_function_key: Option<String> = None;

        let mut ordered_occurrences = doc.occurrences.clone();
        ordered_occurrences.sort_by(|a, b| {
            let a_start = (
                a.range.first().copied().unwrap_or(i32::MAX),
                a.range.get(1).copied().unwrap_or(i32::MAX),
            );
            let b_start = (
                b.range.first().copied().unwrap_or(i32::MAX),
                b.range.get(1).copied().unwrap_or(i32::MAX),
            );
            a_start.cmp(&b_start)
        });

        let mut line_to_type_hints: HashMap<i32, Vec<String>> = HashMap::new();
        for occ in &ordered_occurrences {
            if !is_definition(occ.symbol_roles) && !occ.range.is_empty() {
                let line = occ.range[0];
                if occ.symbol.ends_with('#') {
                    if let Some(type_name) = extract_type_name_from_symbol(&occ.symbol) {
                        line_to_type_hints.entry(line).or_default().push(type_name);
                    }
                }
            }
        }

        for occurrence in &ordered_occurrences {
            let is_def = is_definition(occurrence.symbol_roles);
            let line = if !occurrence.range.is_empty() {
                occurrence.range[0]
            } else {
                -1
            };

            if is_def {
                if let Some(key) = symbol_line_to_key.get(&(occurrence.symbol.clone(), line)) {
                    current_function_key = Some(key.clone());
                    if let Some(node) = call_graph.get_mut(key) {
                        node.range = occurrence.range.clone();
                    }
                }
            }

            if !is_def
                && (all_function_symbols.contains(&occurrence.symbol)
                    || is_external_function_symbol(&occurrence.symbol, all_function_symbols))
            {
                if all_function_symbols.insert(occurrence.symbol.clone()) {
                    let base_name = extract_function_name_from_symbol(&occurrence.symbol);
                    let enriched = enrich_display_name(&occurrence.symbol, &base_name);
                    symbol_to_display_name.insert(occurrence.symbol.clone(), enriched);
                }

                if let Some(caller_key) = &current_function_key {
                    if let Some(caller_node) = call_graph.get_mut(caller_key) {
                        if caller_node.symbol != occurrence.symbol {
                            let type_hints =
                                line_to_type_hints.get(&line).cloned().unwrap_or_default();
                            caller_node.callees.insert(CalleeInfo {
                                symbol: occurrence.symbol.clone(),
                                type_hints,
                                line,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn extract_type_name_from_symbol(symbol: &str) -> Option<String> {
    let without_hash = symbol.trim_end_matches('#');
    if let Some(last_slash) = without_hash.rfind('/') {
        let name = &without_hash[last_slash + 1..];
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn extract_impl_type_info(signature: &str) -> Option<String> {
    let signature = signature.trim();

    let params_start = signature.find('(')?;
    let params_end = signature.find(')')?;
    if params_start >= params_end {
        return None;
    }
    let params = &signature[params_start + 1..params_end];

    let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();

    if parts.len() >= 2 {
        let second_param = parts[1];
        if let Some(type_str) = extract_type_from_param(second_param) {
            return Some(type_str);
        }
    }

    if parts.len() == 1 {
        let first_param = parts[0].trim();
        if !first_param.is_empty() && !first_param.starts_with("self") && first_param.contains(':')
        {
            if let Some(type_str) = extract_type_from_param(first_param) {
                return Some(type_str);
            }
        }
    }

    if let Some(arrow_pos) = signature.find("->") {
        let return_type = signature[arrow_pos + 2..].trim();
        let clean_return = clean_type_string(return_type);
        if !clean_return.is_empty() && clean_return != "Self" {
            return Some(clean_return);
        }
    }

    None
}

fn extract_type_from_param(param: &str) -> Option<String> {
    let colon_pos = param.find(':')?;
    let type_part = param[colon_pos + 1..].trim();
    let clean = clean_type_string_preserve_ref(type_part);
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

/// Strip a leading lifetime annotation (e.g. `'a `, `'static `, `'_ `) from a type string.
fn strip_lifetime(s: &str) -> &str {
    if s.starts_with('\'') {
        s.find(' ').map(|i| &s[i + 1..]).unwrap_or(s)
    } else {
        s
    }
}

fn clean_type_string_preserve_ref(type_str: &str) -> String {
    let type_str = type_str.trim();
    let is_ref = type_str.starts_with('&');
    let without_ref = type_str.trim_start_matches('&').trim();
    let clean = strip_lifetime(without_ref)
        .trim_start_matches("mut ")
        .trim();

    if clean.is_empty() {
        String::new()
    } else if is_ref {
        format!("&{}", clean)
    } else {
        clean.to_string()
    }
}

fn clean_type_string(type_str: &str) -> String {
    let without_ref = type_str.trim().trim_start_matches('&');
    strip_lifetime(without_ref)
        .trim_start_matches("mut ")
        .trim()
        .to_string()
}

fn extract_self_type(self_signature: &str) -> Option<String> {
    let self_signature = self_signature.trim();

    if let Some(colon_pos) = self_signature.find(':') {
        let type_part = self_signature[colon_pos + 1..].trim();
        let is_ref = type_part.starts_with('&');
        let clean_type = strip_lifetime(type_part.trim_start_matches('&').trim())
            .trim_start_matches("mut ")
            .trim();

        if !clean_type.is_empty() {
            if is_ref {
                return Some(format!("&{}", clean_type));
            } else {
                return Some(clean_type.to_string());
            }
        }
    }

    None
}

fn needs_self_type_enrichment(symbol: &str) -> bool {
    let hash_count = symbol.matches('#').count();
    hash_count == 1
}

fn extract_code_module(probe_name: &str) -> String {
    let s = probe_name
        .strip_prefix(PROBE_URI_PREFIX)
        .unwrap_or(probe_name);

    let hash_pos = s.find('#').unwrap_or(s.len());
    let before_hash = &s[..hash_pos];

    let slashes: Vec<usize> = before_hash.match_indices('/').map(|(i, _)| i).collect();

    if slashes.len() < 3 {
        return String::new();
    }

    let start = slashes[1] + 1;
    let end = slashes[slashes.len() - 1];

    if start < end {
        before_hash[start..end].to_string()
    } else {
        String::new()
    }
}

fn symbol_to_code_name(
    symbol: &str,
    display_name: &str,
    signature: Option<&str>,
    self_type: Option<&str>,
) -> String {
    symbol_to_code_name_with_line(symbol, display_name, signature, self_type, None)
}

fn symbol_to_code_name_with_line(
    symbol: &str,
    display_name: &str,
    signature: Option<&str>,
    self_type: Option<&str>,
    line_number: Option<usize>,
) -> String {
    symbol_to_code_name_full(
        symbol,
        display_name,
        signature,
        self_type,
        line_number,
        None,
    )
    .unwrap_or_else(|e| {
        eprintln!("Warning: {}", e);
        let raw = symbol.replace("rust-analyzer cargo ", "").replace(' ', "/");
        let normalized = raw.strip_suffix('.').unwrap_or(&raw);
        format!("{}{}", PROBE_URI_PREFIX, normalized)
    })
}

fn symbol_to_code_name_full(
    symbol: &str,
    display_name: &str,
    signature: Option<&str>,
    self_type: Option<&str>,
    line_number: Option<usize>,
    target_type: Option<&str>,
) -> Result<String, ProbeError> {
    let s = symbol.strip_prefix(SCIP_SYMBOL_PREFIX).ok_or_else(|| {
        ProbeError::invalid_symbol(
            format!("Symbol does not start with '{}'", SCIP_SYMBOL_PREFIX),
            symbol,
        )
    })?;

    let method_name = display_name.rsplit("::").next().unwrap_or(display_name);
    let expected_suffix = format!("{}().", method_name);

    if !s.ends_with(&expected_suffix) {
        return Err(ProbeError::invalid_symbol(
            format!("Symbol does not end with '{}'", expected_suffix),
            symbol,
        ));
    }

    let mut result = s[..s.len() - 1].to_string();

    if let Some(sig) = signature {
        if let Some(type_info) = extract_impl_type_info(sig) {
            if result.contains('#') {
                if let Some(hash_pos) = result.rfind('#') {
                    result = format!(
                        "{}<{}>{}",
                        &result[..hash_pos],
                        type_info,
                        &result[hash_pos..]
                    );
                }
            }
        }
    }

    if let Some(self_t) = self_type {
        if needs_self_type_enrichment(&result) {
            if let Some(slash_pos) = result.rfind('/') {
                let before_slash = &result[..=slash_pos];
                let after_slash = &result[slash_pos + 1..];
                result = format!("{}{}#{}", before_slash, self_t, after_slash);
            }
        }
    }

    let mut target_type_applied = false;
    if let Some(target_t) = target_type {
        if let Some(first_hash) = result.find('#') {
            let before_hash = &result[..first_hash];
            if !before_hash.ends_with('>') {
                result = format!("{}<{}>{}", before_hash, target_t, &result[first_hash..]);
                target_type_applied = true;
            }
        }
    }

    if let Some(line) = line_number {
        if !target_type_applied {
            result = format!("{}@{}", result, line);
        }
    }

    Ok(format!("{}{}", PROBE_URI_PREFIX, result.replace(' ', "/")))
}

/// Convert call graph to atoms with accurate line numbers by parsing source files.
#[must_use]
pub fn convert_to_atoms_with_parsed_spans(
    call_graph: &HashMap<String, FunctionNode>,
    symbol_to_display_name: &HashMap<String, String>,
    project_root: &Path,
    with_locations: bool,
    pkg_name: Option<&str>,
) -> Vec<AtomWithLines> {
    let relative_paths: Vec<String> = call_graph
        .values()
        .map(|node| node.relative_path.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let span_map = rust_parser::build_function_span_map(project_root, &relative_paths);

    convert_to_atoms_with_lines_internal(
        call_graph,
        symbol_to_display_name,
        Some(&span_map),
        with_locations,
        pkg_name,
    )
}

/// Internal function that does the actual conversion.
fn convert_to_atoms_with_lines_internal(
    call_graph: &HashMap<String, FunctionNode>,
    symbol_to_display_name: &HashMap<String, String>,
    span_map: Option<&HashMap<(String, String, usize), rust_parser::SpanInfo>>,
    with_locations: bool,
    pkg_name: Option<&str>,
) -> Vec<AtomWithLines> {
    // === Phase 1: Compute line ranges and base code_names for all nodes ===
    struct NodeData<'a> {
        node: &'a FunctionNode,
        lines_start: usize,
        lines_end: usize,
        base_code_name: String,
    }

    let mut sorted_nodes: Vec<&FunctionNode> = call_graph.values().collect();
    sorted_nodes.sort_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then_with(|| a.range.cmp(&b.range))
    });

    let node_data: Vec<NodeData> = sorted_nodes
        .into_iter()
        .map(|node| {
            let lines_start = if !node.range.is_empty() {
                node.range[0] as usize + 1
            } else {
                0
            };

            let lines_end = if let Some(map) = span_map {
                rust_parser::get_function_end_line(
                    map,
                    &node.relative_path,
                    &node.display_name,
                    lines_start,
                )
                .unwrap_or(lines_start)
            } else {
                match node.range.len() {
                    4 => node.range[2] as usize + 1,
                    _ => lines_start,
                }
            };

            let base_code_name = symbol_to_code_name(
                &node.symbol,
                &node.display_name,
                Some(&node.signature_text),
                node.self_type.as_deref(),
            );

            NodeData {
                node,
                lines_start,
                lines_end,
                base_code_name,
            }
        })
        .collect();

    // === Phase 2: Detect duplicates and compute final code_names ===
    let mut code_name_count: HashMap<String, usize> = HashMap::new();
    for data in &node_data {
        *code_name_count
            .entry(data.base_code_name.clone())
            .or_insert(0) += 1;
    }

    let mut code_name_to_nodes: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, data) in node_data.iter().enumerate() {
        code_name_to_nodes
            .entry(&data.base_code_name)
            .or_default()
            .push(idx);
    }

    let mut node_discriminating_type: HashMap<usize, Option<String>> = HashMap::new();
    for indices in code_name_to_nodes.values() {
        if indices.len() <= 1 {
            for &idx in indices {
                node_discriminating_type.insert(idx, None);
            }
            continue;
        }

        let all_contexts: Vec<&Vec<String>> = indices
            .iter()
            .map(|&idx| &node_data[idx].node.definition_type_context)
            .collect();

        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        for ctx in &all_contexts {
            for t in *ctx {
                *type_counts.entry(t.as_str()).or_insert(0) += 1;
            }
        }

        for &idx in indices {
            let ctx = &node_data[idx].node.definition_type_context;
            let discriminating = ctx
                .iter()
                .find(|t| type_counts.get(t.as_str()).copied().unwrap_or(0) == 1);
            node_discriminating_type.insert(idx, discriminating.cloned());
        }
    }

    let final_code_names: Vec<String> = node_data
        .iter()
        .enumerate()
        .map(|(idx, data)| {
            let is_duplicate = code_name_count
                .get(&data.base_code_name)
                .copied()
                .unwrap_or(0)
                > 1;

            if is_duplicate {
                let line_fallback = if data.lines_start > 0 {
                    Some(data.lines_start)
                } else {
                    None
                };
                let result = if let Some(Some(target_type)) = node_discriminating_type.get(&idx) {
                    symbol_to_code_name_full(
                        &data.node.symbol,
                        &data.node.display_name,
                        Some(&data.node.signature_text),
                        data.node.self_type.as_deref(),
                        line_fallback,
                        Some(target_type),
                    )
                } else if data.lines_start > 0 {
                    symbol_to_code_name_full(
                        &data.node.symbol,
                        &data.node.display_name,
                        Some(&data.node.signature_text),
                        data.node.self_type.as_deref(),
                        Some(data.lines_start),
                        None,
                    )
                } else {
                    Ok(data.base_code_name.clone())
                };
                result.unwrap_or_else(|e| {
                    eprintln!("Warning: {}", e);
                    data.base_code_name.clone()
                })
            } else {
                data.base_code_name.clone()
            }
        })
        .collect();

    // === Phase 3: Build map from raw symbol → list of (code_name, type_context) ===
    struct CodeNameWithContext {
        code_name: String,
        type_context: Vec<String>,
    }

    let mut raw_symbol_to_code_names: HashMap<String, Vec<CodeNameWithContext>> = HashMap::new();
    for (data, final_name) in node_data.iter().zip(final_code_names.iter()) {
        let type_context = data.node.definition_type_context.clone();

        raw_symbol_to_code_names
            .entry(data.node.symbol.clone())
            .or_default()
            .push(CodeNameWithContext {
                code_name: final_name.clone(),
                type_context,
            });
    }

    // === Phase 4: Build final atoms with resolved dependencies ===
    node_data
        .into_iter()
        .zip(final_code_names)
        .map(|(data, code_name)| {
            let mut dependencies = BTreeSet::new();
            let mut dependencies_with_locations: Vec<DependencyWithLocation> = Vec::new();

            for callee in &data.node.callees {
                let call_line_1based = if with_locations {
                    (callee.line + 1) as usize
                } else {
                    0
                };

                if let Some(code_name_contexts) = raw_symbol_to_code_names.get(&callee.symbol) {
                    if code_name_contexts.len() == 1 {
                        let dep_code_name = code_name_contexts[0].code_name.clone();
                        dependencies.insert(dep_code_name.clone());
                        if with_locations {
                            dependencies_with_locations.push(DependencyWithLocation {
                                code_name: dep_code_name,
                                location: CallLocation::Inner,
                                line: call_line_1based,
                            });
                        }
                    } else if !callee.type_hints.is_empty() {
                        let discriminating_hints: Vec<_> = callee
                            .type_hints
                            .iter()
                            .filter(|hint| {
                                let matching_count = code_name_contexts
                                    .iter()
                                    .filter(|ctx| ctx.type_context.iter().any(|t| t == *hint))
                                    .count();
                                matching_count > 0 && matching_count < code_name_contexts.len()
                            })
                            .collect();

                        let matched: Vec<_> = if !discriminating_hints.is_empty() {
                            code_name_contexts
                                .iter()
                                .filter(|ctx| {
                                    discriminating_hints
                                        .iter()
                                        .any(|hint| ctx.type_context.iter().any(|t| t == *hint))
                                })
                                .collect()
                        } else {
                            code_name_contexts
                                .iter()
                                .filter(|ctx| {
                                    callee.type_hints.iter().any(|hint| {
                                        ctx.type_context
                                            .iter()
                                            .any(|t| t.contains(hint) || hint.contains(t))
                                    })
                                })
                                .collect()
                        };

                        if matched.len() == 1 {
                            let dep_code_name = matched[0].code_name.clone();
                            dependencies.insert(dep_code_name.clone());
                            if with_locations {
                                dependencies_with_locations.push(DependencyWithLocation {
                                    code_name: dep_code_name,
                                    location: CallLocation::Inner,
                                    line: call_line_1based,
                                });
                            }
                        } else {
                            for ctx in code_name_contexts {
                                dependencies.insert(ctx.code_name.clone());
                                if with_locations {
                                    dependencies_with_locations.push(DependencyWithLocation {
                                        code_name: ctx.code_name.clone(),
                                        location: CallLocation::Inner,
                                        line: call_line_1based,
                                    });
                                }
                            }
                        }
                    } else {
                        for ctx in code_name_contexts {
                            dependencies.insert(ctx.code_name.clone());
                            if with_locations {
                                dependencies_with_locations.push(DependencyWithLocation {
                                    code_name: ctx.code_name.clone(),
                                    location: CallLocation::Inner,
                                    line: call_line_1based,
                                });
                            }
                        }
                    }
                } else {
                    let display_name = symbol_to_display_name
                        .get(&callee.symbol)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    let dep_path = symbol_to_code_name(&callee.symbol, &display_name, None, None);
                    dependencies.insert(dep_path.clone());
                    if with_locations {
                        dependencies_with_locations.push(DependencyWithLocation {
                            code_name: dep_path,
                            location: CallLocation::Inner,
                            line: call_line_1based,
                        });
                    }
                }
            }

            let code_module = extract_code_module(&code_name);
            let rqn = derive_rust_qualified_name(
                &data.node.relative_path,
                &data.node.display_name,
                pkg_name,
            );
            AtomWithLines {
                display_name: data.node.display_name.clone(),
                code_name,
                dependencies,
                dependencies_with_locations,
                code_module,
                code_path: data.node.relative_path.clone(),
                code_text: CodeTextInfo {
                    lines_start: data.lines_start,
                    lines_end: data.lines_end,
                },
                kind: DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: rqn,
            }
        })
        .collect()
}

/// Information about a duplicate code_name
#[derive(Debug, Clone)]
pub struct DuplicateCodeName {
    pub code_name: String,
    pub occurrences: Vec<DuplicateOccurrence>,
}

#[derive(Debug, Clone)]
pub struct DuplicateOccurrence {
    pub display_name: String,
    pub code_path: String,
    pub lines_start: usize,
}

/// Check for duplicate code_names in the atoms output.
#[must_use]
pub fn find_duplicate_code_names(atoms: &[AtomWithLines]) -> Vec<DuplicateCodeName> {
    let mut code_name_to_atoms: HashMap<String, Vec<&AtomWithLines>> = HashMap::new();

    for atom in atoms {
        code_name_to_atoms
            .entry(atom.code_name.clone())
            .or_default()
            .push(atom);
    }

    code_name_to_atoms
        .into_iter()
        .filter(|(_, atoms)| atoms.len() > 1)
        .map(|(code_name, atoms)| DuplicateCodeName {
            code_name,
            occurrences: atoms
                .into_iter()
                .map(|a| DuplicateOccurrence {
                    display_name: a.display_name.clone(),
                    code_path: a.code_path.clone(),
                    lines_start: a.code_text.lines_start,
                })
                .collect(),
        })
        .collect()
}

fn extract_display_name_from_code_name(code_name: &str) -> String {
    let s = code_name
        .strip_prefix(PROBE_URI_PREFIX)
        .unwrap_or(code_name);
    let without_parens = s
        .strip_suffix("().")
        .or_else(|| s.strip_suffix("()"))
        .unwrap_or(s);
    let name = without_parens
        .rsplit_once(']')
        .map(|(_, n)| n)
        .or_else(|| without_parens.rsplit_once('#').map(|(_, n)| n))
        .or_else(|| without_parens.rsplit_once('/').map(|(_, n)| n))
        .unwrap_or(without_parens);
    name.to_string()
}

/// Normalize a code_name by stripping a trailing dot if present.
#[must_use]
pub fn normalize_code_name(code_name: &str) -> String {
    code_name.strip_suffix('.').unwrap_or(code_name).to_string()
}

/// Add stub atoms for external function dependencies that don't have their own atom entry.
#[must_use]
pub fn add_external_stubs(atoms_dict: &mut BTreeMap<String, AtomWithLines>) -> usize {
    let external_deps: Vec<String> = atoms_dict
        .values()
        .flat_map(|atom| atom.dependencies.iter().cloned())
        .filter(|dep| !atoms_dict.contains_key(dep))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let count = external_deps.len();
    for dep_code_name in external_deps {
        let display_name = extract_display_name_from_code_name(&dep_code_name);
        let code_module = extract_code_module(&dep_code_name);
        atoms_dict.insert(
            dep_code_name.clone(),
            AtomWithLines {
                display_name,
                code_name: dep_code_name,
                dependencies: BTreeSet::new(),
                dependencies_with_locations: Vec::new(),
                code_module,
                code_path: String::new(),
                code_text: CodeTextInfo {
                    lines_start: 0,
                    lines_end: 0,
                },
                kind: DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
            },
        );
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrich_impl_method() {
        let symbol =
            "rust-analyzer cargo curve25519-dalek 4.1.3 edwards/CompressedEdwardsY#ConstantTimeEq<&CompressedEdwardsY>#ct_eq().";
        assert_eq!(
            enrich_display_name(symbol, "ct_eq"),
            "CompressedEdwardsY::ct_eq"
        );
    }

    #[test]
    fn test_enrich_free_function_unchanged() {
        let symbol =
            "rust-analyzer cargo curve25519-dalek 4.1.3 ristretto_specs/specs/spec_ristretto_decompress().";
        assert_eq!(
            enrich_display_name(symbol, "spec_ristretto_decompress"),
            "spec_ristretto_decompress"
        );
    }

    #[test]
    fn test_extract_function_name_method() {
        assert_eq!(
            extract_function_name_from_symbol(
                "rust-analyzer cargo x25519-dalek 2.0.1 x25519/StaticSecret#diffie_hellman()."
            ),
            "diffie_hellman"
        );
    }

    #[test]
    fn test_extract_function_name_free_function() {
        assert_eq!(
            extract_function_name_from_symbol("rust-analyzer cargo core 1.0.0 mem/swap()."),
            "swap"
        );
    }

    #[test]
    fn test_external_function_detected() {
        let known = HashSet::new();
        assert!(constants::is_external_function_symbol(
            "rust-analyzer cargo x25519-dalek 2.0.1 x25519/impl#[StaticSecret]diffie_hellman().",
            &known,
        ));
    }

    #[test]
    fn test_add_external_stubs_creates_missing() {
        let mut atoms_dict = BTreeMap::new();
        let mut deps = BTreeSet::new();
        deps.insert("probe:external-crate/1.0/mod/func()".to_string());

        atoms_dict.insert(
            "probe:my-crate/1.0/caller()".to_string(),
            AtomWithLines {
                display_name: "caller".to_string(),
                code_name: "probe:my-crate/1.0/caller()".to_string(),
                dependencies: deps,
                dependencies_with_locations: Vec::new(),
                code_module: String::new(),
                code_path: "src/lib.rs".to_string(),
                code_text: CodeTextInfo {
                    lines_start: 10,
                    lines_end: 20,
                },
                kind: DeclKind::Exec,
                language: "rust".to_string(),
                rust_qualified_name: None,
            },
        );

        let count = add_external_stubs(&mut atoms_dict);
        assert_eq!(count, 1);
        assert_eq!(atoms_dict.len(), 2);

        let stub = atoms_dict
            .get("probe:external-crate/1.0/mod/func()")
            .unwrap();
        assert_eq!(stub.display_name, "func");
        assert!(stub.code_path.is_empty());
    }

    #[test]
    fn test_derive_rust_qualified_name_method() {
        let rqn = derive_rust_qualified_name(
            "curve25519-dalek/src/backend/serial/u64/field.rs",
            "FieldElement51::reduce",
            None,
        );
        assert_eq!(
            rqn.unwrap(),
            "curve25519_dalek::backend::serial::u64::field::FieldElement51::reduce"
        );
    }

    #[test]
    fn test_derive_rust_qualified_name_lib_root() {
        let rqn = derive_rust_qualified_name("my-crate/src/lib.rs", "init", None);
        assert_eq!(rqn.unwrap(), "my_crate::init");
    }

    #[test]
    fn test_derive_rust_qualified_name_bare_src_prefix() {
        let rqn = derive_rust_qualified_name("src/lib.rs", "init", None);
        assert_eq!(rqn.unwrap(), "init");
    }

    #[test]
    fn test_derive_rust_qualified_name_bare_src_with_pkg_name() {
        let rqn = derive_rust_qualified_name("src/lib.rs", "init", Some("curve25519-dalek"));
        assert_eq!(rqn.unwrap(), "curve25519_dalek::init");
    }

    #[test]
    fn test_derive_rust_qualified_name_bare_src_nested() {
        let rqn = derive_rust_qualified_name("src/commands/extract.rs", "cmd_extract", None);
        assert_eq!(rqn.unwrap(), "commands::extract::cmd_extract");
    }

    #[test]
    fn test_derive_rust_qualified_name_bare_src_nested_with_pkg_name() {
        let rqn = derive_rust_qualified_name(
            "src/commands/extract.rs",
            "cmd_extract",
            Some("probe-rust"),
        );
        assert_eq!(rqn.unwrap(), "probe_rust::commands::extract::cmd_extract");
    }

    #[test]
    fn test_derive_rust_qualified_name_no_src() {
        assert!(derive_rust_qualified_name("some/path/file.rs", "foo", None).is_none());
    }

    #[test]
    fn test_derive_rust_qualified_name_empty() {
        assert!(derive_rust_qualified_name("", "foo", None).is_none());
    }

    #[test]
    fn test_derive_rust_qualified_name_pkg_name_ignored_when_crate_in_path() {
        let rqn =
            derive_rust_qualified_name("curve25519-dalek/src/lib.rs", "init", Some("other-crate"));
        assert_eq!(rqn.unwrap(), "curve25519_dalek::init");
    }

    #[test]
    fn test_language_field_defaults_to_rust_on_old_json() {
        let old_json = serde_json::json!({
            "display-name": "foo",
            "dependencies": [],
            "code-module": "",
            "code-path": "src/lib.rs",
            "code-text": { "lines-start": 1, "lines-end": 10 },
            "kind": "exec"
        });
        let atom: AtomWithLines = serde_json::from_value(old_json).unwrap();
        assert_eq!(atom.language, "rust");
    }

    #[test]
    fn test_normalize_code_name_strips_trailing_dot() {
        assert_eq!(
            normalize_code_name("probe:x25519-dalek/2.0.1/x25519/diffie_hellman()."),
            "probe:x25519-dalek/2.0.1/x25519/diffie_hellman()"
        );
    }
}
