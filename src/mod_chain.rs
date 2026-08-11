//! Module-chain analysis: configuration-independent facts about how source
//! files are mounted into their crate.
//!
//! The per-function `cfg` predicate from `rust_parser` covers a function's own
//! `#[cfg]` and enclosing same-file gates, but not the `mod` declarations in
//! *parent* files that decide whether its file is compiled at all. This module
//! supplies that missing half by walking each package's module tree from its
//! target entry files (lib, bins), following every `mod` declaration —
//! out-of-line (`mod foo;` → `foo.rs` / `foo/mod.rs`, honoring `#[path]`) and
//! inline (`mod foo { … }`) — and accumulating the `#[cfg(...)]` gates along
//! each mount chain.
//!
//! Unlike the classifier this design replaces (probe-aeneas's source walk),
//! nothing here evaluates a predicate against a feature set: probe-rust
//! reports configuration-independent source facts, and consumers decide what
//! they mean for their build. Concretely:
//!
//! - **File gate** ([`ModChainFacts::file_gate`]): for a file whose every
//!   mount chain is cfg-gated, the predicate under which the file is compiled
//!   — `any(all(chain₁…), all(chain₂…))` over its chains, unioned across all
//!   packages' walks (a file mounted through several chains is compiled when
//!   any of them is active; mutually exclusive `#[path]` remounts are the
//!   standard case). A file with at least one gate-free chain is
//!   unconditionally mounted: no gate. A file-level `#![cfg(...)]` counts as
//!   a gate on every chain mounting that file.
//! - **Unmounted** ([`ModChainFacts::unmounted`]): `src/**.rs` files that no
//!   `mod` chain from any analyzed package's lib/bin target entries reaches:
//!   not part of any lib or bin build. (Such a file may still be the root of,
//!   or `#[path]`-mounted from, a test/bench/example target — those targets
//!   are deliberately not scanned; downstream scope policy already treats
//!   them as outside the verified build.)
//!
//! The error direction is fixed toward *tracked*, enforced by one rule: **a
//! walk that was not provably complete emits nothing** — neither unmounted
//! files nor file gates, project-wide. The two facts fail in the same
//! forbidden direction: an invisible mount could make compiled code
//! "unmounted", and an invisible UNGATED mount could make every gate emitted
//! next to it over-state (downstream would grey compiled code). Completeness
//! valves: a file that cannot be read or parsed, an unresolvable `mod`
//! target, `include!`, a `cfg_attr` on a `mod` declaration (it may inject
//! `path`, selecting a different file per configuration), any item or
//! `macro_rules!` definition or unrecognized macro invocation whose tokens
//! mention `mod` (block-local `#[path] mod`, macro-generated mounts), an
//! unreadable package manifest, a mount cycle, or a chain-cap overflow.
//!
//! Within a complete walk, gates still only under-gate: `cfg_if!` branch
//! items are walked with the branch predicate dropped (their mounts count,
//! their gates don't), and a file's own `#![cfg(...)]` inner gates go to its
//! DESCENDANTS' chains only (its own functions already carry them via
//! `rust_parser`, so folding them here would double-count).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::rust_parser::{cfg_predicates_of, try_parse_cfg_if_items};
use walkdir::WalkDir;

/// A file can be re-mounted through this many distinct gate chains before the
/// walk gives up on it (and disables unmounted inference). Real code uses two
/// (mutually exclusive `#[path]` pairs); the cap only guards degenerate input.
const MAX_CHAINS_PER_FILE: usize = 8;

/// Project-level module-chain facts, keyed by path relative to the analyzed
/// project root (forward slashes, e.g. `src/sha3/tests.rs`) — the same keys
/// SCIP uses for `relative_path`.
#[derive(Debug, Default)]
pub struct ModChainFacts {
    /// File → the cfg predicate of its mount chains (unioned across every
    /// package's walk), for files whose every chain is gated. Absent for
    /// unconditionally mounted files and for files no walk reached. Like
    /// `unmounted`, only populated when every package's walk was provably
    /// complete (see the module doc for why gates share that requirement).
    pub file_gate: HashMap<String, String>,
    /// `src/**.rs` files not reached by any mount chain of any package.
    /// Only populated when every package's walk was provably complete.
    pub unmounted: HashSet<String>,
    /// Why the walk was NOT provably complete (empty ⇔ facts were emitted).
    /// One entry per distinct cause, for operator diagnostics: silent
    /// suppression would look identical to "nothing gated, nothing dead".
    pub taints: Vec<String>,
}

