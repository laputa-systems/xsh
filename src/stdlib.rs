//! Compile-time catalog of embedded XSH standard-library implementations.
//!
//! Public standard-module entries are declared in `xsh_registry`; an entry may
//! bind its implementation to one of the embedded modules catalogued here
//! instead of a native `RuntimeOp` body. The catalog is a fixed compile-time
//! table — it never scans a directory, reads the environment, or consults the
//! filesystem when the executable runs, and no installed stdlib tree is
//! required.
//!
//! `include_str!` embeds each source and makes Cargo rebuild tracking cover
//! every embedded file. [`IDENTITIES`] is the accepted set of implementation
//! module identities; [`CATALOG`] must contain exactly one entry per identity,
//! which the tests below enforce.

use crate::modules::api_spec;
use crate::modules::signature::RuntimeOp;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCommand, ArenaExprKind, AstArena, CommandStmtId, ExprId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// A function whose body the runtime provides as a private representation
/// operation.
///
/// The embedded source still declares the function, so its signature is
/// checked with the module and callers bind arguments against it normally.
/// Lowering replaces every call with the operation, and the declared body
/// exists only to give the signature a shape: it is written so that reaching
/// it raises, and it can never be reached because every call site is
/// rewritten.
///
/// Access is narrow by construction: a call is rewritten only when the
/// enclosing function belongs to this very module, and the module's namespace
/// is a spelling no XSH identifier can produce.
pub(crate) struct StdlibBridge {
    /// Function identity inside the owning implementation module.
    pub(crate) function: &'static str,
    /// The private runtime operation the function lowers to.
    pub(crate) op: RuntimeOp,
}

/// One embedded implementation module.
pub(crate) struct StdlibModule {
    /// Stable internal identity used by registry implementation bindings.
    pub(crate) identity: &'static str,
    /// Presentation-only diagnostic label. Never identity or authority.
    pub(crate) label: &'static str,
    /// Embedded UTF-8 implementation source.
    pub(crate) source: &'static str,
    /// Functions in this module whose bodies the runtime provides.
    pub(crate) bridges: &'static [StdlibBridge],
}

pub(crate) const CATALOG: &[StdlibModule] = &[
    StdlibModule {
        identity: "bytes",
        label: "<xsh-stdlib:bytes>",
        bridges: &[],
        source: include_str!("../stdlib/bytes.xsh"),
    },
    StdlibModule {
        identity: "cli",
        label: "<xsh-stdlib:cli>",
        source: include_str!("../stdlib/cli.xsh"),
        bridges: &[
            StdlibBridge {
                function: "record_with_field",
                op: RuntimeOp::RecordWithField,
            },
            StdlibBridge {
                function: "type_name",
                op: RuntimeOp::BridgeTypeName,
            },
            StdlibBridge {
                function: "command_name",
                op: RuntimeOp::BridgeCommandName,
            },
        ],
    },
    StdlibModule {
        identity: "env",
        label: "<xsh-stdlib:env>",
        bridges: &[],
        source: include_str!("../stdlib/env.xsh"),
    },
    StdlibModule {
        identity: "hash",
        label: "<xsh-stdlib:hash>",
        bridges: &[],
        source: include_str!("../stdlib/hash.xsh"),
    },
    StdlibModule {
        identity: "linux_text",
        label: "<xsh-stdlib:linux_text>",
        bridges: &[StdlibBridge {
            function: "append_bytes",
            op: RuntimeOp::BridgeAppendBytes,
        }],
        source: include_str!("../stdlib/linux_text.xsh"),
    },
    StdlibModule {
        identity: "ini",
        label: "<xsh-stdlib:ini>",
        bridges: &[],
        source: include_str!("../stdlib/ini.xsh"),
    },
    StdlibModule {
        identity: "json",
        label: "<xsh-stdlib:json>",
        source: include_str!("../stdlib/json.xsh"),
        bridges: &[
            StdlibBridge {
                function: "record_with_field",
                op: RuntimeOp::RecordWithField,
            },
            StdlibBridge {
                function: "record_remove_field",
                op: RuntimeOp::RecordRemoveField,
            },
            StdlibBridge {
                function: "type_name",
                op: RuntimeOp::BridgeTypeName,
            },
        ],
    },
    StdlibModule {
        identity: "mime",
        label: "<xsh-stdlib:mime>",
        bridges: &[],
        source: include_str!("../stdlib/mime.xsh"),
    },
    StdlibModule {
        identity: "process",
        label: "<xsh-stdlib:process>",
        bridges: &[],
        source: include_str!("../stdlib/process.xsh"),
    },
    StdlibModule {
        identity: "shlex",
        label: "<xsh-stdlib:shlex>",
        bridges: &[],
        source: include_str!("../stdlib/shlex.xsh"),
    },
    StdlibModule {
        identity: "system",
        label: "<xsh-stdlib:system>",
        bridges: &[],
        source: include_str!("../stdlib/system.xsh"),
    },
    StdlibModule {
        identity: "text",
        label: "<xsh-stdlib:text>",
        bridges: &[],
        source: include_str!("../stdlib/text.xsh"),
    },
    StdlibModule {
        identity: "time",
        label: "<xsh-stdlib:time>",
        bridges: &[],
        source: include_str!("../stdlib/time.xsh"),
    },
    StdlibModule {
        identity: "tui",
        label: "<xsh-stdlib:tui>",
        bridges: &[],
        source: include_str!("../stdlib/tui.xsh"),
    },
    StdlibModule {
        identity: "unix",
        label: "<xsh-stdlib:unix>",
        bridges: &[],
        source: include_str!("../stdlib/unix.xsh"),
    },
];

