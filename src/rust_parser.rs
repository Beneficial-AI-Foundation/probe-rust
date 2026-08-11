//! Parser module using syn to extract accurate function spans.
//!
//! SCIP only provides the location of function names, not their full body spans.
//! This module parses the actual source files to get accurate start/end line numbers.
//! Also provides richer `FunctionInfo` for the `list-functions` command.

use quote::ToTokens;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;
use walkdir::WalkDir;

/// Function span information
#[derive(Debug, Clone)]
pub struct FunctionSpan {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    /// The combined item-gating `#[cfg(...)]` predicate governing this function
    /// (own `#[cfg]` plus every enclosing `impl`/`mod`/`trait` gate, `all(...)`-
    /// joined), if any. `None` when the function has no `#[cfg]` gate. Consumers
    /// evaluate it against the build config to decide whether the function is
    /// compiled (and hence in verification scope).
    pub cfg: Option<String>,
    /// Whether the function has an actual body. `false` only for a trait method
    /// *declaration* with no default body (`fn f() -> Self;`); `true` for free
    /// functions, impl methods, and trait methods that do have a default body.
    pub has_body: bool,
}

// =============================================================================
// FunctionInfo - richer metadata for list-functions
// =============================================================================

/// Detailed function information for the list-functions command.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub is_method: bool,
}

/// Summary statistics for a function listing.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionListSummary {
    pub total_functions: usize,
    pub total_files: usize,
}

/// Full output of list_all_functions.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionListOutput {
    pub functions: Vec<FunctionInfo>,
    pub summary: FunctionListSummary,
}

/// Span information for a function.
#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub end_line: usize,
    /// Combined item-gating `#[cfg(...)]` predicate (see [`FunctionSpan::cfg`]).
    pub cfg: Option<String>,
    /// Whether the function has a body (see [`FunctionSpan::has_body`]).
    pub has_body: bool,
}

/// Extract the predicates of **all** item-gating `#[cfg(...)]` attributes on an
/// item, in source order (e.g. `["feature = \"alloc\"", "not(test)"]`).
///
/// Multiple `#[cfg]` attributes on the same item are conjunctive — the item is
/// compiled only if every one holds — so callers combine them with `all(...)`.
/// Only true `#[cfg(...)]` is item-gating. `#[cfg_attr(...)]` conditionally adds
/// a *doc/derive/allow* attribute but always compiles the item, so it is
/// deliberately ignored — it is not a scope gate.
fn cfg_predicates_of(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("cfg") {
                if let syn::Meta::List(list) = &attr.meta {
                    return Some(list.tokens.to_string());
                }
            }
            None
        })
        .collect()
}

/// Combine several cfg predicates into one with `all(...)`. `None` when empty.
fn combine_cfg_predicates(preds: &[String]) -> Option<String> {
    match preds {
        [] => None,
        [single] => Some(single.clone()),
        many => Some(format!("all({})", many.join(", "))),
    }
}

/// Visitor that collects function spans from an AST
struct FunctionSpanVisitor {
    functions: Vec<FunctionSpan>,
    /// Stack of `#[cfg(...)]` predicates from enclosing `mod`/`impl`/`trait`
    /// blocks. A function's effective predicate is `all(stack + own_cfg)`.
    cfg_stack: Vec<String>,
}

impl FunctionSpanVisitor {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            cfg_stack: Vec::new(),
        }
    }

    /// Combined predicate for a function with the given `attrs`: enclosing gates
    /// (the stack) plus all of the function's own `#[cfg(...)]` attributes.
    fn combined_cfg(&self, attrs: &[syn::Attribute]) -> Option<String> {
        let mut parts = self.cfg_stack.clone();
        parts.extend(cfg_predicates_of(attrs));
        combine_cfg_predicates(&parts)
    }

    /// Run `f` with all of `attrs`'s `#[cfg]` predicates pushed onto the
    /// enclosing-gate stack, restoring it afterwards. Used for `mod`/`impl`/
    /// `trait` blocks, which may themselves carry multiple `#[cfg]` attributes.
    fn with_enclosing_cfg(&mut self, attrs: &[syn::Attribute], f: impl FnOnce(&mut Self)) {
        let pushed = cfg_predicates_of(attrs);
        let n = pushed.len();
        self.cfg_stack.extend(pushed);
        f(self);
        self.cfg_stack.truncate(self.cfg_stack.len() - n);
    }
}

