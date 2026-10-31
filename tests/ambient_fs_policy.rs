//! Capability-hygiene lint for ambient filesystem authority.
//!
//! This is intentionally AST-based, modeled after `libc_hygiene.rs`: it parses
//! `src/` with `syn`, tracks imports and aliases, and flags new ambient
//! filesystem access outside explicitly allowlisted host-boundary modules.

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{ExprCall, ExprMethodCall, File, Item, ItemMod, Lit, Meta, Path as SynPath, UseTree};
use walkdir::WalkDir;

const FS_CRATE_PATH: &[&str] = &["std", "fs"];
const ENV_CRATE_PATH: &[&str] = &["std", "env"];
const ENV_FUNCTIONS: &[&str] = &["current_dir", "set_current_dir", "temp_dir"];
const AMBIENT_CRATES: &[&str] = &["directories", "tempfile"];

const ALLOWED: &[(&str, &str)] = &[
    (
        "src/runtime/eval/modules/auth.rs",
        "auth helpers intentionally access host user databases and login shell state",
    ),
    (
        "src/runtime/cgroup.rs",
        "cgroup support intentionally operates on /proc and /sys/fs/cgroup",
    ),
    (
        "crates/xsht/src/cli/check.rs",
        "developer tooling reads user-supplied source paths",
    ),
    (
        "crates/xsht/src/cli/coverage.rs",
        "developer tooling reads coverage paths",
    ),
    (
        "crates/xsht/src/cli/files.rs",
        "developer tooling searches and edits user-supplied workspace paths",
    ),
    (
        "crates/xsht/src/cli/fmt.rs",
        "developer tooling formats user-supplied source paths",
    ),
    (
        "crates/xsht/src/cli/grep.rs",
        "developer tooling searches user-supplied source paths",
    ),
    (
        "crates/xsht/src/cli/lint.rs",
        "developer tooling lints user-supplied source paths",
    ),
    (
        "crates/xsht/src/cli/refactor.rs",
        "developer tooling rewrites user-supplied source paths",
    ),
    (
        "crates/xsht/src/stats.rs",
        "runtime stats writes the report path supplied on its command line",
    ),
    (
        "crates/xsht/src/cli/trace.rs",
        "developer tooling reads trace paths",
    ),
    (
        "crates/xsht/src/cli/trace/syscall_trace.rs",
        "developer tooling owns temporary syscall trace files",
    ),
    (
        "src/docs.rs",
        "documentation generation operates on repository paths",
    ),
    (
        "src/frontend_stats.rs",
        "frontend diagnostics read user-supplied corpus paths",
    ),
    (
        "src/runtime/eval/indexed.rs",
        "test-only indexed IR evidence scans the checked-in frontend corpus",
    ),
    (
        "src/runtime/eval/indexed/full.rs",
        "test-only indexed module-loading coverage creates an isolated host temp tree",
    ),
    (
        "crates/xsht/src/main.rs",
        "test entrypoint reads user-supplied test paths",
    ),
    (
        "crates/xsht/src/xsht/test.rs",
        "test runner owns per-test host temp roots",
    ),
    (
        "src/runtime/eval/modules/host.rs",
        "host command specs preserve ambient cwd/path interop",
    ),
    (
        "src/modules/archive/cpio.rs",
        "archive cpio helpers operate on user-supplied archive paths",
    ),
    (
        "src/modules/archive/mod.rs",
        "archive facade operates on user-supplied archive paths",
    ),
    (
        "src/modules/archive/policy.rs",
        "archive policy validates user-supplied destination paths",
    ),
    (
        "src/modules/archive/tar.rs",
        "archive tar helpers operate on user-supplied archive paths",
    ),
    (
        "src/modules/archive/zip.rs",
        "archive zip helpers operate on user-supplied archive paths",
    ),
    (
        "src/modules/bytes.rs",
        "bytes helper reads and writes user-supplied byte paths",
    ),
    (
        "src/modules/codec.rs",
        "codec file adapters read bounded user-supplied paths after filesystem-effect admission",
    ),
    (
        "src/modules/codec/sequence.rs",
        "codec sequence readers retain and close admitted file handles for lazy reads",
    ),
    (
        "src/modules/compression.rs",
        "compression helpers operate on user-supplied archive paths",
    ),
    (
        "src/modules/diff.rs",
        "diff helper reads user-supplied files",
    ),
    (
        "src/modules/dns.rs",
        "DNS helper intentionally reads host resolver configuration",
    ),
    (
        "src/modules/elf.rs",
        "ELF helper reads user-supplied binaries",
    ),
    (
        "src/modules/fs.rs",
        "filesystem module is the boundary for ambient fs APIs",
    ),
    (
        "src/modules/fs/control.rs",
        "filesystem control primitives operate on caller-supplied paths",
    ),
    (
        "src/modules/fs/prims.rs",
        "filesystem primitives operate on caller-supplied paths",
    ),
    (
        "src/modules/group.rs",
        "group helper intentionally reads host group database",
    ),
    (
        "src/modules/hash.rs",
        "hash helper reads user-supplied files",
    ),
    (
        "src/modules/json.rs",
        "JSON helper reads user-supplied files",
    ),
    (
        "src/modules/linux.rs",
        "Linux facade stores file handles for explicitly requested device operations",
    ),
    (
        "src/modules/linux/block.rs",
        "Linux block helpers intentionally operate on host device and sysfs paths",
    ),
    (
        "src/modules/linux/block_control.rs",
        "Linux block control primitives operate on host device paths",
    ),
    (
        "src/modules/linux/kernel.rs",
        "Linux kernel helpers intentionally read host module trees",
    ),
    (
        "src/modules/linux/process.rs",
        "Linux process helpers intentionally inspect host process state",
    ),
    (
        "src/modules/linux/namespace.rs",
        "Linux namespace primitives inspect host process and namespace state",
    ),
    (
        "src/modules/linux/net_prims/fake.rs",
        "native-test network fakes read caller-supplied netlink fixture paths",
    ),
    (
        "src/modules/linux/sampling.rs",
        "Linux sampling primitives read host kernel counter tables",
    ),
    (
        "src/modules/linux/storage/discovery.rs",
        "Linux storage discovery reads host sysfs device paths",
    ),
    (
        "src/modules/linux/storage/fixture.rs",
        "native-test storage fakes read caller-supplied device fixture paths",
    ),
    (
        "src/modules/linux/real/boot.rs",
        "Linux real boot helpers intentionally operate on host boot resources",
    ),
    (
        "src/modules/linux/real/device.rs",
        "Linux real device helpers intentionally operate on host device resources",
    ),
    (
        "src/modules/linux/real/fs.rs",
        "Linux real filesystem helpers intentionally operate on host filesystems",
    ),
    (
        "src/modules/linux/real/kernel.rs",
        "Linux real kernel helpers intentionally operate on host kernel state",
    ),
    (
        "src/modules/linux/real/mount.rs",
        "Linux real mount helpers intentionally operate on host mounts",
    ),
    (
        "src/modules/linux/real/net.rs",
        "Linux real network helpers intentionally operate on host network state",
    ),
    (
        "src/modules/linux/tests.rs",
        "Linux module unit tests create host fixtures",
    ),
    (
        "src/modules/mime.rs",
        "MIME helper may inspect user-supplied paths",
    ),
    (
        "src/modules/net.rs",
        "network helper loads user-supplied TLS material",
    ),
    (
        "src/modules/patch.rs",
        "patch helper uses temporary files for atomic patch application",
    ),
    (
        "src/modules/process.rs",
        "process inspection intentionally reads /proc and platform process state",
    ),
    (
        "src/modules/ssh.rs",
        "ssh stream options open and create caller-supplied paths",
    ),
    (
        "src/modules/system.rs",
        "system helper intentionally inspects host system files",
    ),
    (
        "src/modules/tui.rs",
        "TUI helper reads terminal-related host state",
    ),
    (
        "src/modules/unix.rs",
        "Unix helper intentionally exposes host Unix state",
    ),
    (
        "src/modules/unix/process.rs",
        "Unix process primitives operate on caller-supplied descriptors and paths",
    ),
    (
        "src/modules/unix/sessions.rs",
        "Unix session primitives read host session records",
    ),
    (
        "src/modules/user.rs",
        "user helper intentionally reads host user database",
    ),
    (
        "src/runner.rs",
        "runner writes trace output requested by the user",
    ),
    (
        "src/runtime/eval.rs",
        "runtime owns module loading, cwd, globbing, and path effects",
    ),
    (
        "src/runtime/eval/lowered_run.rs",
        "lowered runtime owns module loading and explicit host path effects",
    ),
    (
        "src/runtime/eval/lowered_run/module_linux.rs",
        "lowered Linux calls perform host operations on explicit device and destination paths",
    ),
    (
        "src/runtime/eval/stream.rs",
        "lowered runtime line streams wrap files opened from explicit path methods",
    ),
    (
        "src/runtime/eval/command.rs",
        "runtime command evaluator handles redirections and path writes",
    ),
    (
        "src/runtime/eval/methods.rs",
        "runtime path methods intentionally operate on host paths",
    ),
    (
        "src/runtime/eval/modules/fs.rs",
        "runtime filesystem module bridges public APIs to host/capability operations",
    ),
    (
        "src/runtime/eval/modules/linux.rs",
        "Linux runtime facade writes explicit dry-run and device paths",
    ),
    (
        "src/runtime/eval/route_witness.rs",
        "native-test route witnesses read and write artifacts in the invocation output directory",
    ),
    (
        "src/runtime/eval/modules/misc.rs",
        "test and misc runtime helpers own fixture paths and cache files",
    ),
    (
        "src/runtime/eval/modules/unix.rs",
        "Unix runtime facade writes explicit dry-run paths",
    ),
    (
        "src/runtime/eval/tests.rs",
        "runtime unit tests create host fixtures",
    ),
    (
        "src/stdlib/embedded_fixture_tests.rs",
        "embedded standard-library tests read committed fixture files",
    ),
    (
        "src/runtime/process.rs",
        "process runtime owns executable lookup, cwd, and redirection handles",
    ),
    (
        "src/runtime/value.rs",
        "FsEntryValue constructor inspects FileType flags from caller-provided metadata",
    ),
    (
        "src/project.rs",
        "project loader reads the nearest user project configuration for a script entry",
    ),
    (
        "src/sema/check/cache_trace.rs",
        "opt-in checker cache tracing writes a fresh user-selected trace path",
    ),
    (
        "src/loader.rs",
        "script loader reads user-supplied source and module paths",
    ),
];

