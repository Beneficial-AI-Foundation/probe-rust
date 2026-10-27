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
    /// (own `#[cfg]` plus every enclosing `impl`/`mod`/`trait`/`extern` gate,
    /// `all(...)`-joined), if any. `None` when the function has no `#[cfg]`
    /// gate. Consumers evaluate it against the build config to decide whether
    /// the function is compiled (and hence in verification scope).
    pub cfg: Option<String>,
    /// Declared inside an `extern { … }` block: a binding whose implementation
    /// lives outside Rust — there is no Rust body to analyze or verify.
    pub is_foreign: bool,
    /// A bodyless trait method signature: the proof obligation lives on the
    /// implementations, not here. `false` for trait methods with a default
    /// body.
    pub trait_required: bool,
    /// For a method of an `impl` block: the source ranges of that impl's
    /// trait path (trait impls only) and self type. Consumers resolve the
    /// names in these ranges to identify the method without relying on line
    /// numbers. Empty for every other function, and for a method whose name
    /// occurs more than once under the same impl header in the file
    /// (`cfg`-split variants, in one impl block or in several, share the
    /// header, so it cannot tell them apart).
    pub impl_header: Vec<SourceRange>,
    /// The number of type paths in that impl header (the trait path, its type
    /// arguments, the self type and their nested type arguments) when the
    /// header is made of type paths only; `None` when it also contains a
    /// reference, slice, array, tuple, pointer, trait object or associated
    /// type binding, or when `impl_header` is empty. Primitive types and type
    /// parameters are paths to `syn` and are counted, so a consumer that
    /// resolves fewer types than this count knows the header names more than
    /// the resolved paths.
    pub impl_header_path_count: Option<usize>,
}

/// The impl header that methods of the innermost enclosing item inherit.
#[derive(Default)]
struct ImplHeaderFrame {
    ranges: Vec<SourceRange>,
    /// See [`FunctionSpan::impl_header_path_count`].
    path_count: Option<usize>,
    /// The header's tokens (trait path and self type), which identify impl
    /// blocks with the same header within a file.
    key: String,
}

/// A source range in `syn` coordinates: 1-based lines, 0-based columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SourceRange {
    fn of(span: proc_macro2::Span) -> Self {
        Self {
            start_line: span.start().line,
            start_column: span.start().column,
            end_line: span.end().line,
            end_column: span.end().column,
        }
    }

    /// `true` when the range `[start, end)` given as 1-based line and 0-based
    /// column pairs lies inside this range.
    pub fn contains(&self, start: (usize, usize), end: (usize, usize)) -> bool {
        (self.start_line, self.start_column) <= start && end <= (self.end_line, self.end_column)
    }
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
    /// Extern-block member (see [`FunctionSpan::is_foreign`]).
    pub is_foreign: bool,
    /// Bodyless trait signature (see [`FunctionSpan::trait_required`]).
    pub trait_required: bool,
    /// Enclosing impl header ranges (see [`FunctionSpan::impl_header`]).
    pub impl_header: Vec<SourceRange>,
    /// Type-path count of that header (see [`FunctionSpan::impl_header_path_count`]).
    pub impl_header_path_count: Option<usize>,
}

/// Extract the predicates of **all** item-gating `#[cfg(...)]` attributes on an
/// item, in source order (e.g. `["feature = \"alloc\"", "not(test)"]`).
///
/// Multiple `#[cfg]` attributes on the same item are conjunctive — the item is
/// compiled only if every one holds — so callers combine them with `all(...)`.
/// Only true `#[cfg(...)]` is collected. `#[cfg_attr(cond, ...)]` is ignored:
/// it usually injects cosmetic attributes, but it CAN inject `cfg(...)` (a
/// real gate) — ignoring that case under-gates, which is the permitted error
/// direction (the item is treated as compiled). A `cfg_attr`-injected `path`
/// on a `mod` declaration is handled separately by the module walk (it taints
/// the walk, because it can change which file is mounted).
pub(crate) fn cfg_predicates_of(attrs: &[syn::Attribute]) -> Vec<String> {
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
    /// Header ranges of the innermost enclosing item that owns methods: an
    /// `impl` block pushes its trait-path and self-type ranges; a free
    /// function, trait or module pushes an empty entry, so a function nested
    /// in one of them is not attributed to an outer impl.
    impl_header_stack: Vec<ImplHeaderFrame>,
    /// For each entry of `functions`, the header key of its impl when it
    /// recorded a header (see [`ImplHeaderFrame::key`]).
    impl_header_keys: Vec<Option<String>>,
}

