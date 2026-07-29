//! Capability-hygiene lint for ambient filesystem authority.
//!
//! This is intentionally AST-based, modeled after `libc_hygiene.rs`: it parses
//! `src/` with `syn`, tracks imports and aliases, and flags new ambient
//! filesystem access outside explicitly allowlisted host-boundary modules.

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{ExprCall, ExprMethodCall, File, Path as SynPath, UseTree};
use walkdir::WalkDir;

const FS_CRATE_PATH: &[&str] = &["std", "fs"];
const ENV_CRATE_PATH: &[&str] = &["std", "env"];
const ENV_FUNCTIONS: &[&str] = &["current_dir", "set_current_dir", "temp_dir"];
const AMBIENT_CRATES: &[&str] = &["directories", "tempfile"];

const ALLOWED: &[(&str, &str)] = &[
    (
        "crates/xsh-applets/src/",
        "privileged applets intentionally operate on host device trees",
    ),
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
        "crates/xshi/src/interactive/app.rs",
        "xshi app tests and completion fixtures use host temp paths",
    ),
    (
        "crates/xshi/src/interactive/bench.rs",
        "xshi benchmarks initialize from host cwd",
    ),
    (
        "crates/xshi/src/interactive/complete.rs",
        "xshi completion inspects host filesystem paths",
    ),
    (
        "crates/xshi/src/interactive/config.rs",
        "xshi config/profile loading is host-facing",
    ),
    (
        "crates/xshi/src/interactive/denv.rs",
        "xshi direnv-style discovery and tests inspect host project files",
    ),
    (
        "crates/xshi/src/interactive/history.rs",
        "xshi history persists to host files",
    ),
    (
        "crates/xshi/src/interactive/listing.rs",
        "xshi listing inspects host directory entries",
    ),
    (
        "crates/xshi/src/interactive/session.rs",
        "xshi session owns cwd changes and host directory snapshots",
    ),
    (
        "crates/xshi/src/interactive/shell/glob.rs",
        "xshi shell globbing expands host paths",
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
        "src/modules/group.rs",
        "group helper intentionally reads host group database",
    ),
    (
        "src/modules/hash.rs",
        "hash helper reads user-supplied files",
    ),
    ("src/modules/ini.rs", "INI helper reads user-supplied files"),
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
        "src/modules/linux/kernel.rs",
        "Linux kernel helpers intentionally read host module trees",
    ),
    (
        "src/modules/linux/process.rs",
        "Linux process helpers intentionally inspect host process state",
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
        "src/runtime/process.rs",
        "process runtime owns executable lookup, cwd, and redirection handles",
    ),
    (
        "src/runtime/value.rs",
        "FsEntryValue constructor inspects FileType flags from caller-provided metadata",
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

    for path in src_files() {
        let key = path_key(&path);
        if is_allowed(&key) {
            continue;
        }
        let source =
            std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {key}: {error}"));
        let ast: File =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {key}: {error}"));

        let mut use_collector = UseCollector {
            imports: Imports {
                fs_modules: ["std::fs".to_string()].into_iter().collect(),
                env_modules: ["std::env".to_string()].into_iter().collect(),
                ..Default::default()
            },
        };
        use_collector.visit_file(&ast);

        let mut visitor = AmbientVisitor {
            file: &key,
            imports: &use_collector.imports,
            violations: Vec::new(),
        };
        visitor.visit_file(&ast);
        violations.extend(visitor.violations);
    }

    assert!(
        violations.is_empty(),
        "ambient filesystem access must be migrated to cap-std or explicitly allowlisted:\n{}",
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