#[derive(Default)]
struct Imports {
    fs_modules: FxHashSet<String>,
    env_modules: FxHashSet<String>,
    ambient_modules: FxHashSet<String>,
    fs_items: FxHashMap<String, String>,
    env_items: FxHashMap<String, String>,
    ambient_items: FxHashMap<String, String>,
}

fn item_is_test_only(item: &Item) -> bool {
    let attrs = match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) => return false,
        _ => return false,
    };
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<Meta>()
                .is_ok_and(|condition| cfg_condition_requires_test(&condition))
    })
}

fn cfg_condition_requires_test(condition: &Meta) -> bool {
    match condition {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => {
            let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
            parser
                .parse2(list.tokens.clone())
                .is_ok_and(|conditions| conditions.iter().any(cfg_condition_requires_test))
        }
        Meta::List(list) if list.path.is_ident("any") => {
            let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
            parser.parse2(list.tokens.clone()).is_ok_and(|conditions| {
                !conditions.is_empty() && conditions.iter().all(cfg_condition_requires_test)
            })
        }
        _ => false,
    }
}

struct UseCollector {
    imports: Imports,
}

impl UseCollector {
    fn record_std_subtree(&mut self, tree: &UseTree) {
        match tree {
            UseTree::Name(name) if name.ident == "fs" => {
                self.imports.fs_modules.insert("fs".to_string());
            }
            UseTree::Rename(rename) if rename.ident == "fs" => {
                self.imports.fs_modules.insert(rename.rename.to_string());
            }
            UseTree::Name(name) if name.ident == "env" => {
                self.imports.env_modules.insert("env".to_string());
            }
            UseTree::Rename(rename) if rename.ident == "env" => {
                self.imports.env_modules.insert(rename.rename.to_string());
            }
            UseTree::Path(path) => {
                if path.ident == "fs" {
                    self.record_fs_subtree(&path.tree);
                } else if path.ident == "env" {
                    self.record_env_subtree(&path.tree);
                } else if AMBIENT_CRATES.contains(&path.ident.to_string().as_str()) {
                    self.record_ambient_subtree(&path.ident.to_string(), &path.tree);
                }
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.record_std_subtree(item);
                }
            }
            UseTree::Glob(_) | UseTree::Name(_) | UseTree::Rename(_) => {}
        }
    }

    fn record_fs_subtree(&mut self, tree: &UseTree) {
        match tree {
            UseTree::Name(name) if name.ident == "self" => {
                self.imports.fs_modules.insert("fs".to_string());
            }
            UseTree::Name(name) => {
                let symbol = name.ident.to_string();
                self.imports.fs_items.insert(symbol.clone(), symbol);
            }
            UseTree::Rename(rename) if rename.ident == "self" => {
                self.imports.fs_modules.insert(rename.rename.to_string());
            }
            UseTree::Rename(rename) => {
                self.imports
                    .fs_items
                    .insert(rename.rename.to_string(), rename.ident.to_string());
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.record_fs_subtree(item);
                }
            }
            UseTree::Path(path) => self.record_fs_subtree(&path.tree),
            UseTree::Glob(_) => {
                self.imports.fs_modules.insert("fs".to_string());
            }
        }
    }

    fn record_env_subtree(&mut self, tree: &UseTree) {
        match tree {
            UseTree::Name(name) if name.ident == "self" => {
                self.imports.env_modules.insert("env".to_string());
            }
            UseTree::Name(name) => {
                let symbol = name.ident.to_string();
                self.imports.env_items.insert(symbol.clone(), symbol);
            }
            UseTree::Rename(rename) if rename.ident == "self" => {
                self.imports.env_modules.insert(rename.rename.to_string());
            }
            UseTree::Rename(rename) => {
                self.imports
                    .env_items
                    .insert(rename.rename.to_string(), rename.ident.to_string());
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.record_env_subtree(item);
                }
            }
            UseTree::Path(path) => self.record_env_subtree(&path.tree),
            UseTree::Glob(_) => {
                self.imports.env_modules.insert("env".to_string());
            }
        }
    }

    fn record_ambient_subtree(&mut self, crate_name: &str, tree: &UseTree) {
        match tree {
            UseTree::Name(name) if name.ident == "self" => {
                self.imports.ambient_modules.insert(crate_name.to_string());
            }
            UseTree::Name(name) => {
                self.imports
                    .ambient_items
                    .insert(name.ident.to_string(), crate_name.to_string());
            }
            UseTree::Rename(rename) if rename.ident == "self" => {
                self.imports
                    .ambient_modules
                    .insert(rename.rename.to_string());
            }
            UseTree::Rename(rename) => {
                self.imports
                    .ambient_items
                    .insert(rename.rename.to_string(), crate_name.to_string());
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.record_ambient_subtree(crate_name, item);
                }
            }
            UseTree::Path(path) => self.record_ambient_subtree(crate_name, &path.tree),
            UseTree::Glob(_) => {
                self.imports.ambient_modules.insert(crate_name.to_string());
            }
        }
    }
}