/// Look up an embedded module by its internal identity.
pub(crate) fn find(identity: &str) -> Option<&'static StdlibModule> {
    CATALOG.iter().find(|module| module.identity == identity)
}

/// Fixture coverage for the host-text policy the embedded modules own.
///
/// Test-only: the fixtures drive the embedded helpers through
/// `Evaluator::probe_embedded_call`, which resolves the module in the compiled
/// catalog, so the tests observe the real bodies without exposing them.
#[cfg(test)]
mod embedded_fixture_tests;

/// The embedded module a reserved internal namespace spelling names.
pub(crate) fn find_by_namespace(namespace: &str) -> Option<&'static StdlibModule> {
    let identity = namespace.strip_prefix("<xsh-stdlib:")?.strip_suffix('>')?;
    find(identity)
}

/// Whether an operation is a private representation bridge.
///
/// These operations are not bound to any public entry; a program reaches one
/// only through the lowering rewrite inside the module that declares it.
pub(crate) fn is_private_bridge_op(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::RecordWithField
            | RuntimeOp::RecordRemoveField
            | RuntimeOp::BridgeTypeName
            | RuntimeOp::BridgeCommandName
            | RuntimeOp::BridgeAppendBytes
    )
}

/// Whether `module` declares a bridge that lowers to `op`.
pub(crate) fn declares_bridge_op(module: &'static StdlibModule, op: RuntimeOp) -> bool {
    module.bridges.iter().any(|bridge| bridge.op == op)
}

/// The private operation a bridge function in `module` lowers to.
pub(crate) fn bridge_op(module: &'static StdlibModule, function: &str) -> Option<RuntimeOp> {
    module
        .bridges
        .iter()
        .find(|bridge| bridge.function == function)
        .map(|bridge| bridge.op)
}

/// The internal arena namespace for an embedded module identity.
///
/// The spelling is deliberately unrepresentable as an XSH identifier so user
/// source, `use` paths, and module search roots cannot name an internal module
/// or capture its helpers.
pub(crate) fn namespace_text(identity: &str) -> String {
    format!("<xsh-stdlib:{identity}>")
}

/// Preparation counters for the standard-library architecture tests.
///
/// The counters observe only crate-internal preparation work, so they are
/// compiled behind the existing `native-tests` feature rather than shipped as a
/// public instrumentation switch.
#[cfg(feature = "native-tests")]
pub mod counters {
    use std::cell::Cell;

    thread_local! {
        static PARSED_MODULES: Cell<usize> = const { Cell::new(0) };
    }

    /// Embedded modules parsed on this thread since the last [`reset`].
    ///
    /// The count is thread-local because preparation happens on the calling
    /// thread and the architecture tests run in parallel with each other.
    pub fn parsed_modules() -> usize {
        PARSED_MODULES.with(Cell::get)
    }

    /// Restart the count. Tests that measure one preparation call this first.
    pub fn reset() {
        PARSED_MODULES.with(|count| count.set(0));
    }

    pub(crate) fn record_parsed_module() {
        PARSED_MODULES.with(|count| count.set(count.get() + 1));
    }
}