/// Analyze every package under `project_root` (each directory holding a
/// `Cargo.toml`, skipping `target/`, `.git/`, and `data/`) and merge their
/// module-chain facts.
///
/// The merge is cross-package on purpose: `#[path]` lets one workspace member
/// mount a file that lives under another member's `src/`, so a file's mount
/// chains are the union over ALL packages' walks, and unmounted inference is
/// only sound when EVERY package's walk was provably complete (a dirty walk
/// anywhere could hide exactly such a cross-package mount).
#[must_use]
pub fn analyze(project_root: &Path) -> ModChainFacts {
    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let package_dirs = find_package_dirs(&canonical_root);

    // Walk every package, merging the chains each file is mounted through.
    let mut all_chains: HashMap<PathBuf, Vec<(Vec<String>, bool)>> = HashMap::new();
    let mut walked_any = false;
    let mut taints: Vec<String> = Vec::new();
    for package_dir in &package_dirs {
        let (entries, manifest_ok) = package_entries(package_dir);
        if !manifest_ok {
            // An unreadable/unparsable manifest can hide a custom lib/bin
            // path: this package's files may be mounted in ways we cannot
            // see.
            taints.push(format!(
                "unreadable manifest: {}",
                package_dir.join("Cargo.toml").display()
            ));
        }
        if entries.is_empty() {
            // No lib/bin roots found, yet the package has src files: either
            // an exotic layout or a manifest we misread — its src tree must
            // not be inferred unmounted.
            let mut src = Vec::new();
            collect_rs_files(&package_dir.join("src"), &mut src);
            if !src.is_empty() {
                taints.push(format!(
                    "package without lib/bin roots but with src files: {}",
                    package_dir.display()
                ));
            }
            continue;
        }
        walked_any = true;
        let mut walker = Walker {
            chains: HashMap::new(),
            stack: Vec::new(),
            taints: Vec::new(),
        };
        // A crate root owns its directory: its `mod` children resolve as
        // siblings.
        for entry in &entries {
            walker.visit_file(entry, &[], true);
        }
        taints.extend(walker.taints);
        for (file, chains) in walker.chains {
            all_chains.entry(file).or_default().extend(chains);
        }
    }
    let all_clean = taints.is_empty();

    // Both facts require every walk to have been provably complete. For
    // unmounted that is obvious (an invisible mount could make compiled code
    // "unmounted"). For file gates it is the same forbidden direction one
    // step removed: an invisible mount could be UNGATED, so any gate emitted
    // next to it would over-state — and downstream would grey compiled code.
    // A dirty walk therefore emits nothing (the per-function `cfg` from
    // `rust_parser` is unaffected).
    let mut sorted_taints = taints;
    sorted_taints.sort();
    sorted_taints.dedup();
    let mut facts = ModChainFacts {
        taints: sorted_taints,
        ..Default::default()
    };
    if !(all_clean && walked_any) {
        return facts;
    }

    for (file, chains) in &all_chains {
        // At least one gate-free chain ⇒ unconditionally mounted, no gate.
        if chains.iter().any(|(c, _)| c.is_empty()) {
            continue;
        }
        if let Some(rel) = relative_key(&canonical_root, file) {
            let gate_chains: Vec<Vec<String>> = chains.iter().map(|(c, _)| c.clone()).collect();
            facts.file_gate.insert(rel, chains_predicate(&gate_chains));
        }
    }

    for package_dir in &package_dirs {
        let src_dir = package_dir.join("src");
        let mut all_src = Vec::new();
        collect_rs_files(&src_dir, &mut all_src);
        for file in all_src {
            let canonical = file.canonicalize().unwrap_or(file);
            if !all_chains.contains_key(&canonical) {
                if let Some(rel) = relative_key(&canonical_root, &canonical) {
                    facts.unmounted.insert(rel);
                }
            }
        }
    }
    facts
}

fn find_package_dirs(root: &Path) -> Vec<PathBuf> {
    // Same pruning as the SCIP cache's source scan: `target/` and `.git/`
    // anywhere, plus exactly the top-level `data/` cache dir.
    let cache_dir = root.join("data");
    WalkDir::new(root)
        .into_iter()
        .filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (name == "target" || name == ".git" || e.path() == cache_dir))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "Cargo.toml")
        .filter_map(|e| e.path().parent().map(Path::to_path_buf))
        .collect()
}

/// The target entry files of a package: the lib root (`[lib] path` or
/// `src/lib.rs`) and bin roots (`[[bin]] path`, `src/main.rs`,
/// `src/bin/*.rs`, `src/bin/*/main.rs`). Bench/test/example targets are
/// deliberately excluded: they live outside `src/`, and downstream scope
/// policy already treats them as non-library targets.
///
/// The second return value is `false` when the manifest could not be read or
/// parsed — the entry list may then be missing a custom lib/bin path, so the
/// caller must not trust the walk's completeness.
fn package_entries(package_dir: &Path) -> (Vec<PathBuf>, bool) {
    let mut entries = Vec::new();
    let manifest_text = std::fs::read_to_string(package_dir.join("Cargo.toml")).ok();
    let manifest = manifest_text
        .as_ref()
        .and_then(|s| s.parse::<toml::Table>().ok());
    let manifest_ok = manifest.is_some();

    let explicit_path = |section: &toml::Value| -> Option<PathBuf> {
        section
            .get("path")
            .and_then(|p| p.as_str())
            .map(|p| package_dir.join(p))
    };

    let lib_path = manifest
        .as_ref()
        .and_then(|m| m.get("lib"))
        .and_then(explicit_path)
        .unwrap_or_else(|| package_dir.join("src/lib.rs"));
    if lib_path.exists() {
        entries.push(lib_path);
    }

    if let Some(bins) = manifest
        .as_ref()
        .and_then(|m| m.get("bin"))
        .and_then(|b| b.as_array())
    {
        for bin in bins {
            if let Some(p) = explicit_path(bin) {
                if p.exists() {
                    entries.push(p);
                }
            }
        }
    }
    let main_rs = package_dir.join("src/main.rs");
    if main_rs.exists() {
        entries.push(main_rs);
    }
    if let Ok(bin_dir) = std::fs::read_dir(package_dir.join("src/bin")) {
        for entry in bin_dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rs") {
                entries.push(path);
            } else if path.is_dir() && path.join("main.rs").exists() {
                entries.push(path.join("main.rs"));
            }
        }
    }

    entries.sort();
    entries.dedup();
    (entries, manifest_ok)
}