impl FunctionSpanVisitor {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            cfg_stack: Vec::new(),
            impl_header_stack: Vec::new(),
            impl_header_keys: Vec::new(),
        }
    }

    /// Record `span`, with the header key of the enclosing impl when it
    /// carries a header.
    fn push_function(&mut self, span: FunctionSpan) {
        let key = if span.impl_header.is_empty() {
            None
        } else {
            self.impl_header_stack.last().map(|frame| frame.key.clone())
        };
        self.impl_header_keys.push(key);
        self.functions.push(span);
    }

    /// Drop the impl header of every method whose name occurs more than once
    /// under the same header key: those are `cfg`-split variants, and the
    /// header cannot tell them apart.
    fn drop_repeated_impl_headers(&mut self) {
        let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
        for (span, key) in self.functions.iter().zip(&self.impl_header_keys) {
            if let Some(key) = key {
                *counts
                    .entry((key.as_str(), span.name.as_str()))
                    .or_default() += 1;
            }
        }
        let repeated: Vec<bool> = self
            .functions
            .iter()
            .zip(&self.impl_header_keys)
            .map(|(span, key)| {
                key.as_deref()
                    .is_some_and(|k| counts[&(k, span.name.as_str())] > 1)
            })
            .collect();
        for (span, repeated) in self.functions.iter_mut().zip(repeated) {
            if repeated {
                span.impl_header.clear();
                span.impl_header_path_count = None;
            }
        }
    }

    /// Header ranges and path count of the innermost enclosing impl, or none.
    fn current_impl_header(&self) -> (Vec<SourceRange>, Option<usize>) {
        match self.impl_header_stack.last() {
            Some(frame) => (frame.ranges.clone(), frame.path_count),
            None => (Vec::new(), None),
        }
    }

    /// Run `f` with `frame` as the innermost enclosing impl header.
    fn with_impl_header(&mut self, frame: ImplHeaderFrame, f: impl FnOnce(&mut Self)) {
        self.impl_header_stack.push(frame);
        f(self);
        self.impl_header_stack.pop();
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

        self.push_function(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            is_foreign: false,
            trait_required: false,
            impl_header: Vec::new(),
            impl_header_path_count: None,
        });

        self.with_impl_header(ImplHeaderFrame::default(), |v| {
            syn::visit::visit_item_fn(v, node)
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);
        let (impl_header, impl_header_path_count) = self.current_impl_header();

        self.push_function(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            is_foreign: false,
            trait_required: false,
            impl_header,
            impl_header_path_count,
        });

        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);

        self.push_function(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            is_foreign: false,
            trait_required: node.default.is_none(),
            impl_header: Vec::new(),
            impl_header_path_count: None,
        });

        syn::visit::visit_trait_item_fn(self, node);
    }

    fn visit_foreign_item_fn(&mut self, node: &'ast syn::ForeignItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.span();
        let start_line = span.start().line;
        let end_line = span.end().line;
        let cfg = self.combined_cfg(&node.attrs);

        self.push_function(FunctionSpan {
            name,
            start_line,
            end_line,
            cfg,
            is_foreign: true,
            trait_required: false,
            impl_header: Vec::new(),
            impl_header_path_count: None,
        });

        syn::visit::visit_foreign_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let mut ranges: Vec<SourceRange> = node
            .trait_
            .iter()
            .map(|(_, path, _)| SourceRange::of(path.span()))
            .collect();
        ranges.push(SourceRange::of(node.self_ty.span()));
        let key = format!(
            "{} for {}",
            node.trait_
                .as_ref()
                .map(|(_, path, _)| path.to_token_stream().to_string())
                .unwrap_or_default(),
            node.self_ty.to_token_stream()
        );
        let trait_paths = match &node.trait_ {
            Some((_, path, _)) => path_only_count(path),
            None => Some(0),
        };
        let path_count = trait_paths
            .zip(type_path_only_count(&node.self_ty))
            .map(|(a, b)| a + b);
        let frame = ImplHeaderFrame {
            ranges,
            path_count,
            key,
        };
        self.with_impl_header(frame, |v| {
            v.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_impl(v, node))
        });
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.with_impl_header(ImplHeaderFrame::default(), |v| {
            v.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_trait(v, node))
        });
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.with_impl_header(ImplHeaderFrame::default(), |v| {
            v.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_mod(v, node))
        });
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        // An `extern { … }` block can itself be cfg-gated; its gate governs
        // every member.
        self.with_enclosing_cfg(&node.attrs, |v| syn::visit::visit_item_foreign_mod(v, node));
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

/// The number of type paths in `ty` and its type arguments when `ty` is made
/// of type paths only (see [`FunctionSpan::impl_header_path_count`]), else `None`.
fn type_path_only_count(ty: &syn::Type) -> Option<usize> {
    match ty {
        syn::Type::Path(tp) if tp.qself.is_none() => path_only_count(&tp.path),
        syn::Type::Paren(inner) => type_path_only_count(&inner.elem),
        syn::Type::Group(inner) => type_path_only_count(&inner.elem),
        _ => None,
    }
}

