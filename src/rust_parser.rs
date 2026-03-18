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

/// Span information for a function (simplified: just end_line)
#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub end_line: usize,
}

/// Visitor that collects function spans from an AST
struct FunctionSpanVisitor {
    functions: Vec<FunctionSpan>,
}

impl FunctionSpanVisitor {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for FunctionSpanVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
        });

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
        });

        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;

        self.functions.push(FunctionSpan {
            name,
            start_line,
            end_line,
        });

        syn::visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        syn::visit::visit_item_trait(self, node);
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
                    },
                );
            }
        }
    }

    span_map
}

use crate::bare_function_name;

/// Get the end line for a function given its path, name, and start line.
pub fn get_function_end_line(
    span_map: &HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<usize> {
    let bare_name = bare_function_name(function_name);

    // Try exact match first
    let key = (relative_path.to_string(), bare_name.to_string(), start_line);
    if let Some(span_info) = span_map.get(&key) {
        return Some(span_info.end_line);
    }

    // Try containment match: find a function with the same name in the same file
    // where the SCIP start_line falls within the parsed span.
    for ((path, name, parsed_start), span_info) in span_map.iter() {
        if path == relative_path
            && name == bare_name
            && start_line >= *parsed_start
            && start_line <= span_info.end_line
        {
            return Some(span_info.end_line);
        }
    }

    None
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