impl<'ast> Visit<'ast> for UseCollector {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_is_test_only(item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        match &item.tree {
            UseTree::Path(path) if path.ident == "std" => self.record_std_subtree(&path.tree),
            UseTree::Path(path) if path.ident == "fs" => self.record_fs_subtree(&path.tree),
            UseTree::Rename(rename) if rename.ident == "fs" => {
                self.imports.fs_modules.insert(rename.rename.to_string());
            }
            UseTree::Path(path) if path.ident == "env" => self.record_env_subtree(&path.tree),
            UseTree::Rename(rename) if rename.ident == "env" => {
                self.imports.env_modules.insert(rename.rename.to_string());
            }
            UseTree::Path(path) if AMBIENT_CRATES.contains(&path.ident.to_string().as_str()) => {
                self.record_ambient_subtree(&path.ident.to_string(), &path.tree);
            }
            UseTree::Name(name) if AMBIENT_CRATES.contains(&name.ident.to_string().as_str()) => {
                self.imports.ambient_modules.insert(name.ident.to_string());
            }
            UseTree::Rename(rename)
                if AMBIENT_CRATES.contains(&rename.ident.to_string().as_str()) =>
            {
                self.imports
                    .ambient_modules
                    .insert(rename.rename.to_string());
            }
            _ => {}
        }
        visit::visit_item_use(self, item);
    }
}

