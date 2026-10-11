use super::{
    ArenaRange, ArenaStmtKind, AstArena, CheckBundleView, Diagnostic, DiagnosticCode, ExprId,
    FunctionDefId, FxHashMap, FxHashSet, Label, Name, Severity, Span, StmtId,
};
use xsh::frontend::source::SourceId;

/// The bundle-level namespace and import table used by callable reachability.
/// Namespace zero is the entry source; every other namespace is one loaded user
/// module. This deliberately follows the loader's resolved module keys instead
/// of trying to recover paths from source spelling.
pub(super) struct CallableReachabilityModule {
    statements: ArenaRange,
    source_id: Option<SourceId>,
    imports: FxHashMap<Name, usize>,
}

#[derive(Clone, Copy)]
pub(super) struct ReachableCallable {
    definition: FunctionDefId,
    namespace: usize,
    name: Name,
    span: Span,
    exported: bool,
    proc_entry: bool,
}

#[derive(Default)]
pub(super) struct CallableEdges {
    targets: FxHashSet<usize>,
}

impl CallableEdges {
    fn all_targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.targets.iter().copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum CallableEdgeOwner {
    Initializer,
    Callable(FunctionDefId),
    CliMain,
    RootSignal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallableEdgeKind {
    Direct,
    Dynamic,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallableEdgeUse {
    pub owner: CallableEdgeOwner,
    pub kind: CallableEdgeKind,
    pub name: Name,
    pub qualifier: Option<Name>,
    pub shadowed: bool,
    pub call_site: Option<ExprId>,
    pub name_expr: Option<ExprId>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CallableEdgeFacts {
    pub(super) uses: Vec<CallableEdgeUse>,
    pub(super) final_runtime_bindings: FxHashSet<Name>,
}

pub(super) struct CallableEdgeCollector {
    facts: CallableEdgeFacts,
    owner: CallableEdgeOwner,
    active: bool,
    suppress_definitions: bool,
    scopes: Vec<FxHashSet<Name>>,
    contexts: Vec<(CallableEdgeOwner, bool, Vec<FxHashSet<Name>>)>,
    call_sites: Vec<Option<ExprId>>,
}

impl CallableEdgeCollector {
    pub(super) fn new() -> Self {
        Self {
            facts: CallableEdgeFacts::default(),
            owner: CallableEdgeOwner::Initializer,
            active: false,
            suppress_definitions: false,
            scopes: Vec::new(),
            contexts: Vec::new(),
            call_sites: Vec::new(),
        }
    }

    pub(super) fn begin_program(&mut self) {
        self.owner = CallableEdgeOwner::Initializer;
        self.active = true;
        self.suppress_definitions = false;
        self.scopes.clear();
        self.scopes.push(FxHashSet::default());
    }

    pub(super) fn finish_program(&mut self) {
        if let Some(bindings) = self.scopes.first() {
            self.facts.final_runtime_bindings.clone_from(bindings);
        }
        self.active = false;
    }

    pub(super) fn enter_owner(&mut self, owner: CallableEdgeOwner, inherit_root_bindings: bool) {
        let records = self.active && self.owner == CallableEdgeOwner::Initializer;
        let inherited = (records && inherit_root_bindings)
            .then(|| self.scopes.first().cloned().unwrap_or_default());
        self.contexts
            .push((self.owner, self.active, std::mem::take(&mut self.scopes)));
        self.owner = owner;
        self.active = records;
        self.suppress_definitions = false;
        self.scopes = if records {
            vec![inherited.unwrap_or_default()]
        } else {
            Vec::new()
        };
        self.call_sites.clear();
    }

    pub(super) fn leave_owner(&mut self) {
        if let Some((owner, active, scopes)) = self.contexts.pop() {
            self.owner = owner;
            self.active = active;
            self.scopes = scopes;
            self.suppress_definitions = false;
            self.call_sites.clear();
        }
    }

    pub(super) fn push_scope(&mut self) {
        if self.active {
            self.scopes.push(FxHashSet::default());
        }
    }

    pub(super) fn pop_scope(&mut self) {
        if self.active && self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub(super) fn define(&mut self, name: Name) {
        if self.active && !self.suppress_definitions && let Some(scope) = self.scopes.last_mut() {
            scope.insert(name);
        }
    }

    pub(super) fn suppress_definitions(&mut self, suppress: bool) {
        self.suppress_definitions = suppress;
    }

    pub(super) fn enter_call_callee(&mut self, call_site: ExprId, direct_shape: bool) {
        self.call_sites
            .push(direct_shape.then_some(call_site));
    }

    pub(super) fn leave_call_callee(&mut self) {
        self.call_sites.pop();
    }

    pub(super) fn direct_name(&mut self, name: Name, call_site: Option<ExprId>) {
        self.record(
            CallableEdgeKind::Direct,
            name,
            None,
            false,
            call_site,
            None,
        );
    }

    pub(super) fn direct_qualified(
        &mut self,
        alias: Name,
        name: Name,
        call_site: ExprId,
    ) {
        self.record(
            CallableEdgeKind::Direct,
            name,
            Some(alias),
            false,
            Some(call_site),
            None,
        );
    }

    pub(super) fn dynamic_name(&mut self, name: Name, name_expr: Option<ExprId>) {
        self.record(
            CallableEdgeKind::Dynamic,
            name,
            None,
            self.is_bound(name),
            None,
            name_expr,
        );
    }

    pub(super) fn dynamic_qualified(&mut self, alias: Name, name: Name, base: ExprId) {
        self.record(
            CallableEdgeKind::Dynamic,
            name,
            Some(alias),
            self.is_bound(alias),
            None,
            Some(base),
        );
    }

    pub(super) fn facts(&self) -> &CallableEdgeFacts {
        &self.facts
    }

    pub(super) fn into_facts(self) -> CallableEdgeFacts {
        self.facts
    }

    fn record(
        &mut self,
        kind: CallableEdgeKind,
        name: Name,
        qualifier: Option<Name>,
        shadowed: bool,
        call_site: Option<ExprId>,
        name_expr: Option<ExprId>,
    ) {
        if !self.active {
            return;
        }
        self.facts.uses.push(CallableEdgeUse {
            owner: self.owner,
            kind,
            name,
            qualifier,
            shadowed,
            call_site: call_site.or_else(|| self.call_sites.last().copied().flatten()),
            name_expr,
        });
    }

    fn is_bound(&self, name: Name) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(&name))
    }
}

/// A closed-world graph over the checked program bundle's callable
/// declarations. Direct calls are resolved from the AST's already-resolved
/// module imports; function identities used as values are separate dynamic
/// escape edges. The latter are only activated when their enclosing owner is
/// reachable, which avoids keeping a callee alive merely because an unused
/// callable refers to it.
pub(super) struct CallableReachability<'a> {
    arena: &'a AstArena,
    modules: Vec<CallableReachabilityModule>,
    callables: Vec<ReachableCallable>,
    callable_by_name: FxHashMap<(usize, Name), usize>,
}

impl<'a> CallableReachability<'a> {
    pub(super) fn new(view: &CheckBundleView<'a>) -> Self {
        let program = view.program();
        let arena = &program.arena;
        let mut module_keys = FxHashMap::default();
        for (offset, module) in view.modules().enumerate() {
            module_keys.insert(module.key.clone(), offset + 1);
        }

        let mut modules = Vec::with_capacity(view.modules().count() + 1);
        modules.push(CallableReachabilityModule {
            statements: view.root_statements(),
            source_id: source_id_for_range(program, view.root_statements()),
            imports: FxHashMap::default(),
        });
        modules.extend(
            view.modules()
                .map(|module| CallableReachabilityModule {
                    statements: module.statements,
                    source_id: source_id_for_range(program, module.statements),
                    imports: FxHashMap::default(),
                }),
        );

        for module in &mut modules {
            for stmt in arena.stmt_ids(module.statements) {
                let ArenaStmtKind::Use(use_id) = arena.stmt(stmt).kind else {
                    continue;
                };
                let use_stmt = arena.use_stmt(use_id);
                let Some(target) = use_stmt
                    .resolved
                    .as_deref()
                    .and_then(|key| module_keys.get(key))
                    .copied()
                else {
                    continue;
                };
                let alias = use_stmt.alias.or_else(|| arena.names(use_stmt.path).last());
                if let Some(alias) = alias {
                    module.imports.insert(alias, target);
                }
            }
        }

        let mut reachability = Self {
            arena,
            modules,
            callables: Vec::new(),
            callable_by_name: FxHashMap::default(),
        };
        reachability.collect_callable_declarations();
        reachability
    }

    fn collect_callable_declarations(&mut self) {
        let module_statements = self
            .modules
            .iter()
            .enumerate()
            .map(|(namespace, module)| {
                (
                    namespace,
                    self.arena.stmt_ids(module.statements).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        for (namespace, statements) in module_statements {
            for stmt in statements {
                let Some((definition, exported, proc_entry)) =
                    callable_statement_info(self.arena, stmt)
                else {
                    continue;
                };
                let callable = ReachableCallable {
                    definition,
                    namespace,
                    name: self.arena.function_def(definition).name,
                    span: self.arena.stmt(stmt).span,
                    exported,
                    proc_entry,
                };
                let index = self.callables.len();
                self.callable_by_name
                    .insert((namespace, callable.name), index);
                self.callables.push(callable);
            }
        }
    }

    pub(super) fn diagnostics(
        &self,
        source_facts: &FxHashMap<SourceId, CallableEdgeFacts>,
    ) -> Vec<Diagnostic> {
        let mut root_targets = FxHashSet::default();
        let mut callable_indices = FxHashMap::default();
        for (index, callable) in self.callables.iter().enumerate() {
            callable_indices.insert(callable.definition, index);
            if callable.exported
                || (callable.namespace == 0
                    && callable.proc_entry
                    && (callable.name == "main"
                        // A subcommand `cli main` entry is named `main WORD...`.
                        || callable.name.as_str().starts_with("main ")
                        || self
                            .arena
                            .function_def(callable.definition)
                            .test_declaration))
            {
                root_targets.insert(index);
            }
        }

        let mut edges = (0..self.callables.len())
            .map(|_| CallableEdges::default())
            .collect::<Vec<_>>();
        for (namespace, module) in self.modules.iter().enumerate() {
            let Some(facts) = module.source_id.and_then(|id| source_facts.get(&id)) else {
                continue;
            };
            let mut direct_call_sites = FxHashSet::default();
            for usage in facts
                .uses
                .iter()
                .filter(|usage| usage.kind == CallableEdgeKind::Direct)
            {
                if usage.owner == CallableEdgeOwner::RootSignal && namespace != 0 {
                    continue;
                }
                let target = usage.qualifier.map_or_else(
                    || self.resolve_unqualified(namespace, usage.name),
                    |alias| self.resolve_qualified(namespace, alias, usage.name),
                );
                if let Some(target) = target {
                    add_owner_edge(
                        usage.owner,
                        target,
                        &callable_indices,
                        &mut edges,
                        &mut root_targets,
                    );
                    if let Some(site) = usage.call_site {
                        direct_call_sites.insert(site);
                    }
                }
            }

            let mut resolved_names = FxHashSet::default();
            for usage in facts.uses.iter().filter(|usage| {
                usage.kind == CallableEdgeKind::Dynamic && usage.qualifier.is_some()
            }) {
                if usage.owner == CallableEdgeOwner::RootSignal && namespace != 0 {
                    continue;
                }
                if usage.call_site.is_some_and(|site| direct_call_sites.contains(&site))
                    || usage.shadowed
                    || (usage.owner == CallableEdgeOwner::RootSignal
                        && facts.final_runtime_bindings.contains(&usage.qualifier.unwrap()))
                {
                    continue;
                }
                let Some(target) = self.resolve_qualified(
                    namespace,
                    usage.qualifier.unwrap(),
                    usage.name,
                ) else {
                    continue;
                };
                add_owner_edge(
                    usage.owner,
                    target,
                    &callable_indices,
                    &mut edges,
                    &mut root_targets,
                );
                if let Some(name_expr) = usage.name_expr {
                    resolved_names.insert(name_expr);
                }
            }

            for usage in facts.uses.iter().filter(|usage| {
                usage.kind == CallableEdgeKind::Dynamic && usage.qualifier.is_none()
            }) {
                if usage.owner == CallableEdgeOwner::RootSignal && namespace != 0 {
                    continue;
                }
                if usage.call_site.is_some_and(|site| direct_call_sites.contains(&site))
                    || usage.name_expr.is_some_and(|name_expr| resolved_names.contains(&name_expr))
                    || usage.shadowed
                    || (usage.owner == CallableEdgeOwner::RootSignal
                        && facts.final_runtime_bindings.contains(&usage.name))
                {
                    continue;
                }
                let Some(target) = self.resolve_unqualified(namespace, usage.name) else {
                    continue;
                };
                add_owner_edge(
                    usage.owner,
                    target,
                    &callable_indices,
                    &mut edges,
                    &mut root_targets,
                );
            }
        }

        let mut reachable = vec![false; self.callables.len()];
        let mut pending = root_targets.into_iter().collect::<Vec<_>>();
        while let Some(index) = pending.pop() {
            if reachable[index] {
                continue;
            }
            reachable[index] = true;
            pending.extend(edges[index].all_targets());
        }

        let mut candidates = self
            .callables
            .iter()
            .enumerate()
            .filter(|(index, callable)| !callable.exported && !reachable[*index])
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, callable)| callable.span);
        candidates
            .into_iter()
            .map(|(_, callable)| {
                Diagnostic::new(
                    Severity::Warning,
                    format!("unused callable `{}`", callable.name.as_str()),
                )
                .with_code(DiagnosticCode::LintUnusedCallable)
                .with_label(Label::secondary(
                    callable.span,
                    "this unexported callable is not reachable from a bundle entry point",
                ))
            })
            .collect()
    }

    fn resolve_unqualified(&self, namespace: usize, name: Name) -> Option<usize> {
        self.callable_by_name.get(&(namespace, name)).copied()
    }

    fn resolve_qualified(&self, namespace: usize, alias: Name, name: Name) -> Option<usize> {
        let target_namespace = self.modules[namespace].imports.get(&alias)?;
        self.resolve_unqualified(*target_namespace, name)
    }
}

pub(super) fn callable_statement_info(arena: &AstArena, stmt: StmtId) -> Option<(FunctionDefId, bool, bool)> {
    let mut current = stmt;
    let mut exported = false;
    loop {
        match arena.stmt(current).kind {
            ArenaStmtKind::Export(inner) => {
                exported = true;
                current = inner;
            }
            ArenaStmtKind::ProcDef(definition) => return Some((definition, exported, true)),
            ArenaStmtKind::PureDef(definition) | ArenaStmtKind::StreamDef(definition) => {
                return Some((definition, exported, false));
            }
            _ => return None,
        }
    }
}

fn add_owner_edge(
    owner: CallableEdgeOwner,
    target: usize,
    callable_indices: &FxHashMap<FunctionDefId, usize>,
    edges: &mut [CallableEdges],
    root_targets: &mut FxHashSet<usize>,
) {
    if let CallableEdgeOwner::Callable(definition) = owner {
        if let Some(index) = callable_indices.get(&definition) {
            edges[*index].targets.insert(target);
        }
    } else {
        root_targets.insert(target);
    }
}

fn source_id_for_range(program: &xsh::frontend::syntax::arena::ArenaProgram, range: ArenaRange) -> Option<SourceId> {
    program
        .arena
        .stmt_ids(range)
        .next()
        .map(|statement| program.arena.stmt(statement).span.source_id)
        .or_else(|| program.module_doc_for(range).map(|span| span.source_id))
}