/// Combine a file's (all-gated) mount chains into one predicate:
/// `any(all(chain₁…), all(chain₂…))` — alternatives sorted and deduplicated
/// for deterministic output, trivial arities collapsed (a single chain needs
/// no `any`, a single gate no `all`).
fn chains_predicate(chains: &[Vec<String>]) -> String {
    let mut alternatives: Vec<String> = chains
        .iter()
        .map(|gates| match gates.as_slice() {
            [single] => single.clone(),
            many => format!("all({})", many.join(", ")),
        })
        .collect();
    alternatives.sort();
    alternatives.dedup();
    match alternatives.as_slice() {
        [single] => single.clone(),
        many => format!("any({})", many.join(", ")),
    }
}

fn relative_key(base: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(base).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    // WalkDir does not follow directory symlinks by default, so a
    // `src/loop -> ../src` link cannot cause unbounded recursion.
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "rs") {
            out.push(path.to_path_buf());
        }
    }
}

/// One out-of-line `mod` declaration found in a file.
struct OutlineDecl {
    name: String,
    /// Cfg gates on this declaration plus its enclosing inline mods'.
    gates: Vec<String>,
    /// `#[path = "..."]` override, relative to the declaring file's directory.
    path_override: Option<String>,
    /// Names of enclosing inline mods (they add directory segments when
    /// resolving the child conventionally).
    inline_chain: Vec<String>,
}

struct FileScan {
    decls: Vec<OutlineDecl>,
    /// Cfg gates from the file's own inner attributes (`#![cfg(...)]`): they
    /// gate the file exactly like a gate on the `mod` declaration mounting it.
    inner_gates: Vec<String>,
    /// Completeness-taint causes (empty ⇔ clean scan).
    taints: Vec<String>,
}

fn scan_file(path: &Path) -> FileScan {
    let dirty = |cause: &str| FileScan {
        decls: Vec::new(),
        inner_gates: Vec::new(),
        taints: vec![cause.to_string()],
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return dirty("unreadable file");
    };
    let Ok(ast) = syn::parse_file(&content) else {
        return dirty("file does not parse");
    };
    let mut scanner = Scanner {
        decls: Vec::new(),
        inline: Vec::new(),
        taints: Vec::new(),
    };
    for item in &ast.items {
        scanner.visit(item);
    }
    FileScan {
        decls: scanner.decls,
        inner_gates: cfg_predicates_of(&ast.attrs),
        taints: scanner.taints,
    }
}

/// A short human label for an item kind, for taint diagnostics.
fn item_label(item: &syn::Item) -> &'static str {
    match item {
        syn::Item::Fn(_) => "fn",
        syn::Item::Impl(_) => "impl",
        syn::Item::Trait(_) => "trait",
        syn::Item::Const(_) => "const",
        syn::Item::Static(_) => "static",
        syn::Item::Struct(_) => "struct",
        syn::Item::Enum(_) => "enum",
        syn::Item::Use(_) => "use",
        syn::Item::ForeignMod(_) => "extern block",
        _ => "item",
    }
}

struct Scanner {
    decls: Vec<OutlineDecl>,
    /// Enclosing inline mods: (directory segment — the mod's `#[path]`
    /// override if present, else its name — and its own cfg gates).
    inline: Vec<(String, Vec<String>)>,
    /// Completeness-taint causes found in this file (empty ⇔ clean scan).
    taints: Vec<String>,
}

