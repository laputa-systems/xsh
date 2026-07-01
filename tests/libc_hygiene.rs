//! Migration-hygiene lint: flags calls into a crate you are migrating *away*
//! from (`FROM_CRATE`, e.g. `libc`) when the crate you are migrating *to*
//! (`TO_CRATE`, e.g. `rustix`) already provides an equivalent.
//!
//! This is intentionally generic — to reuse it in another repo, copy this file
//! into `tests/` and edit the CONFIGURATION block below (the two crate names,
//! the skip list, and the `RENAMES`/`ALLOW` tables). It needs `miniserde`,
//! `syn` (features `full`, `visit`), and `walkdir` as dev-dependencies, and it
//! assumes the audited crate's sources live under `$CARGO_MANIFEST_DIR/src`.
//!
//! How it works:
//!   1. `cargo metadata` locates `TO_CRATE`'s checked-out source.
//!   2. Its whole `src/` tree (minus `TO_SKIP_DIRS`) is parsed with `syn`,
//!      collecting every `pub fn` name — free functions *and* associated
//!      functions in `impl` blocks — plus any `#[doc(alias = "...")]` they carry
//!      (the convention rustix uses to name the C symbol a function replaces).
//!      Those names form the MIGRATABLE set.
//!   3. `RENAMES` adds mappings auto-discovery can't infer (different name with
//!      no alias, or one API replacing several C functions).
//!   4. `ALLOW` removes symbols deliberately kept; each entry needs a reason.
//!
//! What it enforces: no reference to a MIGRATABLE `FROM_CRATE` symbol in `src/`.
//! That covers `FROM::X` paths, `use FROM as alias; alias::X`, `use FROM::X`
//! (the import itself is flagged), and bare calls to a `use FROM::X` import.
//! A violation fails the test — migrate the call, or add it to `ALLOW`/`RENAMES`.
//!
//! Limits worth knowing: matching is by *name*, so it cannot tell a function
//! call from a same-named type/constant (handle via `ALLOW`); it sees `pub fn`s
//! regardless of `#[cfg]`, so a function gated off your target still counts; and
//! it only knows the `TO_CRATE` version that `cargo metadata` resolved.
use miniserde::json::Value as JsonValue;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command as Cmd;
use syn::visit::{self, Visit};
use syn::{File, UseTree};
use walkdir::WalkDir;

// ──────────────────────────── CONFIGURATION ────────────────────────────────
// Everything a different repo needs to change lives in this block.

/// Crate being migrated away from; its `FROM_CRATE::sym` uses are audited.
const FROM_CRATE: &str = "libc";

/// Crate being migrated to; its public API defines what counts as migratable.
const TO_CRATE: &str = "rustix";

/// First-path-components under `TO_CRATE/src` to skip (a `.rs` suffix is
/// ignored, so `"runtime"` skips both `runtime/` and `runtime.rs`). These hold
/// internal or unstable APIs that are not real migration targets.
const TO_SKIP_DIRS: &[&str] = &["backend", "maybe_polyfill", "runtime"];

// Renames: FROM_CRATE symbols whose TO_CRATE equivalent can't be auto-discovered
// because the name differs or one API replaces several C functions.
const RENAMES: &[(&str, &str)] = &[
    ("opendir", "rustix::fs::Dir::read_from()"),
    ("readdir", "rustix::fs::Dir iteration"),
    ("closedir", "rustix::fs::Dir (auto-close on drop)"),
    ("stat", "rustix::fs::statat(CWD, path, AtFlags::empty())"),
    (
        "lstat",
        "rustix::fs::statat(CWD, path, AtFlags::SYMLINK_NOFOLLOW)",
    ),
    (
        "mknod",
        "rustix::fs::mknodat(CWD, path, FileType, Mode, dev)",
    ),
];

// Allow: symbols that ARE discovered as migratable but are deliberately
// retained. Each needs a reason. Add entries only after confirming the
// TO_CRATE equivalent genuinely does not fit.
const ALLOW: &[(&str, &str)] = &[
    // `libc::statfs` is used as a *type* for the macOS getmntinfo(3) mount
    // enumeration; rustix's `statfs` is the statfs(2) syscall (a function) and
    // does not wrap getmntinfo, so there is no equivalent.
    (
        "statfs",
        "macOS getmntinfo(3) struct type — no rustix equivalent",
    ),
    // `libc::ioctl` is used with bespoke request codes (SIOCGIFFLAGS, etc.);
    // rustix's `ioctl` is an unsafe typed framework requiring an `Ioctl` impl
    // per request, which is not a drop-in for these one-off calls.
    (
        "ioctl",
        "raw ioctl request codes — rustix's typed ioctl is not a drop-in",
    ),
    // The STD*_FILENO are integer fd *constants* used as raw fd numbers (struct
    // fields, `raw_fd <= STDERR_FILENO`); rustix's `stdio::{stdin,stdout,stderr}`
    // — which carry these as doc aliases — return a `BorrowedFd`, not the number.
    (
        "STDIN_FILENO",
        "integer fd constant — rustix stdio::stdin() returns a BorrowedFd",
    ),
    (
        "STDOUT_FILENO",
        "integer fd constant — rustix stdio::stdout() returns a BorrowedFd",
    ),
    (
        "STDERR_FILENO",
        "integer fd constant — rustix stdio::stderr() returns a BorrowedFd",
    ),
];