struct AmbientVisitor<'a> {
    file: &'a str,
    imports: &'a Imports,
    violations: Vec<Violation>,
}

struct Violation {
    file: String,
    line: usize,
    kind: &'static str,
    detail: String,
}

impl AmbientVisitor<'_> {
    fn flag(&mut self, line: usize, kind: &'static str, detail: impl Into<String>) {
        self.violations.push(Violation {
            file: self.file.to_string(),
            line,
            kind,
            detail: detail.into(),
        });
    }
}

impl<'ast> Visit<'ast> for AmbientVisitor<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_is_test_only(item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_path(&mut self, path: &'ast SynPath) {
        let segments = path_segments(path);
        if starts_with(&segments, FS_CRATE_PATH) {
            self.flag(
                path.segments[1].ident.span().start().line,
                "std-fs",
                segments.join("::"),
            );
        } else if starts_with(&segments, ENV_CRATE_PATH)
            && segments
                .get(2)
                .is_some_and(|symbol| ENV_FUNCTIONS.contains(&symbol.as_str()))
        {
            self.flag(
                path.segments[2].ident.span().start().line,
                "std-env-cwd",
                segments.join("::"),
            );
        } else if segments
            .first()
            .is_some_and(|segment| AMBIENT_CRATES.contains(&segment.as_str()))
        {
            self.flag(
                path.segments[0].ident.span().start().line,
                "ambient-crate",
                segments.join("::"),
            );
        } else if let Some(first) = segments.first() {
            if self.imports.fs_modules.contains(first) {
                self.flag(
                    path.segments[0].ident.span().start().line,
                    "std-fs",
                    segments.join("::"),
                );
            } else if self.imports.env_modules.contains(first)
                && segments
                    .get(1)
                    .is_some_and(|symbol| ENV_FUNCTIONS.contains(&symbol.as_str()))
            {
                self.flag(
                    path.segments[0].ident.span().start().line,
                    "std-env-cwd",
                    segments.join("::"),
                );
            } else if let Some(symbol) = self.imports.fs_items.get(first) {
                self.flag(path.segments[0].ident.span().start().line, "std-fs", symbol);
            } else if let Some(symbol) = self.imports.env_items.get(first)
                && ENV_FUNCTIONS.contains(&symbol.as_str())
            {
                self.flag(
                    path.segments[0].ident.span().start().line,
                    "std-env-cwd",
                    symbol,
                );
            } else if self.imports.ambient_modules.contains(first) {
                self.flag(
                    path.segments[0].ident.span().start().line,
                    "ambient-crate",
                    segments.join("::"),
                );
            } else if let Some(symbol) = self.imports.ambient_items.get(first) {
                self.flag(
                    path.segments[0].ident.span().start().line,
                    "ambient-crate",
                    symbol,
                );
            }
        }
        visit::visit_path(self, path);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let syn::Expr::Path(expr_path) = call.func.as_ref() {
            let segments = path_segments(&expr_path.path);
            if starts_with(&segments, ENV_CRATE_PATH)
                && segments
                    .get(2)
                    .is_some_and(|symbol| ENV_FUNCTIONS.contains(&symbol.as_str()))
            {
                self.flag(
                    expr_path.path.segments[2].ident.span().start().line,
                    "std-env-cwd",
                    segments.join("::"),
                );
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if call.method == "canonicalize" {
            self.flag(
                call.method.span().start().line,
                "path-canonicalize",
                ".canonicalize()",
            );
        }
        visit::visit_expr_method_call(self, call);
    }
}

#[test]
fn ambient_filesystem_use_is_allowlisted() {
    let mut violations = Vec::new();
    let sources = src_files();
    let parsed_sources = parse_source_files(&sources);
    let ownership = source_ownership(&parsed_sources, &source_roots(&sources));

    for path in sources {
        let key = path_key(&path);
        if is_allowed(&key) {
            continue;
        }
        let ast = parsed_sources
            .get(&path)
            .unwrap_or_else(|| panic!("source ownership omitted {key}"));
        violations.extend(owned_ambient_violations(&path, &key, ast, &ownership));
    }

    assert!(
        violations.is_empty(),
        "ambient filesystem access must be rooted or explicitly allowlisted:\n{}",
        violations
            .iter()
            .map(|violation| format!(
                "{}:{}: {} {}",
                violation.file, violation.line, violation.kind, violation.detail
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn owned_ambient_violations(
    source: &Path,
    file: &str,
    ast: &File,
    ownership: &SourceOwnership,
) -> Vec<Violation> {
    if ownership.is_test_only(source) {
        Vec::new()
    } else {
        ambient_violations(file, ast)
    }
}

fn ambient_violations(file: &str, ast: &File) -> Vec<Violation> {
    let mut use_collector = UseCollector {
        imports: Imports {
            fs_modules: ["std::fs".to_string()].into_iter().collect(),
            env_modules: ["std::env".to_string()].into_iter().collect(),
            ..Default::default()
        },
    };
    use_collector.visit_file(ast);

    let mut visitor = AmbientVisitor {
        file,
        imports: &use_collector.imports,
        violations: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.violations
}

fn path_segments(path: &SynPath) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

fn starts_with(path: &[String], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(segment, expected)| segment == expected)
}

fn src_files() -> Vec<PathBuf> {
    WalkDir::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .map(|entry| entry.into_path())
        .collect()
}

fn parse_source_files(files: &[PathBuf]) -> FxHashMap<PathBuf, File> {
    let mut parsed = FxHashMap::default();
    for file in files {
        let key = path_key(file);
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {key}: {error}"));
        let ast: File = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("parse {key}: {error}"));
        parsed.insert(normalize_source_path(file), ast);
    }
    parsed
}

fn source_roots(files: &[PathBuf]) -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots = [
        "src/lib.rs",
        "src/main.rs",
        "src/entrypoints/xsh_main.rs",
    ]
    .into_iter()
    .map(|path| normalize_source_path(&manifest.join(path)))
    .collect::<Vec<_>>();
    let bin_directory = manifest.join("src/bin");
    roots.extend(files.iter().filter_map(|path| {
        let under_bin = path.starts_with(&bin_directory);
        let is_bin_root = path.parent() == Some(bin_directory.as_path())
            || path.file_name().is_some_and(|name| name == "main.rs");
        (under_bin && is_bin_root).then(|| normalize_source_path(path))
    }));

    let available = files
        .iter()
        .map(|path| normalize_source_path(path))
        .collect::<FxHashSet<_>>();
    for root in &roots {
        assert!(available.contains(root), "crate source root is missing: {}", path_key(root));
    }
    roots
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum SourceReachability {
    Production,
    Test,
}

#[derive(Default)]
struct SourceOwnership {
    production: FxHashSet<PathBuf>,
    test: FxHashSet<PathBuf>,
}

impl SourceOwnership {
    fn is_test_only(&self, path: &Path) -> bool {
        let path = normalize_source_path(path);
        self.test.contains(&path) && !self.production.contains(&path)
    }
}

fn source_ownership(
    source_files: &FxHashMap<PathBuf, File>,
    roots: &[PathBuf],
) -> SourceOwnership {
    let mut ownership = SourceOwnership::default();
    let mut active = FxHashSet::default();
    let mut visited = FxHashSet::default();
    for root in roots {
        let root = normalize_source_path(root);
        let directory = root.parent().expect("crate source root has a parent");
        visit_source_file(
            &root,
            directory,
            directory,
            SourceReachability::Production,
            source_files,
            &mut ownership,
            &mut active,
            &mut visited,
        );
    }
    ownership
}

fn visit_source_file(
    file: &Path,
    module_directory: &Path,
    path_base: &Path,
    reachability: SourceReachability,
    source_files: &FxHashMap<PathBuf, File>,
    ownership: &mut SourceOwnership,
    active: &mut FxHashSet<PathBuf>,
    visited: &mut FxHashSet<(PathBuf, PathBuf, PathBuf, SourceReachability)>,
) {
    let file = normalize_source_path(file);
    if active.contains(&file) {
        panic!("source module cycle at {}", path_key(&file));
    }
    let state = (
        file.clone(),
        normalize_source_path(module_directory),
        normalize_source_path(path_base),
        reachability,
    );
    if visited.contains(&state) {
        return;
    }
    let ast = source_files
        .get(&file)
        .unwrap_or_else(|| panic!("source module is not in src scan: {}", path_key(&file)));
    match reachability {
        SourceReachability::Production => {
            ownership.production.insert(file.clone());
        }
        SourceReachability::Test => {
            ownership.test.insert(file.clone());
        }
    }

    active.insert(file.clone());
    visit_source_items(
        &ast.items,
        &file,
        &state.1,
        &state.2,
        reachability,
        source_files,
        ownership,
        active,
        visited,
    );
    active.remove(&file);
    visited.insert(state);
}

fn visit_source_items(
    items: &[Item],
    file: &Path,
    directory: &Path,
    path_base: &Path,
    reachability: SourceReachability,
    source_files: &FxHashMap<PathBuf, File>,
    ownership: &mut SourceOwnership,
    active: &mut FxHashSet<PathBuf>,
    visited: &mut FxHashSet<(PathBuf, PathBuf, PathBuf, SourceReachability)>,
) {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let child_reachability = if reachability == SourceReachability::Test
            || item_is_test_only(item)
        {
            SourceReachability::Test
        } else {
            SourceReachability::Production
        };
        if let Some((_, items)) = &module.content {
            let inline_directory = directory.join(module.ident.to_string());
            visit_source_items(
                items,
                file,
                &inline_directory,
                &inline_directory,
                child_reachability,
                source_files,
                ownership,
                active,
                visited,
            );
        } else if let Some((source, child_directory, child_path_base)) =
            external_module_source(module, directory, path_base, source_files)
        {
            visit_source_file(
                &source,
                &child_directory,
                &child_path_base,
                child_reachability,
                source_files,
                ownership,
                active,
                visited,
            );
        }
    }
}

fn external_module_source(
    module: &ItemMod,
    directory: &Path,
    path_base: &Path,
    source_files: &FxHashMap<PathBuf, File>,
) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let explicit_path = module.attrs.iter().find_map(|attribute| {
        let Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        if !value.path.is_ident("path") {
            return None;
        }
        let syn::Expr::Lit(expression) = &value.value else {
            return None;
        };
        let Lit::Str(path) = &expression.lit else {
            return None;
        };
        Some(path.value())
    });
    if let Some(path) = explicit_path {
        let source = normalize_source_path(&path_base.join(path));
        if source_files.contains_key(&source) {
            let child_directory = source.parent()?.to_path_buf();
            return Some((source, child_directory.clone(), child_directory));
        }
        return None;
    }

    let name = module.ident.to_string();
    let flat = normalize_source_path(&directory.join(format!("{name}.rs")));
    if source_files.contains_key(&flat) {
        let child_directory = directory.join(name);
        let child_path_base = flat.parent()?.to_path_buf();
        return Some((flat, child_directory, child_path_base));
    }
    let nested = normalize_source_path(&directory.join(&name).join("mod.rs"));
    if source_files.contains_key(&nested) {
        let child_directory = directory.join(name);
        let child_path_base = nested.parent()?.to_path_buf();
        return Some((nested, child_directory, child_path_base));
    }
    None
}

fn normalize_source_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[test]
fn test_only_external_sources_require_exclusive_test_ownership() {
    let source_files = [
        (
            "src/lib.rs",
            "#[path = \"shared.rs\"] mod production_shared;\n\
             #[cfg(test)] #[path = \"shared.rs\"] mod test_shared;\n\
             #[cfg(any(test, feature = \"native-tests\"))]\n\
             #[path = \"mixed.rs\"] mod mixed;\n\
             #[cfg(test)] #[path = \"fixture.rs\"] mod test_fixture;",
        ),
        (
            "src/shared.rs",
            "fn shared() { let _ = std::fs::read(\"shared\"); }",
        ),
        (
            "src/mixed.rs",
            "fn mixed() { let _ = std::fs::read(\"mixed\"); }",
        ),
        ("src/fixture.rs", "mod nested;"),
        (
            "src/nested.rs",
            "fn nested() { let _ = std::fs::read(\"nested\"); }",
        ),
        (
            "src/unowned.rs",
            "fn unowned() { let _ = std::fs::read(\"unowned\"); }",
        ),
    ]
    .into_iter()
    .map(|(path, source)| {
        (
            PathBuf::from(path),
            syn::parse_file(source).expect("parse ownership fixture"),
        )
    })
    .collect::<FxHashMap<_, _>>();
    let ownership = source_ownership(&source_files, &[PathBuf::from("src/lib.rs")]);

    assert!(!ownership.is_test_only(Path::new("src/shared.rs")));
    assert!(!ownership.is_test_only(Path::new("src/mixed.rs")));
    assert!(ownership.is_test_only(Path::new("src/fixture.rs")));
    assert!(ownership.is_test_only(Path::new("src/nested.rs")));
    assert!(!ownership.is_test_only(Path::new("src/unowned.rs")));
    assert!(!owned_ambient_violations(
        Path::new("src/shared.rs"),
        "src/shared.rs",
        &source_files[Path::new("src/shared.rs")],
        &ownership,
    )
    .is_empty());
    assert!(!owned_ambient_violations(
        Path::new("src/mixed.rs"),
        "src/mixed.rs",
        &source_files[Path::new("src/mixed.rs")],
        &ownership,
    )
    .is_empty());
    assert!(owned_ambient_violations(
        Path::new("src/nested.rs"),
        "src/nested.rs",
        &source_files[Path::new("src/nested.rs")],
        &ownership,
    )
    .is_empty());
    assert!(owned_ambient_violations(
        Path::new("src/fixture.rs"),
        "src/fixture.rs",
        &source_files[Path::new("src/fixture.rs")],
        &ownership,
    )
    .is_empty());
    assert!(!owned_ambient_violations(
        Path::new("src/unowned.rs"),
        "src/unowned.rs",
        &source_files[Path::new("src/unowned.rs")],
        &ownership,
    )
    .is_empty());
}

fn path_key(path: &Path) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    path.strip_prefix(manifest)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_allowed(path: &str) -> bool {
    ALLOWED
        .iter()
        .any(|(prefix, _reason)| path.starts_with(prefix))
}