impl<'ast> Visit<'ast> for FunctionSpanVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            has_body: true,
        });

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            has_body: true,
        });

        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            // `default: None` is a bodiless declaration (`fn f() -> Self;`).
            has_body: node.default.is_some(),
        });

        syn::visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        self.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_impl(v, node));
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_trait(v, node));
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_mod(v, node));
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(items) = try_parse_cfg_if_items(node) {
            for item in items {
                self.visit_item(&item);
            }
        }
        syn::visit::visit_item_macro(self, node);
    }
}

/// Try to parse a `cfg_if!` macro node and return the items from all branches.
/// Returns `None` if the macro is not `cfg_if` or parsing fails.
fn try_parse_cfg_if_items(node: &syn::ItemMacro) -> Option<Vec<syn::Item>> {
    let ident = node.mac.path.get_ident()?;
    if *ident != "cfg_if" {
        return None;
    }
    let branches = syn::parse2::<CfgIfMacroBody>(node.mac.tokens.clone()).ok()?;
    Some(branches.all_items.into_iter().flatten().collect())
}

/// Helper struct to parse cfg_if! macro body
struct CfgIfMacroBody {
    all_items: Vec<Vec<syn::Item>>,
}

impl syn::parse::Parse for CfgIfMacroBody {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        use syn::Token;

        let mut all_items = Vec::new();

        if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            input.parse::<Token![#]>()?;
            let _attr_group: proc_macro2::Group = input.parse()?;

            let content;
            syn::braced!(content in input);
            let mut items = Vec::new();
            while !content.is_empty() {
                items.push(content.parse()?);
            }
            all_items.push(items);
        }

        while input.peek(Token![else]) {
            input.parse::<Token![else]>()?;

            if input.peek(Token![if]) {
                input.parse::<Token![if]>()?;
                input.parse::<Token![#]>()?;
                let _attr_group: proc_macro2::Group = input.parse()?;

                let content;
                syn::braced!(content in input);
                let mut items = Vec::new();
                while !content.is_empty() {
                    items.push(content.parse()?);
                }
                all_items.push(items);
            } else {
                let content;
                syn::braced!(content in input);
                let mut items = Vec::new();
                while !content.is_empty() {
                    items.push(content.parse()?);
                }
                all_items.push(items);
                break;
            }
        }

        Ok(CfgIfMacroBody { all_items })
    }
}

/// Parse a single source file and extract all function spans.
pub fn parse_file_for_spans(file_path: &Path) -> Result<Vec<FunctionSpan>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;

    let syntax_tree = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse file {}: {}", file_path.display(), e))?;

    let mut visitor = FunctionSpanVisitor::new();
    visitor.visit_file(&syntax_tree);

    Ok(visitor.functions)
}

// =============================================================================
// FunctionInfoVisitor - collects richer metadata (visibility, context)
// =============================================================================

struct FunctionInfoVisitor {
    functions: Vec<FunctionInfo>,
    current_context: Option<String>,
}

impl FunctionInfoVisitor {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            current_context: None,
        }
    }
}

fn visibility_string(vis: &syn::Visibility) -> Option<String> {
    match vis {
        syn::Visibility::Public(_) => Some("pub".to_string()),
        syn::Visibility::Restricted(r) => Some(format!("pub({})", r.path.to_token_stream())),
        syn::Visibility::Inherited => None,
    }
}