/// One for `path` itself plus the type paths in its generic arguments, or
/// `None` when an argument is not a type path (lifetimes are ignored).
fn path_only_count(path: &syn::Path) -> Option<usize> {
    let mut count = 1;
    for segment in &path.segments {
        match &segment.arguments {
            syn::PathArguments::None => {}
            syn::PathArguments::AngleBracketed(args) => {
                for arg in &args.args {
                    match arg {
                        syn::GenericArgument::Lifetime(_) => {}
                        syn::GenericArgument::Type(ty) => count += type_path_only_count(ty)?,
                        _ => return None,
                    }
                }
            }
            syn::PathArguments::Parenthesized(_) => return None,
        }
    }
    Some(count)
}

/// Try to parse a `cfg_if!` macro node and return the items from all branches.
/// Returns `None` if the macro is not `cfg_if` or parsing fails.
pub(crate) fn try_parse_cfg_if_items(node: &syn::ItemMacro) -> Option<Vec<syn::Item>> {
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
    // A file-level inner attribute (`#![cfg(...)]`) gates every item in the
    // file, exactly like an enclosing mod gate.
    visitor.cfg_stack = cfg_predicates_of(&syntax_tree.attrs);
    visitor.visit_file(&syntax_tree);
    visitor.drop_repeated_impl_headers();

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
                        is_foreign: func.is_foreign,
                        trait_required: func.trait_required,
                        impl_header: func.impl_header,
                        impl_header_path_count: func.impl_header_path_count,
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

    // Try containment match: find a function with the same name in the same
    // file where the SCIP start_line falls within the parsed span. This is a
    // heuristic recovery path — when MORE than one same-name span contains
    // the line (nested/overlapping same-name functions), the match cannot be
    // trusted to identify the right declaration, and a wrong match would
    // assign scope-changing facts (`cfg`, `is-foreign`, `trait-required`) to
    // the wrong function. Ambiguity therefore returns `None` (counted and
    // warned as a span miss) instead of guessing.
    let mut candidates = span_map
        .iter()
        .filter(|((path, name, parsed_start), span_info)| {
            path == relative_path
                && name == bare_name
                && start_line >= *parsed_start
                && start_line <= span_info.end_line
        });
    let first = candidates.next();
    match (first, candidates.next()) {
        (Some((_, span_info)), None) => Some(span_info),
        _ => None,
    }
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

/// Get the full parsed [`SpanInfo`] for a function (cfg plus declaration-kind
/// facts), using the same matching as [`get_function_end_line`].
pub fn get_function_span_info<'a>(
    span_map: &'a HashMap<(String, String, usize), SpanInfo>,
    relative_path: &str,
    function_name: &str,
    start_line: usize,
) -> Option<&'a SpanInfo> {
    find_span_info(span_map, relative_path, function_name, start_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// A method records its impl's trait-path and self-type ranges; a free
    /// function nested in the method body, a trait method, a free function
    /// and a method repeated under the same header record none.
    #[test]
    fn test_impl_header_ranges() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "impl From<serialize::Error> for Error {{\n    fn from(v: serialize::Error) -> Self {{\n        fn helper() {{}}\n        Error::Serialization(v)\n    }}\n}}\ntrait T {{\n    fn t() {{}}\n}}\nfn free() {{}}\nimpl T for X {{\n    #[cfg(a)]\n    fn split() {{}}\n    #[cfg(not(a))]\n    fn split() {{}}\n}}\n#[cfg(a)]\nimpl T for Y {{\n    fn across() {{}}\n}}\n#[cfg(not(a))]\nimpl T for Y {{\n    fn across() {{}}\n}}\n"
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let header = |name: &str| {
            spans
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .impl_header
                .clone()
        };
        let range = |start_column, end_column| SourceRange {
            start_line: 1,
            start_column,
            end_line: 1,
            end_column,
        };
        assert_eq!(header("from"), vec![range(5, 27), range(32, 37)]);
        let paths = |name: &str| {
            spans
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .impl_header_path_count
        };
        // `From`, `serialize::Error`, `Error`.
        assert_eq!(paths("from"), Some(3));
        assert_eq!(paths("free"), None);
        assert!(header("helper").is_empty());
        assert!(header("t").is_empty());
        assert!(header("free").is_empty());
        // `cfg`-split variants share one header, in one impl block or in two,
        // so none of them records it.
        assert!(spans
            .iter()
            .filter(|s| s.name == "split" || s.name == "across")
            .all(|s| s.impl_header.is_empty() && s.impl_header_path_count.is_none()));
    }

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
    fn test_foreign_and_trait_required_facts() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