// ─────────────────────────── target discovery ──────────────────────────────

fn to_crate_src_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Cmd::new(cargo)
        .args(["metadata", "--format-version=1"])
        .current_dir(manifest_dir)
        .output()
        .expect("cargo metadata failed");

    let text = String::from_utf8(output.stdout).expect("cargo metadata is not UTF-8");
    let json: JsonValue =
        miniserde::json::from_str(&text).expect("cargo metadata is not valid JSON");
    let packages = json_array(json_field(&json, "packages"));
    let target = packages
        .iter()
        .find(|p| json_str(json_field(p, "name")) == TO_CRATE)
        .unwrap_or_else(|| panic!("{TO_CRATE} not found in cargo metadata"));
    let manifest = json_str(json_field(target, "manifest_path"));

    Path::new(manifest)
        .parent()
        .expect("manifest has no parent directory")
        .join("src")
}

fn json_field<'a>(value: &'a JsonValue, key: &str) -> &'a JsonValue {
    match value {
        JsonValue::Object(fields) => fields.get(key).unwrap_or_else(|| panic!("missing {key}")),
        _ => panic!("expected JSON object"),
    }
}

fn json_array(value: &JsonValue) -> &miniserde::json::Array {
    match value {
        JsonValue::Array(items) => items,
        _ => panic!("expected JSON array"),
    }
}

fn json_str(value: &JsonValue) -> &str {
    match value {
        JsonValue::String(value) => value,
        _ => panic!("expected JSON string"),
    }
}

// Collect every libc symbol named by a `#[doc(alias = ...)]` attribute. Handles
// all three shapes rustix uses:
//   #[doc(alias = "x")]
//   #[doc(alias = "x", alias = "y")]   (comma-separated name/values)
//   #[doc(alias("x", "y"))]            (alias as a list)
fn extract_doc_aliases(attr: &syn::Attribute) -> Vec<String> {
    let syn::Meta::List(ml) = &attr.meta else {
        return Vec::new();
    };
    if !ml.path.is_ident("doc") {
        return Vec::new();
    }
    let Ok(items) = ml.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return Vec::new();
    };
    let mut aliases = Vec::new();
    for item in items {
        match item {
            syn::Meta::NameValue(nv) if nv.path.is_ident("alias") => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = nv.value
                {
                    aliases.push(s.value());
                }
            }
            syn::Meta::List(list) if list.path.is_ident("alias") => {
                if let Ok(strs) = list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated,
                ) {
                    aliases.extend(strs.into_iter().map(|s| s.value()));
                }
            }
            _ => {}
        }
    }
    aliases
}

struct PubFnCollector {
    module_prefix: String,
    discovered: Vec<(String, String)>,
}

impl PubFnCollector {
    fn record(&mut self, fn_name: &str, attrs: &[syn::Attribute]) {
        let suggestion = format!("{}::{}()", self.module_prefix, fn_name);
        self.discovered
            .push((fn_name.to_string(), suggestion.clone()));
        for attr in attrs {
            for alias in extract_doc_aliases(attr) {
                self.discovered.push((alias, suggestion.clone()));
            }
        }
    }
}

impl<'ast> Visit<'ast> for PubFnCollector {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if matches!(item.vis, syn::Visibility::Public(_)) {
            self.record(&item.sig.ident.to_string(), &item.attrs);
        }
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if matches!(item.vis, syn::Visibility::Public(_)) {
            self.record(&item.sig.ident.to_string(), &item.attrs);
        }
        visit::visit_impl_item_fn(self, item);
    }
}

fn collect_pub_fns(path: &Path, prefix: &str) -> Vec<(String, String)> {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "libc_hygiene: WARNING — could not read {TO_CRATE} module {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };
    let ast = match syn::parse_file(&source) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "libc_hygiene: WARNING — could not parse {TO_CRATE} module {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };
    let mut collector = PubFnCollector {
        module_prefix: prefix.to_string(),
        discovered: Vec::new(),
    };
    collector.visit_file(&ast);
    collector.discovered
}

fn rust_files_under(path: &Path) -> Vec<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .map(|e| e.into_path())
        .collect()
}