impl<'ast> Visit<'ast> for FunctionInfoVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let span = node.span();
        self.functions.push(FunctionInfo {
            name: node.sig.ident.to_string(),
            file: None,
            start_line: span.start().line,
            end_line: span.end().line,
            visibility: visibility_string(&node.vis),
            context: self.current_context.clone(),
            is_method: false,
        });
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let type_name = node.self_ty.to_token_stream().to_string();
        let ctx = if let Some((_, ref trait_path, _)) = node.trait_ {
            format!("impl {} for {}", trait_path.to_token_stream(), type_name)
        } else {
            format!("impl {}", type_name)
        };
        let prev = self.current_context.replace(ctx);
        syn::visit::visit_item_impl(self, node);
        self.current_context = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let span = node.span();
        self.functions.push(FunctionInfo {
            name: node.sig.ident.to_string(),
            file: None,
            start_line: span.start().line,
            end_line: span.end().line,
            visibility: visibility_string(&node.vis),
            context: self.current_context.clone(),
            is_method: true,
        });
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        let ctx = format!("trait {}", node.ident);
        let prev = self.current_context.replace(ctx);
        syn::visit::visit_item_trait(self, node);
        self.current_context = prev;
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let span = node.span();
        self.functions.push(FunctionInfo {
            name: node.sig.ident.to_string(),
            file: None,
            start_line: span.start().line,
            end_line: span.end().line,
            visibility: None,
            context: self.current_context.clone(),
            is_method: true,
        });
        syn::visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(items) = try_parse_cfg_if_items(node) {
            for item in items {
                self.visit_item(&item);
            }
        }
        syn::visit::visit_item_macro(self, node);
    }
}

/// Parse a single source file and extract detailed function information.
pub fn parse_file_for_function_info(file_path: &Path) -> Result<Vec<FunctionInfo>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;

    let syntax_tree = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse file {}: {}", file_path.display(), e))?;

    let mut visitor = FunctionInfoVisitor::new();
    visitor.visit_file(&syntax_tree);

    Ok(visitor.functions)
}

/// Walk all `.rs` files under `root` and collect function information.
///
/// Skips `target/` directories. Returns a `FunctionListOutput` with all
/// functions and summary statistics.
pub fn list_all_functions(root: &Path) -> FunctionListOutput {
    let mut functions = Vec::new();
    let mut file_count = 0;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != "target" && name != ".git"
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }

        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        match parse_file_for_function_info(path) {
            Ok(file_functions) => {
                if !file_functions.is_empty() {
                    file_count += 1;
                }
                for mut fi in file_functions {
                    fi.file = Some(rel_path.clone());
                    functions.push(fi);
                }
            }
            Err(e) => {
                eprintln!("Warning: {}", e);
            }
        }
    }

    let total_functions = functions.len();
    FunctionListOutput {
        functions,
        summary: FunctionListSummary {
            total_functions,
            total_files: file_count,
        },
    }
}

/// Parse all source files in a project and build a lookup map.
///
/// Returns a map from (relative_path, function_name, definition_line) -> SpanInfo.
pub fn build_function_span_map(
    project_root: &Path,
    relative_paths: &[String],
) -> HashMap<(String, String, usize), SpanInfo> {
    let mut span_map = HashMap::new();

    let canonical_root = match project_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Warning: cannot canonicalize project root {}: {e}  — skipping path safety checks",
                project_root.display()
            );
            return span_map;
        }
    };

    for rel_path in relative_paths {
        let full_path = project_root.join(rel_path);
        let canonical = match full_path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !canonical.starts_with(&canonical_root) {
            eprintln!(
                "Warning: SCIP relative_path escapes project root, skipping: {}",
                rel_path
            );
            continue;
        }

        if let Ok(functions) = parse_file_for_spans(&canonical) {
            for func in functions {
                let key = (rel_path.clone(), func.name.clone(), func.start_line);
                span_map.insert(
                    key,
                    SpanInfo {
                        end_line: func.end_line,
                        cfg: func.cfg.clone(),
                        has_body: func.has_body,
                    },
                );
            }
        }
    }

    span_map
}