unsafe extern "C" {{
    pub fn ExternalInit();
    #[cfg(not(feature = "std"))]
    fn ExternalAlloc(
        n_bytes: usize,
    ) -> *mut u8;
}}

pub trait Cipher {{
    fn required_len(&self) -> usize;
    fn defaulted(&self) -> usize {{
        0
    }}
}}

impl Cipher for Aes {{
    fn required_len(&self) -> usize {{
        16
    }}
}}

pub fn plain() {{}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let by = |n: &str| spans.iter().filter(|s| s.name == n).collect::<Vec<_>>();

        let init = &by("ExternalInit")[0];
        assert!(init.is_foreign);
        assert!(!init.trait_required);
        assert_eq!(init.cfg, None);

        // Extern member with its own cfg gate, spanning multiple lines.
        let alloc = &by("ExternalAlloc")[0];
        assert!(alloc.is_foreign);
        assert_eq!(alloc.cfg.as_deref(), Some(r#"not (feature = "std")"#));
        assert!(alloc.end_line > alloc.start_line, "multi-line decl span");

        // Bodyless trait signature vs defaulted vs impl.
        let required = by("required_len");
        let sig = required
            .iter()
            .find(|s| s.trait_required)
            .expect("trait signature");
        assert!(!sig.is_foreign);
        let imp = required
            .iter()
            .find(|s| !s.trait_required)
            .expect("impl method");
        assert!(!imp.is_foreign);
        assert!(!by("defaulted")[0].trait_required);
        assert!(!by("plain")[0].is_foreign);
        assert!(!by("plain")[0].trait_required);
    }

    #[test]
    fn test_file_inner_cfg_gates_every_function() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"#![cfg(feature = "std")]

pub fn top() {{}}

#[cfg(test)]
fn gated_too() {{}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let by = |n: &str| spans.iter().find(|s| s.name == n).unwrap();
        assert_eq!(by("top").cfg.as_deref(), Some(r#"feature = "std""#));
        assert_eq!(
            by("gated_too").cfg.as_deref(),
            Some(r#"all(feature = "std", test)"#)
        );
    }

    #[test]
    fn test_extern_block_inside_gated_inline_mod() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
#[cfg(test)]
mod harness {{
    unsafe extern "C" {{
        pub fn HarnessInit();
    }}
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let init = spans.iter().find(|s| s.name == "HarnessInit").unwrap();
        assert!(init.is_foreign);
        assert_eq!(init.cfg.as_deref(), Some("test"));
    }

    #[test]
    fn test_defaulted_trait_fn_with_own_cfg() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
pub trait T {{
    #[cfg(feature = "extra")]
    fn defaulted(&self) -> usize {{
        0
    }}
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let d = spans.iter().find(|s| s.name == "defaulted").unwrap();
        assert!(!d.trait_required, "default body ⇒ not trait-required");
        assert_eq!(d.cfg.as_deref(), Some(r#"feature = "extra""#));
    }

    #[test]
    fn test_cfg_gated_extern_block_gates_members() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
#[cfg(feature = "ffi")]
unsafe extern "C" {{
    pub fn Bound();
}}
"#
        )
        .unwrap();

        let spans = parse_file_for_spans(file.path()).unwrap();
        let bound = spans.iter().find(|s| s.name == "Bound").unwrap();
        assert!(bound.is_foreign);
        assert_eq!(bound.cfg.as_deref(), Some(r#"feature = "ffi""#));
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
    fn test_ambiguous_containment_match_returns_none() {
        // Two same-name spans both containing the probe line: the fallback
        // cannot know which declaration the SCIP line belongs to, and a wrong
        // pick would assign scope-changing facts to the wrong function — it
        // must refuse (and be counted as a miss) rather than guess.
        let mut span_map = HashMap::new();
        span_map.insert(
            ("src/lib.rs".to_string(), "f".to_string(), 10),
            SpanInfo {
                end_line: 30,
                cfg: Some("test".to_string()),
                is_foreign: false,
                trait_required: false,
                impl_header: Vec::new(),
                impl_header_path_count: None,
            },
        );
        span_map.insert(
            ("src/lib.rs".to_string(), "f".to_string(), 12),
            SpanInfo {
                end_line: 20,
                cfg: None,
                is_foreign: false,
                trait_required: false,
                impl_header: Vec::new(),
                impl_header_path_count: None,
            },
        );

        // Line 15 is inside both spans and matches neither exact key.
        assert!(get_function_span_info(&span_map, "src/lib.rs", "f", 15).is_none());
        // An exact key still resolves.
        assert!(get_function_span_info(&span_map, "src/lib.rs", "f", 10).is_some());
        // A line contained by exactly one span resolves via fallback.
        assert!(get_function_span_info(&span_map, "src/lib.rs", "f", 25).is_some());
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