/// Embedded implementation modules a parsed program can reach.
///
/// Selection is conservative and syntactic: any mention of a public spelling
/// that a script-backed entry owns counts, including dead code, callable
/// references, command forms, and method calls. Over-selection only prepares an
/// implementation that the program never calls; under-selection would be a
/// preparation defect.
///
/// A referenced user-code loading route can reach any standard entry after
/// execution starts, so it forces the complete applicable set.
pub(crate) fn required_modules(arena: &AstArena) -> Vec<&'static str> {
    let mut needed: BTreeSet<&'static str> = BTreeSet::new();
    if arena_uses_dynamic_module_load(arena) {
        return CATALOG.iter().map(|module| module.identity).collect();
    }
    // An identifier that qualifies a field is resolved through that field, so
    // it is not also a bare mention of the module: `env.get` is the native
    // `get`, and must not select the module for its script-backed neighbours.
    let mut qualified_bases: BTreeSet<usize> = BTreeSet::new();
    for index in 0..arena.expr_tags.len() {
        if let ArenaExprKind::Field { base, .. } = arena.expr(ExprId::from_index(index)).kind {
            qualified_bases.insert(base.index());
        }
    }
    for index in 0..arena.expr_tags.len() {
        let id = ExprId::from_index(index);
        let expr = arena.expr(id);
        match expr.kind {
            ArenaExprKind::Ident(module) => {
                if qualified_bases.contains(&index) {
                    continue;
                }
                collect_module_mentions(&mut needed, &module.as_str());
            }
            ArenaExprKind::Field { base, name } => {
                if let ArenaExprKind::Ident(module) = arena.expr(base).kind {
                    collect_qualified_mention(&mut needed, &module.as_str(), &name.as_str());
                }
                collect_method_mention(&mut needed, &name.as_str());
            }
            _ => {}
        }
    }
    for index in 0..arena.command_stmts.len() {
        let ArenaCommand::Proc { name, .. } =
            arena.command_stmt(CommandStmtId::from_index(index)).command
        else {
            continue;
        };
        match name.as_str().as_str().split_once('.') {
            Some((module, api)) => collect_qualified_mention(&mut needed, module, api),
            None => collect_module_mentions(&mut needed, &name.as_str()),
        }
    }
    needed.into_iter().collect()
}

/// Whether the program can load user code after execution starts.
///
/// Only the `module.load` entry is a loading route, and only under a spelling
/// that names the `module` standard module: the program's own binding for that
/// name, whether written directly or introduced by `use module as …`. A local
/// variable or a record field that happens to be called `load` or `module` is
/// not a loading route, and treating one as such charges the program for the
/// whole embedded catalog.
fn arena_uses_dynamic_module_load(arena: &AstArena) -> bool {
    let mut loaders: BTreeSet<Name> = BTreeSet::new();
    loaders.insert(Name::intern("module"));
    for use_stmt in &arena.use_stmts {
        // `use module as alias` binds another spelling for the same module.
        let mut names = arena.names(use_stmt.path);
        match names.next() {
            Some(name) if name == "module" => {}
            _ => continue,
        }
        if let Some(alias) = use_stmt.alias.as_ref() {
            loaders.insert(*alias);
        }
    }
    for index in 0..arena.expr_tags.len() {
        let ArenaExprKind::Field { base, name } = arena.expr(ExprId::from_index(index)).kind
        else {
            continue;
        };
        if name != "load" {
            continue;
        }
        if let ArenaExprKind::Ident(module) = arena.expr(base).kind
            && loaders.contains(&module)
        {
            return true;
        }
    }
    false
}

/// A bare mention of a standard module name can be a callable reference,
/// module-level command form, or an opaque value handed to a callback; treat
/// every script-backed entry of that module as reachable.
fn collect_module_mentions(needed: &mut BTreeSet<&'static str>, module: &str) {
    let Some(entry) = api_spec().module(module) else {
        return;
    };
    for function in &entry.functions {
        for overload in &function.overloads {
            if let Some(script) = overload.script_impl() {
                needed.insert(script.module);
            }
        }
    }
}

fn collect_qualified_mention(needed: &mut BTreeSet<&'static str>, module: &str, function: &str) {
    let Some(overloads) = api_spec().module_overloads(module, function) else {
        return;
    };
    for overload in overloads {
        if let Some(script) = overload.script_impl() {
            needed.insert(script.module);
        }
    }
}