use crate::bare_function_name;

/// Look up the parsed [`SpanInfo`] for a function given its path, name, and
/// SCIP start line. Tries an exact `(path, bare-name, start-line)` key first,
/// then a containment match (same file + name, SCIP start within the parsed
/// span). Shared by [`get_function_end_line`] and [`get_function_cfg`].
fn find_span_info<'a>(
    span_map: &'a HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<&'a SpanInfo> {
    let bare_name = bare_function_name(function_name);

    // Try exact match first
    let key = (relative_path.to_string(), bare_name.to_string(), start_line);
    if let Some(span_info) = span_map.get(&key) {
        return Some(span_info);
    }

    // Try containment match: find a function with the same name in the same file
    // where the SCIP start_line falls within the parsed span.
    span_map
        .iter()
        .find_map(|((path, name, parsed_start), span_info)| {
            (path == relative_path
                && name == bare_name
                && start_line >= *parsed_start
                && start_line <= span_info.end_line)
                .then_some(span_info)
        })
}

/// Get the end line for a function given its path, name, and start line.
pub fn get_function_end_line(
    span_map: &HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<usize> {
    find_span_info(span_map, relative_path, function_name, start_line).map(|s| s.end_line)
}

/// Get the combined `#[cfg(...)]` predicate for a function given its path, name,
/// and start line, using the same matching as [`get_function_end_line`].
pub fn get_function_cfg(
    span_map: &HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<String> {
    find_span_info(span_map, relative_path, function_name, start_line).and_then(|s| s.cfg.clone())
}

/// Whether the function has a body, using the same matching as
/// [`get_function_end_line`]. `None` when no span was resolved for it.
pub fn get_function_has_body(
    span_map: &HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<bool> {
    find_span_info(span_map, relative_path, function_name, start_line).map(|s| s.has_body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_simple_function() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
fn hello_world() {{
    println!("Hello, world!");
}}

fn another_function(x: i32) -> i32 {{
    x + 1
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].name, "hello_world");
        assert_eq!(spans[1].name, "another_function");

        assert!(spans[0].end_line >= spans[0].start_line);
        assert!(spans[1].end_line >= spans[1].start_line);
    }

    #[test]
    fn test_trait_method_has_body() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
trait T {{
    fn bodiless(&self) -> u32;

    fn bodiless_multiline<I>(iter: I) -> u32
    where
        I: IntoIterator<Item = u32>;

    fn defaulted(&self) -> u32 {{
        1
    }}
}}

struct S;

impl T for S {{
    fn bodiless(&self) -> u32 {{
        2
    }}
}}

fn free() {{}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let get = |n: &str| spans.iter().filter(|f| f.name == n).collect::<Vec<_>>();

        // The bodiless declaration and its concrete impl share a name; only the
        // declaration is bodiless.
        let bodiless = get("bodiless");
        assert_eq!(bodiless.len(), 2);
        assert_eq!(
            bodiless.iter().filter(|f| f.has_body).count(),
            1,
            "exactly one `bodiless` (the impl) should have a body"
        );

        // A multi-line signature is still bodiless — end_line > start_line, so the
        // span alone cannot be used as a proxy.
        let ml = get("bodiless_multiline");
        assert_eq!(ml.len(), 1);
        assert!(!ml[0].has_body);
        assert!(ml[0].end_line > ml[0].start_line);

        assert!(get("defaulted")[0].has_body);
        assert!(get("free")[0].has_body);
    }

    #[test]
    fn test_cfg_predicate_capture() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
#[cfg(feature = "alloc")]
fn gated_free() {{}}

fn plain_free() {{}}

#[cfg(unix)]
#[cfg(feature = "std")]
fn gated_multi() {{}}

#[cfg(test)]
mod tests_mod {{
    #[cfg(feature = "serde")]
    impl Foo {{
        fn nested(&self) {{}}
    }}
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let by = |n: &str| spans.iter().find(|s| s.name == n).unwrap();

        // Own gate only.
        assert_eq!(
            by("gated_free").cfg.as_deref(),
            Some(r#"feature = "alloc""#)
        );
        // No gate anywhere.
        assert_eq!(by("plain_free").cfg, None);
        // Multiple `#[cfg]` on one item are conjunctive → `all(...)`.
        assert_eq!(
            by("gated_multi").cfg.as_deref(),
            Some(r#"all(unix, feature = "std")"#)
        );
        // Enclosing mod + impl gates combined with `all(...)`, outermost first.
        assert_eq!(
            by("nested").cfg.as_deref(),
            Some(r#"all(test, feature = "serde")"#)
        );
    }

    #[test]
    fn test_parse_impl_methods() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
pub fn public_func() {{}}

fn private_func() {{}}

impl Foo {{
    pub fn method(&self) {{}}
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        assert_eq!(spans.len(), 3);

        let names: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"public_func"));
        assert!(names.contains(&"private_func"));
        assert!(names.contains(&"method"));
    }

    #[test]
    fn test_bare_function_name() {
        assert_eq!(bare_function_name("EdwardsPoint::eq"), "eq");
        assert_eq!(bare_function_name("simple_func"), "simple_func");
        assert_eq!(bare_function_name("A::B::method"), "method");
    }

    #[test]
    fn test_function_info_visibility_and_context() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
pub fn public_func() {{}}

fn private_func() {{}}

impl MyStruct {{
    pub fn pub_method(&self) {{}}
    fn priv_method(&self) {{}}
}}

trait MyTrait {{
    fn trait_method(&self);
}}
"#
        )
        .unwrap();

        let funcs = parse_file_for_function_info(file.path()).unwrap();
        assert_eq!(funcs.len(), 5);

        let public_func = funcs.iter().find(|f| f.name == "public_func").unwrap();
        assert_eq!(public_func.visibility.as_deref(), Some("pub"));
        assert!(public_func.context.is_none());
        assert!(!public_func.is_method);

        let private_func = funcs.iter().find(|f| f.name == "private_func").unwrap();
        assert!(private_func.visibility.is_none());
        assert!(!private_func.is_method);

        let pub_method = funcs.iter().find(|f| f.name == "pub_method").unwrap();
        assert_eq!(pub_method.visibility.as_deref(), Some("pub"));
        assert_eq!(pub_method.context.as_deref(), Some("impl MyStruct"));
        assert!(pub_method.is_method);

        let priv_method = funcs.iter().find(|f| f.name == "priv_method").unwrap();
        assert!(priv_method.visibility.is_none());
        assert!(priv_method.is_method);

        let trait_method = funcs.iter().find(|f| f.name == "trait_method").unwrap();
        assert_eq!(trait_method.context.as_deref(), Some("trait MyTrait"));
        assert!(trait_method.is_method);
    }

    #[test]
    fn test_function_info_trait_impl_context() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
trait Greet {{
    fn greet(&self);
}}

impl Greet for MyStruct {{
    fn greet(&self) {{}}
}}
"#
        )
        .unwrap();

        let funcs = parse_file_for_function_info(file.path()).unwrap();
        assert_eq!(funcs.len(), 2);

        let trait_fn = funcs
            .iter()
            .find(|f| f.context.as_deref() == Some("trait Greet"))
            .unwrap();
        assert_eq!(trait_fn.name, "greet");

        let impl_fn = funcs
            .iter()
            .find(|f| {
                f.context
                    .as_deref()
                    .is_some_and(|c| c.contains("impl Greet for"))
            })
            .unwrap();
        assert_eq!(impl_fn.name, "greet");
    }
}