/// Whether a file (given relative to the target crate's `src/`) is in a skipped
/// area. Matches the first path component with any `.rs` suffix stripped, so
/// both `runtime/` and `runtime.rs` are covered by a `"runtime"` entry.
fn is_skipped(rel: &Path) -> bool {
    let Some(Component::Normal(first)) = rel.components().next() else {
        return false;
    };
    let name = first.to_string_lossy();
    let stem = name.strip_suffix(".rs").unwrap_or(&name);
    TO_SKIP_DIRS.contains(&stem)
}

/// Advisory module path for a suggestion, derived from the file's location:
/// the top-level component under `src/` (`net/types.rs` → `rustix::net`,
/// `system.rs` → `rustix::system`). Affects hint text only, never pass/fail.
fn module_prefix(rel: &Path) -> String {
    match rel.components().next() {
        Some(Component::Normal(first)) => {
            let name = first.to_string_lossy();
            let stem = name.strip_suffix(".rs").unwrap_or(&name);
            if stem == "lib" || stem == "mod" {
                TO_CRATE.to_string()
            } else {
                format!("{TO_CRATE}::{stem}")
            }
        }
        _ => TO_CRATE.to_string(),
    }
}

fn build_migratable(src_dir: &Path) -> Vec<(String, String)> {
    if !src_dir.is_dir() {
        eprintln!(
            "libc_hygiene: WARNING — {TO_CRATE} src dir not found: {}",
            src_dir.display()
        );
    }

    let mut migratable: Vec<(String, String)> = Vec::new();
    for file in rust_files_under(src_dir) {
        let rel = file.strip_prefix(src_dir).unwrap_or(&file);
        if is_skipped(rel) {
            continue;
        }
        migratable.extend(collect_pub_fns(&file, &module_prefix(rel)));
    }

    // Deduplicate by symbol name — keep first occurrence.
    let mut seen = FxHashSet::default();
    migratable.retain(|(name, _)| seen.insert(name.clone()));

    migratable.extend(RENAMES.iter().map(|(n, s)| (n.to_string(), s.to_string())));

    // Drop deliberately-retained symbols.
    let allow: FxHashSet<&str> = ALLOW.iter().map(|(s, _)| *s).collect();
    migratable.retain(|(name, _)| !allow.contains(name.as_str()));

    migratable
}

// ── import tracking ─────────────────────────────────────────────────────────

/// Per-file record of how `FROM_CRATE` is brought into scope, so we can catch
/// usages that don't go through a literal `FROM_CRATE::` path.
#[derive(Default)]
struct Imports {
    /// Path roots that resolve to `FROM_CRATE` (always includes `FROM_CRATE`
    /// itself; extended by `use <FROM_CRATE> as alias`).
    crate_aliases: FxHashSet<String>,
    /// Bare local name → symbol, from `use <FROM>::sym` / `use <FROM>::sym as local`.
    bare: FxHashMap<String, String>,
    /// Violations discovered while walking `use` trees (e.g. importing a
    /// migratable symbol, or a `use libc::*` glob that defeats analysis).
    import_violations: Vec<Violation>,
}

struct UseCollector<'a> {
    file_path: &'a str,
    migratable: &'a [(String, String)],
    imports: Imports,
}

impl<'a> UseCollector<'a> {
    fn suggestion_for(&self, symbol: &str) -> Option<&str> {
        self.migratable
            .iter()
            .find(|(s, _)| s == symbol)
            .map(|(_, sug)| sug.as_str())
    }

    /// Walk the subtree rooted *under* `FROM_CRATE` (i.e. after the crate
    /// segment), recording imported symbols and flagging migratable ones.
    fn walk_from_subtree(&mut self, tree: &UseTree) {
        match tree {
            UseTree::Name(n) => {
                let sym = n.ident.to_string();
                self.record_import(&sym, &sym, n.ident.span().start().line);
            }
            UseTree::Rename(r) => {
                // `use <FROM>::sym as local` — bare name is `local`, symbol is `sym`.
                let sym = r.ident.to_string();
                let local = r.rename.to_string();
                self.record_import(&local, &sym, r.ident.span().start().line);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.walk_from_subtree(item);
                }
            }
            UseTree::Path(p) => {
                // e.g. `use <FROM>::sub::foo` — keep descending; the leaf is
                // what matters for name matching.
                self.walk_from_subtree(&p.tree);
            }
            UseTree::Glob(g) => {
                self.imports.import_violations.push(Violation {
                    file: self.file_path.to_owned(),
                    line: g.star_token.spans[0].start().line,
                    symbol: "*".to_string(),
                    suggestion: format!(
                        "avoid `use {FROM_CRATE}::*` — it imports migratable \
                         symbols unqualified and defeats this hygiene check"
                    ),
                });
            }
        }
    }

    fn record_import(&mut self, local: &str, from_sym: &str, line: usize) {
        self.imports
            .bare
            .insert(local.to_string(), from_sym.to_string());
        if let Some(sug) = self.suggestion_for(from_sym) {
            self.imports.import_violations.push(Violation {
                file: self.file_path.to_owned(),
                line,
                symbol: from_sym.to_string(),
                suggestion: sug.to_string(),
            });
        }
    }
}