impl Scanner {
    fn visit(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Mod(m) => {
                // `cfg_attr` can conditionally inject `path` (selecting a
                // different file per configuration) or `cfg`. Neither can be
                // resolved config-independently, and a conditional `path`
                // could make this walk follow a decoy file — taint.
                if m.attrs.iter().any(|a| a.path().is_ident("cfg_attr")) {
                    self.taints.push(format!("cfg_attr on `mod {}`", m.ident));
                }
                let own_gates = cfg_predicates_of(&m.attrs);
                match &m.content {
                    Some((_, items)) => {
                        // A `#[path = "dir"]` on an INLINE module changes the
                        // directory its descendants resolve under.
                        let segment =
                            path_override_of(&m.attrs).unwrap_or_else(|| m.ident.to_string());
                        self.inline.push((segment, own_gates));
                        for it in items {
                            self.visit(it);
                        }
                        self.inline.pop();
                    }
                    None => {
                        let mut gates: Vec<String> = self
                            .inline
                            .iter()
                            .flat_map(|(_, g)| g.iter().cloned())
                            .collect();
                        gates.extend(own_gates);
                        self.decls.push(OutlineDecl {
                            name: m.ident.to_string(),
                            gates,
                            path_override: path_override_of(&m.attrs),
                            inline_chain: self.inline.iter().map(|(n, _)| n.clone()).collect(),
                        });
                    }
                }
            }
            syn::Item::Macro(mac) => {
                if mac.ident.is_some() {
                    // A `macro_rules!` definition mounts nothing by itself,
                    // but a definition whose body contains `mod` can mount
                    // modules at any later invocation site (which may not
                    // mention `mod` at all) — taint at the definition.
                    if tokens_mention_mod(mac.mac.tokens.clone()) {
                        self.taints.push(format!(
                            "macro_rules! definition mentioning `mod`: {}",
                            mac.ident
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default()
                        ));
                    }
                    return;
                }
                let last_seg = mac
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if last_seg == "include" {
                    // Spliced content we cannot see may declare modules.
                    self.taints.push("include! invocation".to_string());
                    return;
                }
                if let Some(items) = try_parse_cfg_if_items(mac) {
                    // `cfg_if!` expands one branch in place; module resolution
                    // is as if the items were written here. The branch
                    // predicates are dropped, so any `mod` found inside is
                    // under-gated (treated more compiled than it may be) —
                    // conservative — and its mount is still recorded.
                    for it in &items {
                        self.visit(it);
                    }
                    return;
                }
                // Any other item-level macro invocation whose tokens mention
                // `mod` may mount modules this walk cannot see.
                if tokens_mention_mod(mac.mac.tokens.clone()) {
                    self.taints
                        .push(format!("macro invocation mentioning `mod`: {}!", last_seg));
                }
            }
            other => {
                // Any other item whose tokens mention `mod` may hide a mount
                // this walk cannot see: a block-local `#[path] mod x;` inside
                // a function body, a mod behind an attribute/derive macro,
                // etc. The word check over the item's tokens is the same
                // audit the line scanner this module replaced used — string
                // literals containing the word trip it, which errs in the
                // conservative direction.
                use quote::ToTokens;
                if tokens_mention_mod(other.to_token_stream()) {
                    self.taints
                        .push(format!("item mentioning `mod`: {}", item_label(other)));
                }
            }
        }
    }
}

fn path_override_of(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(el) = &nv.value {
                if let syn::Lit::Str(s) = &el.lit {
                    return Some(s.value());
                }
            }
        }
        None
    })
}

/// Whether a token stream contains the `mod` keyword as an actual token,
/// recursing into groups. Literals (doc comments, strings) are skipped — the
/// word "mod" in prose or a string cannot mount a module, and doc comments
/// like "(a * b) mod Q" are everywhere in real crates.
fn tokens_mention_mod(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|tree| match tree {
        proc_macro2::TokenTree::Ident(ident) => ident == "mod",
        proc_macro2::TokenTree::Group(group) => tokens_mention_mod(group.stream()),
        _ => false,
    })
}

struct Walker {
    /// Canonical file → the mount chains it was walked under, each with the
    /// directory-ownership mode of that mount. The gate chain deliberately
    /// EXCLUDES the file's own `#![cfg(...)]` inner gates (those are already
    /// part of every function's own `cfg` via `rust_parser`; duplicating them
    /// here would fold the same gate twice and misattribute it to `file-cfg`).
    /// Descendants' chains DO include them — a gated file's children are
    /// gated too.
    chains: HashMap<PathBuf, Vec<(Vec<String>, bool)>>,
    /// Files on the current recursion path (cycle guard).
    stack: Vec<PathBuf>,
    /// Causes for which a mount could have been missed (empty ⇔ provably
    /// complete). Any entry disables unmounted inference AND all file-gate
    /// emission for the whole project (an unseen mount could be ungated, so
    /// every gate fact becomes unreliable in the forbidden direction).
    taints: Vec<String>,
}

impl Walker {
    fn visit_file(&mut self, file: &Path, chain: &[String], dir_owner: bool) {
        let Ok(canonical) = file.canonicalize() else {
            self.taints
                .push(format!("cannot canonicalize {}", file.display()));
            return;
        };
        if self.stack.contains(&canonical) {
            // A mount cycle (possible with `#[path]`) — rustc rejects these,
            // but the walk must still terminate.
            self.taints
                .push(format!("mount cycle at {}", canonical.display()));
            return;
        }

        let known = self.chains.entry(canonical.clone()).or_default();
        // A previously walked chain whose gates are a subset of this one AND
        // whose directory-ownership mode matches makes the new mount
        // redundant: the file's gate is already at least as permissive, and
        // its children resolved to the same places. A different ownership
        // mode resolves children differently, so it must be walked even with
        // identical gates.
        let chain_vec = chain.to_vec();
        if known.iter().any(|(prev, prev_owner)| {
            *prev_owner == dir_owner && prev.iter().all(|g| chain_vec.contains(g))
        }) {
            return;
        }
        if known.len() >= MAX_CHAINS_PER_FILE {
            // Do NOT record the overflow chain: a recorded-but-unwalked chain
            // would let the file's gate claim more mounts than its
            // descendants actually got. The taint suppresses all gate facts
            // anyway.
            self.taints
                .push(format!("chain cap exceeded at {}", canonical.display()));
            return;
        }
        known.push((chain_vec.clone(), dir_owner));

        let scan = scan_file(&canonical);
        // A dirty scan means this file may mount modules invisibly (an
        // unrecognized macro, a block-local `#[path] mod`): the taint
        // suppresses ALL fact emission in `analyze`. The visible mounts are
        // still walked so the recursion covers everything it can see.
        for cause in &scan.taints {
            self.taints
                .push(format!("{}: {}", canonical.display(), cause));
        }

        // A file-level `#![cfg(...)]` gates everything inside the file,
        // including the modules it mounts — descendants inherit it. (The
        // file's own recorded chain above deliberately does not: see the
        // `chains` field doc.)
        let mut child_base = chain_vec;
        child_base.extend(scan.inner_gates.iter().cloned());

        self.stack.push(canonical.clone());
        for decl in &scan.decls {
            let mut child_chain = child_base.clone();
            child_chain.extend(decl.gates.iter().cloned());
            match resolve_mod_file(&canonical, decl, dir_owner) {
                Some((child, child_owns_dir)) => {
                    self.visit_file(&child, &child_chain, child_owns_dir);
                }
                None => {
                    // Unresolvable target (macro-provided, missing file): its
                    // subtree cannot be safely inferred unmounted.
                    self.taints.push(format!(
                        "unresolvable `mod {}` in {}",
                        decl.name,
                        canonical.display()
                    ));
                }
            }
        }
        self.stack.pop();
    }
}