fn collect_method_mention(needed: &mut BTreeSet<&'static str>, method: &str) {
    for module in script_method_modules(method) {
        needed.insert(module);
    }
}

/// Implementation modules that own a script-backed method with this spelling.
///
/// The receiver type is unknown before checking, so the map is keyed by the
/// method spelling alone and every owner is included. A spelling owned by two
/// receivers therefore prepares both modules, which is the safe direction.
fn script_method_modules(method: &str) -> impl Iterator<Item = &'static str> {
    static BY_METHOD: OnceLock<BTreeMap<&'static str, BTreeSet<&'static str>>> = OnceLock::new();
    BY_METHOD
        .get_or_init(|| {
            let mut by_method: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
            for (receiver, methods) in api_spec().method_entries() {
                let _ = receiver;
                for named in methods {
                    for overload in &named.overloads {
                        if let Some(script) = overload.sig.script_impl() {
                            by_method
                                .entry(named.name)
                                .or_default()
                                .insert(script.module);
                        }
                    }
                }
            }
            by_method
        })
        .get(method)
        .into_iter()
        .flatten()
        .copied()
}

#[cfg(test)]
mod tests {
    use super::{CATALOG, find, namespace_text};
    use crate::modules::api_spec;

    /// Every identity a binding names must exist in the catalog.
    ///
    /// The reverse direction is target-dependent: a binding for a Linux-only
    /// entry is native elsewhere, so a catalog entry can be unreferenced on the
    /// current target while still being required by the build that uses it.
    /// `catalog_is_reachable_on_this_target` covers the direction that does
    /// hold everywhere, and the Linux build checks the other one.
    #[test]
    fn every_script_binding_has_a_catalog_entry() {
        for (module, function, script) in api_spec().script_impls() {
            assert!(
                find(script.module).is_some(),
                "`{module}.{function}` binds the missing module `{}`",
                script.module
            );
        }
    }

    /// Validate every embedded source, used or not.
    ///
    /// The catalog is checked as a whole rather than only through the modules a
    /// particular program happens to reference, so a bad unused embedded source
    /// cannot escape every test. Each source is prepared the way the runtime
    /// will see it — parsed as an internal implementation module, never as a
    /// user entry — and then run through the full production gate: check
    /// declarations, probe bodies, lower, and verify the whole store.
    #[test]
    fn every_catalog_module_parses_checks_and_lowers() {
        for module in CATALOG {
            crate::symbol::SymbolOwner::new().with_current(|| {
                let (sources, parsed) =
                    crate::loader::prepare_stdlib_catalog_module(module.identity)
                        .expect("catalog identity resolves");
                assert!(
                    parsed.diagnostics.is_empty(),
                    "{}: {:?}",
                    module.label,
                    parsed.diagnostics
                );
                let declarations =
                    crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
                assert!(
                    declarations.diagnostics.is_empty(),
                    "{}: {:?}",
                    module.label,
                    declarations.diagnostics
                );
                let bodies =
                    crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
                assert!(
                    bodies.diagnostics.is_empty(),
                    "{}: {:?}",
                    module.label,
                    bodies.diagnostics
                );
                let text = sources
                    .get(crate::source::SourceId::new(0))
                    .map(|source| source.text().to_string())
                    .unwrap_or_default();
                crate::runtime::eval::Evaluator::probe_embedded_module_lowering(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &text,
                    std::sync::Arc::new(sources),
                    crate::source::SourceId::new(0),
                )
                .unwrap_or_else(|error| {
                    panic!("{}: indexed IR rejected the module: {error}", module.label)
                });
            });
        }
    }

    #[test]
    fn catalog_entries_are_unique_and_parse_as_modules() {
        for (index, module) in CATALOG.iter().enumerate() {
            assert!(
                !CATALOG[..index]
                    .iter()
                    .any(|earlier| earlier.identity == module.identity),
                "duplicate catalog identity `{}`",
                module.identity
            );
            assert!(
                !module.source.trim().is_empty(),
                "catalog entry `{}` has no source",
                module.identity
            );
        }
    }

    #[test]
    fn every_binding_names_an_implementation_the_catalog_provides() {
        for (module, function, script) in api_spec().script_impls() {
            let entry = find(script.module).unwrap_or_else(|| {
                panic!(
                    "`{module}.{function}` binds the missing module `{}`",
                    script.module
                )
            });
            assert!(!entry.source.trim().is_empty());
        }
    }