impl<'a, 'ast> Visit<'ast> for UseCollector<'a> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        match &item.tree {
            UseTree::Path(p) if p.ident == FROM_CRATE => self.walk_from_subtree(&p.tree),
            // `use <FROM_CRATE> as alias;`
            UseTree::Rename(r) if r.ident == FROM_CRATE => {
                self.imports.crate_aliases.insert(r.rename.to_string());
            }
            _ => {}
        }
        visit::visit_item_use(self, item);
    }
}

// ── usage visitor ──────────────────────────────────────────────────────────────

struct LibcVisitor<'a> {
    file_path: &'a str,
    migratable: &'a [(String, String)],
    imports: &'a Imports,
    violations: Vec<Violation>,
}

struct Violation {
    file: String,
    line: usize,
    symbol: String,
    suggestion: String,
}

impl<'a> LibcVisitor<'a> {
    fn suggestion_for(&self, symbol: &str) -> Option<&str> {
        self.migratable
            .iter()
            .find(|(s, _)| s == symbol)
            .map(|(_, sug)| sug.as_str())
    }

    fn flag(&mut self, symbol: String, suggestion: String, line: usize) {
        self.violations.push(Violation {
            file: self.file_path.to_owned(),
            line,
            symbol,
            suggestion,
        });
    }
}

impl<'a, 'ast> Visit<'ast> for LibcVisitor<'a> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segs: Vec<_> = path.segments.iter().collect();
        if segs.len() >= 2
            && self
                .imports
                .crate_aliases
                .contains(&segs[0].ident.to_string())
        {
            // `<FROM_CRATE>::X` (or `alias::X`).
            let symbol = segs[1].ident.to_string();
            if let Some(sug) = self.suggestion_for(&symbol) {
                let sug = sug.to_string();
                self.flag(symbol, sug, segs[1].ident.span().start().line);
            }
        } else if segs.len() == 1 {
            // Bare call to a symbol imported via `use <FROM_CRATE>::sym`.
            let local = segs[0].ident.to_string();
            if let Some(from_sym) = self.imports.bare.get(&local)
                && let Some(sug) = self.suggestion_for(from_sym)
            {
                let (sym, sug) = (from_sym.clone(), sug.to_string());
                self.flag(sym, sug, segs[0].ident.span().start().line);
            }
        }
        visit::visit_path(self, path);
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn src_files() -> Vec<PathBuf> {
    rust_files_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
}

fn path_key(path: &Path) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    path.strip_prefix(manifest)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ── test ──────────────────────────────────────────────────────────────────────

#[test]
fn no_migratable_libc_calls() {
    let to_src = to_crate_src_dir();
    let migratable = build_migratable(&to_src);

    let mut violations: Vec<Violation> = Vec::new();

    for path in src_files() {
        let key = path_key(&path);
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("libc_hygiene: WARNING — could not read {key}: {e}");
                continue;
            }
        };
        let ast: File = match syn::parse_file(&source) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("libc_hygiene: WARNING — could not parse {key} (skipped): {e}");
                continue;
            }
        };

        let mut uses = UseCollector {
            file_path: &key,
            migratable: &migratable,
            imports: Imports {
                crate_aliases: [FROM_CRATE.to_string()].into_iter().collect(),
                ..Default::default()
            },
        };
        uses.visit_file(&ast);
        let mut imports = uses.imports;
        violations.append(&mut imports.import_violations);

        let mut visitor = LibcVisitor {
            file_path: &key,
            migratable: &migratable,
            imports: &imports,
            violations: Vec::new(),
        };
        visitor.visit_file(&ast);
        violations.extend(visitor.violations);
    }

    if !violations.is_empty() {
        eprintln!("\n── libc_hygiene: migratable {FROM_CRATE} calls found ──────────────────────");
        for v in &violations {
            eprintln!(
                "  {}:{} — {FROM_CRATE}::{} → consider {}",
                v.file, v.line, v.symbol, v.suggestion
            );
        }
        eprintln!(
            "\n  {} violation(s). Migrate the call to {TO_CRATE}.\n  If a {TO_CRATE} API exists but the name differs, add to RENAMES.\n  If genuinely unmigrateable, add to ALLOW (with a reason) in this test.",
            violations.len()
        );
        panic!("libc hygiene check failed (see stderr above)");
    }
}