/// Resolve the file an out-of-line `mod` declaration refers to, and whether
/// that file owns its directory for *its* children.
///
/// rustc's directory-ownership rule: children of a directory-owning file
/// (crate root, `mod.rs`, or a `#[path]`-mounted file) live in the file's own
/// directory; children of a conventionally mounted `foo.rs` live under
/// `foo/`. Inline enclosing mods add further path segments (a `#[path]` on an
/// inline mod replaces its segment — handled by the scanner).
///
/// `#[path]` overrides: at the top level of a file they resolve relative to
/// the file's directory; inside inline modules they resolve relative to the
/// directory the file's conventional children use, plus the inline segments
/// (the rustc rule that differs between mod-rs and non-mod-rs files). Either
/// way the mounted file becomes a directory owner.
fn resolve_mod_file(
    declaring_file: &Path,
    decl: &OutlineDecl,
    declarer_owns_dir: bool,
) -> Option<(PathBuf, bool)> {
    let dir = declaring_file.parent()?;
    let stem = declaring_file.file_stem()?.to_string_lossy().to_string();
    if let Some(over) = &decl.path_override {
        let p = if decl.inline_chain.is_empty() {
            dir.join(over)
        } else {
            let mut base = if declarer_owns_dir {
                dir.to_path_buf()
            } else {
                dir.join(&stem)
            };
            for seg in &decl.inline_chain {
                base = base.join(seg);
            }
            base.join(over)
        };
        return p.exists().then_some((p, true));
    }
    let mut base = if declarer_owns_dir {
        dir.to_path_buf()
    } else {
        dir.join(&stem)
    };
    for seg in &decl.inline_chain {
        base = base.join(seg);
    }
    let candidate_rs = base.join(format!("{}.rs", decl.name));
    if candidate_rs.exists() {
        return Some((candidate_rs, false));
    }
    let candidate_mod = base.join(&decl.name).join("mod.rs");
    if candidate_mod.exists() {
        return Some((candidate_mod, true));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn package(root: &Path) {
        write(
            root,
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        );
    }

    #[test]
    fn cross_package_path_mount_is_not_unmounted() {
        // `#[path]` can mount a file living under ANOTHER workspace member's
        // src/. Member b never mounts its own shared.rs, but member a does:
        // the file is compiled, so it must not be labeled unmounted.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
        );
        write(
            root,
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        );
        write(
            root,
            "a/src/lib.rs",
            "#[path = \"../../b/src/shared.rs\"]\nmod shared;\n",
        );
        write(
            root,
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n",
        );
        write(root, "b/src/lib.rs", "pub fn b() {}\n");
        write(root, "b/src/shared.rs", "pub fn s() {}\n");

        let facts = analyze(root);
        assert!(
            !facts.unmounted.contains("b/src/shared.rs"),
            "cross-package mount must count: {facts:?}"
        );
    }

    #[test]
    fn cross_package_gated_mount_unions_with_own_ungated_mount() {
        // Member a mounts b's file under cfg(test); member b mounts the same
        // file unconditionally. The union has a gate-free chain: no gate.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
        );
        write(
            root,
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        );
        write(
            root,
            "a/src/lib.rs",
            "#[cfg(test)]\n#[path = \"../../b/src/shared.rs\"]\nmod shared;\n",
        );
        write(
            root,
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n",
        );
        write(root, "b/src/lib.rs", "mod shared;\n");
        write(root, "b/src/shared.rs", "pub fn s() {}\n");

        let facts = analyze(root);
        assert!(
            !facts.file_gate.contains_key("b/src/shared.rs"),
            "gate-free own mount must win: {facts:?}"
        );
    }

    #[test]
    fn mount_cycle_terminates_and_disables_unmounted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "#[path = \"x.rs\"]\nmod x;\n");
        write(root, "src/x.rs", "#[path = \"y.rs\"]\nmod y;\n");
        write(root, "src/y.rs", "#[path = \"x.rs\"]\nmod x_again;\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        // Termination is the main assertion (the walk returned at all);
        // the cycle also poisons unmounted inference.
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn chain_explosion_cap_disables_unmounted() {
        // One file re-mounted through more distinct gate chains than the cap:
        // the walk gives up on it and disables unmounted inference.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        let mut lib = String::new();
        for i in 0..(MAX_CHAINS_PER_FILE + 1) {
            lib.push_str(&format!(
                "#[cfg(feature = \"f{i}\")]\n#[path = \"shared.rs\"]\nmod m{i};\n"
            ));
        }
        write(root, "src/lib.rs", &lib);
        write(root, "src/shared.rs", "pub fn s() {}\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        // The overflow taints the walk: chains past the cap were not walked,
        // so BOTH facts are suppressed (a recorded-but-unwalked weaker chain
        // could otherwise leave the file's descendants over-gated).
        assert!(facts.unmounted.is_empty(), "{facts:?}");
        assert!(facts.file_gate.is_empty(), "{facts:?}");
    }

    #[test]
    fn inline_mod_shadowing_a_file_leaves_it_unmounted() {
        // `mod foo { … }` inline plus an unrelated src/foo.rs on disk: rustc
        // never reads the file.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "mod foo {\n    pub fn f() {}\n}\n");
        write(root, "src/foo.rs", "pub fn shadowed() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.contains("src/foo.rs"), "{facts:?}");
    }

    #[test]
    fn dirty_file_suppresses_all_fact_emission() {
        // A file containing an unrecognized mod-mentioning macro might mount
        // modules invisibly (possibly ungated). An invisible ungated mount
        // would make ANY emitted gate over-state, so a dirty walk emits
        // nothing at all.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "mount_modules! { mod hidden; }\n#[cfg(test)]\nmod tests;\n",
        );
        write(root, "src/tests.rs", "pub fn t() {}\n");

        let facts = analyze(root);
        assert!(
            !facts.file_gate.contains_key("src/tests.rs"),
            "dirty file's gates must be stripped: {facts:?}"
        );
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn file_inner_cfg_gates_descendants_not_own_file_gate() {
        // `#![cfg(...)]` at the top of a file gates its whole subtree. The
        // file's OWN entry gets no `file-cfg` for it — that gate already
        // reaches its functions via their per-function `cfg` (rust_parser
        // seeds the visitor with `File::attrs`), and duplicating it here
        // would fold the same gate twice and misattribute it as a parent
        // mount gate. Descendant files DO inherit it as a chain gate.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "mod imp;\n");
        write(
            root,
            "src/imp.rs",
            "#![cfg(feature = \"x\")]\nmod deep;\npub fn i() {}\n",
        );
        write(root, "src/imp/deep.rs", "pub fn d() {}\n");

        let facts = analyze(root);
        assert!(!facts.file_gate.contains_key("src/imp.rs"), "{facts:?}");
        assert_eq!(
            facts.file_gate.get("src/imp/deep.rs").map(String::as_str),
            Some("feature = \"x\"")
        );
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn path_override_inside_inline_module_resolves_with_inline_segments() {
        // rustc: a `#[path]` inside an inline module resolves relative to the
        // directory the file's conventional children use, plus the inline-mod
        // segments. From src/a.rs (non-mod-rs), `mod inline { #[path =
        // "other.rs"] mod inner; }` reads src/a/inline/other.rs.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "mod a;\n");
        write(
            root,
            "src/a.rs",
            "mod inline {\n    #[path = \"other.rs\"]\n    mod inner;\n}\n",
        );
        write(root, "src/a/inline/other.rs", "pub fn o() {}\n");
        // Decoy at the (wrong) top-level-relative location: must NOT be the
        // one that gets mounted.
        write(root, "src/other.rs", "pub fn decoy() {}\n");

        let facts = analyze(root);
        assert!(
            !facts.unmounted.contains("src/a/inline/other.rs"),
            "the real target must be reached: {facts:?}"
        );
        assert!(
            facts.unmounted.contains("src/other.rs"),
            "the decoy is NOT mounted: {facts:?}"
        );
    }

    #[test]
    fn path_override_on_inline_module_changes_child_directory() {
        // rustc: `#[path = "threads"] mod thread { #[path = "tls.rs"] mod
        // local_data; }` reads threads/tls.rs — the inline mod's own #[path]
        // replaces its directory segment.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "#[path = \"threads\"]\nmod thread {\n    #[path = \"tls.rs\"]\n    mod local_data;\n}\n",
        );
        write(root, "src/threads/tls.rs", "pub fn t() {}\n");

        let facts = analyze(root);
        assert!(!facts.unmounted.contains("src/threads/tls.rs"), "{facts:?}");
    }

    #[test]
    fn cfg_attr_on_mod_taints_the_walk() {
        // `#[cfg_attr(cond, path = "...")]` selects a different file per
        // configuration; this walk cannot resolve it config-independently and
        // could follow a decoy — both facts must be suppressed.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "#[cfg_attr(target_os = \"linux\", path = \"linux.rs\")]\nmod os;\n",
        );
        write(root, "src/os.rs", "pub fn generic() {}\n");
        write(root, "src/linux.rs", "pub fn linux() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
        assert!(facts.file_gate.is_empty(), "{facts:?}");
    }

    #[test]
    fn macro_rules_definition_mounting_mods_taints_the_walk() {
        // The invocation `mount!(hidden)` contains no `mod` token — the
        // mount is hidden in the macro_rules DEFINITION body, which must
        // taint the walk on its own.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "macro_rules! mount {\n    ($name:ident) => { mod $name; };\n}\nmount!(hidden);\n",
        );
        write(root, "src/hidden.rs", "pub fn h() {}\n");

        let facts = analyze(root);
        assert!(
            facts.unmounted.is_empty(),
            "hidden.rs is compiled — must not be inferred unmounted: {facts:?}"
        );
    }

    #[test]
    fn mod_in_doc_comments_and_strings_does_not_taint() {
        // "mod" as prose (doc comments, string literals) cannot mount
        // modules and must not cost a project its facts.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "/// Montgomery multiply: (a * b) / R mod Q.\npub fn mont(a: u32) -> u32 {\n    let _s = \"mod tests\";\n    a\n}\n#[cfg(test)]\nmod tests;\n",
        );
        write(root, "src/tests.rs", "pub fn t() {}\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert!(facts.taints.is_empty(), "{facts:?}");
        assert_eq!(
            facts.file_gate.get("src/tests.rs").map(String::as_str),
            Some("test")
        );
        assert!(facts.unmounted.contains("src/orphan.rs"), "{facts:?}");
    }

    #[test]
    fn block_local_path_mod_taints_the_walk() {
        // Items can be declared inside function bodies; an out-of-line mod
        // there is legal with a `#[path]`. The walk does not resolve it, so
        // its presence must taint completeness.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "pub fn f() {\n    #[path = \"hidden.rs\"]\n    mod hidden;\n}\n",
        );
        write(root, "src/hidden.rs", "pub fn h() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn same_file_mounted_with_both_ownership_modes_walks_both() {
        // The same physical file mounted conventionally (children under its
        // stem dir) AND via #[path] (children as siblings): equal gate chains
        // must not prune the second walk, or the second child tree would be
        // missed and falsely unmounted.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "mod shared;\n#[path = \"shared.rs\"]\nmod shared_alias;\n",
        );
        write(root, "src/shared.rs", "mod child;\n");
        // Conventional mount resolves child under src/shared/, the #[path]
        // mount resolves it as a sibling in src/.
        write(root, "src/shared/child.rs", "pub fn a() {}\n");
        write(root, "src/child.rs", "pub fn b() {}\n");

        let facts = analyze(root);
        assert!(
            !facts.unmounted.contains("src/shared/child.rs"),
            "{facts:?}"
        );
        assert!(!facts.unmounted.contains("src/child.rs"), "{facts:?}");
    }

    #[test]
    fn gated_and_unmounted_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "pub mod verify;\n#[cfg(all(test, not(feature = \"benchmarking\")))]\nmod test_helpers;\n#[cfg(not(feature = \"verify\"))]\nmod ffi;\n",
        );
        write(root, "src/verify.rs", "pub fn f() {}\n");
        write(root, "src/test_helpers.rs", "pub fn h() {}\n");
        write(root, "src/ffi.rs", "pub fn c() {}\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts
                .file_gate
                .get("src/test_helpers.rs")
                .map(String::as_str),
            Some("all (test , not (feature = \"benchmarking\"))")
        );
        assert_eq!(
            facts.file_gate.get("src/ffi.rs").map(String::as_str),
            Some("not (feature = \"verify\")")
        );
        assert!(!facts.file_gate.contains_key("src/verify.rs"));
        assert!(facts.unmounted.contains("src/orphan.rs"), "{facts:?}");
        assert!(!facts.unmounted.contains("src/verify.rs"));
    }

    #[test]
    fn nested_dirs_and_inline_mods() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "mod outer;\n");
        // Children of a non-root file live under its stem directory; an
        // inline cfg-gated mod gates the out-of-line decl inside it.
        write(
            root,
            "src/outer.rs",
            "pub mod sub;\n#[cfg(test)]\nmod tests {\n    mod fixtures;\n}\n",
        );
        write(root, "src/outer/sub.rs", "pub fn s() {}\n");
        write(root, "src/outer/tests/fixtures.rs", "pub fn fx() {}\n");

        let facts = analyze(root);
        assert!(!facts.file_gate.contains_key("src/outer/sub.rs"));
        assert_eq!(
            facts
                .file_gate
                .get("src/outer/tests/fixtures.rs")
                .map(String::as_str),
            Some("test")
        );
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn path_override_and_mod_rs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "mod a;\n#[path = \"weird_name.rs\"]\nmod b;\n",
        );
        write(root, "src/a/mod.rs", "pub fn a() {}\n");
        write(root, "src/weird_name.rs", "pub fn b() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
        assert!(facts.file_gate.is_empty(), "{facts:?}");
    }

    #[test]
    fn path_mounted_file_owns_its_directory() {
        // The SymCRust layout: `#[path = "sha3/sha3.rs"] pub mod sha3;` makes
        // sha3/sha3.rs a directory owner — its children (`mod tests;`,
        // `mod ffi;`) resolve as SIBLINGS in sha3/, not under sha3/sha3/.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "#[path = \"sha3/sha3.rs\"]\npub mod sha3;\n",
        );
        write(
            root,
            "src/sha3/sha3.rs",
            "#[cfg(all(test, not(feature = \"benchmarking\")))]\nmod tests;\n#[cfg(not(feature = \"verify\"))]\nmod ffi;\npub(crate) mod sha3_impl;\n",
        );
        write(root, "src/sha3/tests.rs", "pub fn t() {}\n");
        write(root, "src/sha3/ffi.rs", "pub fn c() {}\n");
        write(root, "src/sha3/sha3_impl.rs", "pub fn imp() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts.file_gate.get("src/sha3/tests.rs").map(String::as_str),
            Some("all (test , not (feature = \"benchmarking\"))")
        );
        assert_eq!(
            facts.file_gate.get("src/sha3/ffi.rs").map(String::as_str),
            Some("not (feature = \"verify\")")
        );
        assert!(!facts.file_gate.contains_key("src/sha3/sha3_impl.rs"));
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn unresolvable_mod_disables_unmounted_inference() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        // `mod ghost;` has no file: possibly macro-provided — the walk is not
        // provably complete, so nothing may be inferred unmounted.
        write(root, "src/lib.rs", "mod ghost;\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn mutually_exclusive_path_mounts_union_their_gates() {
        // The standard `#[path]` pattern: the same file mounted under
        // `cfg(test)` and `cfg(not(test))`. Config-independently the file's
        // gate is `any(test, not(test))` — but we cannot simplify tautologies,
        // and downstream evaluation handles the any() just fine.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "#[cfg(test)]\n#[path = \"shared/mod.rs\"]\nmod imp;\n#[cfg(not(test))]\n#[path = \"shared/mod.rs\"]\nmod imp;\n",
        );
        write(root, "src/shared/mod.rs", "pub mod child;\n");
        write(root, "src/shared/child.rs", "pub fn c() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts.file_gate.get("src/shared/mod.rs").map(String::as_str),
            Some("any(not (test), test)")
        );
        assert_eq!(
            facts
                .file_gate
                .get("src/shared/child.rs")
                .map(String::as_str),
            Some("any(not (test), test)")
        );
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn gate_free_chain_wins_over_gated_remount() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        // Mounted both unconditionally and under cfg(test): unconditional wins.
        write(
            root,
            "src/lib.rs",
            "mod shared;\n#[cfg(test)]\n#[path = \"shared.rs\"]\nmod shared_for_tests;\n",
        );
        write(root, "src/shared.rs", "pub fn s() {}\n");

        let facts = analyze(root);
        assert!(facts.file_gate.is_empty(), "{facts:?}");
    }

    #[test]
    fn cfg_if_mounts_are_seen_ungated() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "cfg_if! {\n    if #[cfg(feature = \"fast\")] {\n        mod fast;\n    } else {\n        mod slow;\n    }\n}\n",
        );
        write(root, "src/fast.rs", "pub fn f() {}\n");
        write(root, "src/slow.rs", "pub fn s() {}\n");

        let facts = analyze(root);
        // Branch predicates are dropped (conservative): mounted, ungated.
        assert!(facts.file_gate.is_empty(), "{facts:?}");
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn include_macro_disables_unmounted_inference() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "include!(\"generated.rs\");\npub fn f() {}\n",
        );
        write(root, "src/orphan.rs", "pub fn maybe_included() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn unknown_macro_mentioning_mod_disables_unmounted_inference() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/lib.rs",
            "mount_modules! { mod hidden; }\npub fn f() {}\n",
        );
        write(root, "src/hidden.rs", "pub fn h() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }

    #[test]
    fn unparsable_file_disables_unmounted_inference() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(root, "src/lib.rs", "mod broken;\nmod good;\n");
        write(root, "src/broken.rs", "this is not rust {{{\n");
        write(root, "src/good.rs", "pub fn g() {}\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert!(facts.unmounted.is_empty(), "{facts:?}");
        // The broken file is still known-mounted (no gate, reached cleanly).
        assert!(facts.file_gate.is_empty(), "{facts:?}");
    }

    #[test]
    fn bin_only_package_walks_from_main() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        package(root);
        write(
            root,
            "src/main.rs",
            "#[cfg(test)]\nmod tests;\nmod app;\nfn main() {}\n",
        );
        write(root, "src/app.rs", "pub fn run() {}\n");
        write(root, "src/tests.rs", "pub fn t() {}\n");
        write(root, "src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts.file_gate.get("src/tests.rs").map(String::as_str),
            Some("test")
        );
        assert!(!facts.file_gate.contains_key("src/app.rs"));
        assert!(facts.unmounted.contains("src/orphan.rs"), "{facts:?}");
    }

    #[test]
    fn workspace_members_analyzed_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
        );
        write(
            root,
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        );
        write(root, "a/src/lib.rs", "#[cfg(test)]\nmod tests;\n");
        write(root, "a/src/tests.rs", "pub fn t() {}\n");
        write(
            root,
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n",
        );
        write(root, "b/src/lib.rs", "pub fn b() {}\n");
        write(root, "b/src/orphan.rs", "pub fn dead() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts.file_gate.get("a/src/tests.rs").map(String::as_str),
            Some("test")
        );
        assert!(facts.unmounted.contains("b/src/orphan.rs"), "{facts:?}");
        assert!(!facts.unmounted.contains("a/src/tests.rs"));
    }

    #[test]
    fn lib_path_override_in_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"src/custom_lib.rs\"\n",
        );
        write(root, "src/custom_lib.rs", "#[cfg(test)]\nmod tests;\n");
        write(root, "src/tests.rs", "pub fn t() {}\n");

        let facts = analyze(root);
        assert_eq!(
            facts.file_gate.get("src/tests.rs").map(String::as_str),
            Some("test")
        );
        assert!(facts.unmounted.is_empty(), "{facts:?}");
    }
}