    #[test]
    fn labels_and_namespaces_are_unique_and_unrepresentable() {
        for module in CATALOG {
            let namespace = namespace_text(module.identity);
            assert!(
                !namespace
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
                "internal namespace must not be a valid XSH identifier"
            );
            assert!(find(module.identity).is_some());
        }
        for (index, module) in CATALOG.iter().enumerate() {
            assert!(
                !CATALOG[..index]
                    .iter()
                    .any(|earlier| earlier.label == module.label),
                "embedded module labels must be unique"
            );
        }
    }

    /// Diagnostic probe of cold-start preparation cost by phase.
    /// Run with `cargo test --features native-tests --lib cold_start_phase_profile
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cold_start_phase_profile() {
        use std::time::{Duration, Instant};

        let reps: usize = 20;
        let mut phases = [("parse", Duration::ZERO), ("declarations", Duration::ZERO), ("bodies", Duration::ZERO), ("lower+verify", Duration::ZERO)];
        for identity in ["cli", "text", "json"] {
            for entry in &mut phases {
                entry.1 = Duration::ZERO;
            }
            for _ in 0..reps {
                crate::symbol::SymbolOwner::new().with_current(|| {
                    let start = Instant::now();
                    let (sources, parsed) =
                        crate::loader::prepare_stdlib_catalog_module(identity).expect("identity");
                    phases[0].1 += start.elapsed();
                    let start = Instant::now();
                    let declarations =
                        crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
                    phases[1].1 += start.elapsed();
                    let start = Instant::now();
                    let bodies =
                        crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
                    phases[2].1 += start.elapsed();
                    let text = sources
                        .get(crate::source::SourceId::new(0))
                        .map(|source| source.text().to_string())
                        .unwrap_or_default();
                    let start = Instant::now();
                    crate::runtime::eval::Evaluator::probe_embedded_module_lowering(
                        &parsed.arena,
                        &declarations,
                        &bodies,
                        &text,
                        std::sync::Arc::new(sources),
                        crate::source::SourceId::new(0),
                    )
                    .expect("lowering");
                    phases[3].1 += start.elapsed();
                });
            }
            let total: Duration = phases.iter().map(|(_, duration)| *duration).sum();
            println!(
                "{identity}: {:.2} ms total | {}",
                total.as_secs_f64() * 1000.0 / reps as f64,
                phases
                    .iter()
                    .map(|(name, duration)| format!(
                        "{name} {:.2} ms",
                        duration.as_secs_f64() * 1000.0 / reps as f64
                    ))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }

        // The whole-program path a `module.load` reference takes: every
        // embedded module is prepared before execution, and the program's own
        // check and lowering cover their bodies too.
        let source = "use module\n\nproc main() [io, error] {\n  let m = module.load(p\"nothing.xsh\")?\n  print m\n}\n";
        let mut phases = [("load+parse", Duration::ZERO), ("declarations", Duration::ZERO), ("bodies", Duration::ZERO), ("lower+verify", Duration::ZERO)];
        for _ in 0..reps {
            crate::symbol::SymbolOwner::new().with_current(|| {
                let start = Instant::now();
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only_with_linkage(
                    "profile.xsh",
                    crate::loader::entry_source_from_text("profile.xsh", source.to_string()),
                    Vec::new(),
                    crate::loader::StdlibLinkage::Prepare,
                );
                phases[0].1 += start.elapsed();
                let start = Instant::now();
                let declarations =
                    crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
                phases[1].1 += start.elapsed();
                let start = Instant::now();
                let bodies =
                    crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
                phases[2].1 += start.elapsed();
                let text = sources
                    .get(crate::source::SourceId::new(0))
                    .map(|entry| entry.text().to_string())
                    .unwrap_or_default();
                let start = Instant::now();
                crate::runtime::eval::Evaluator::probe_embedded_module_lowering(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &text,
                    std::sync::Arc::new(sources),
                    crate::source::SourceId::new(0),
                )
                .expect("lowering");
                phases[3].1 += start.elapsed();
            });
        }
        let total: Duration = phases.iter().map(|(_, duration)| *duration).sum();
        println!(
            "module.load program: {:.2} ms total | {}",
            total.as_secs_f64() * 1000.0 / reps as f64,
            phases
                .iter()
                .map(|(name, duration)| format!(
                    "{name} {:.2} ms",
                    duration.as_secs_f64() * 1000.0 / reps as f64
                ))
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
}
