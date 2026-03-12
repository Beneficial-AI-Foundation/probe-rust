//! Parser module using syn to extract accurate function spans.
//!
//! SCIP only provides the location of function names, not their full body spans.
//! This module parses the actual source files to get accurate start/end line numbers.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// Function span information
#[derive(Debug, Clone)]
pub struct FunctionSpan {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
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
        if let Some(ident) = &node.mac.path.get_ident() {
            if *ident == "cfg_if" {
                if let Ok(branches) = syn::parse2::<CfgIfMacroBody>(node.mac.tokens.clone()) {
                    for items in branches.all_items {
                        for item in items {
                            self.visit_item(&item);
                        }
                    }
                }
            }
        }
        syn::visit::visit_item_macro(self, node);
    }
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

/// Parse all source files in a project and build a lookup map.
///
/// Returns a map from (relative_path, function_name, definition_line) -> SpanInfo.
pub fn build_function_span_map(
    project_root: &Path,
    relative_paths: &[String],
) -> HashMap<(String, String, usize), SpanInfo> {
    let mut span_map = HashMap::new();

    for rel_path in relative_paths {
        let full_path = project_root.join(rel_path);
        if !full_path.exists() {
            continue;
        }

        if let Ok(functions) = parse_file_for_spans(&full_path) {
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

/// Extract the bare function name from a possibly enriched display name.
///
/// SCIP display names are enriched with type info (e.g., "EdwardsPoint::eq"),
/// but syn only stores the bare function name ("eq"). This strips the
/// `Type::` prefix to enable matching.
fn bare_function_name(function_name: &str) -> &str {
    function_name.rsplit("::").next().unwrap_or(function_name)
}

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
}
