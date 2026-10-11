use super::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaPatternKind,
    ArenaPipeStage, ArenaPipeStageKind, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar,
    ArenaSugarOperand, ArenaWordPart, AssignTargetId, AstArena, BindingTargetId, BlockId,
    BuilderBlockId, CommandStmtId, Diagnostic, DiagnosticCode, ExprId, FunctionDefId, FxHashMap,
    FxHashSet, Label, Name, PatternId, RunFormId, Severity, Span, StmtId,
};

/// The bundle-level namespace and import table used by callable reachability.
/// Namespace zero is the entry source; every other namespace is one loaded user
/// module. This deliberately follows the loader's resolved module keys instead
/// of trying to recover paths from source spelling.
pub(super) struct CallableReachabilityModule {
    statements: ArenaRange,
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
    direct: FxHashSet<usize>,
    dynamic: FxHashSet<usize>,
}

impl CallableEdges {
    fn all_targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.direct.iter().chain(&self.dynamic).copied()
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
    pub(super) fn new(program: &'a ArenaProgram) -> Self {
        let arena = &program.arena;
        let mut module_keys = FxHashMap::default();
        for (offset, module) in program.modules.iter().enumerate() {
            module_keys.insert(module.key.clone(), offset + 1);
        }

        let mut modules = Vec::with_capacity(program.modules.len() + 1);
        modules.push(CallableReachabilityModule {
            statements: program.statements,
            imports: FxHashMap::default(),
        });
        modules.extend(
            program
                .modules
                .iter()
                .map(|module| CallableReachabilityModule {
                    statements: module.statements,
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

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut root_targets = FxHashSet::default();
        for (index, callable) in self.callables.iter().enumerate() {
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

        for namespace in 0..self.modules.len() {
            let mut scanner = CallableEdgeScanner::new(self, namespace);
            scanner.scan_top_level_initializers();
            root_targets.extend(scanner.edges.all_targets());
            if namespace == 0 {
                scanner.scan_root_signal_hooks();
                root_targets.extend(scanner.edges.all_targets());
            }
        }

        let edges = self
            .callables
            .iter()
            .map(|callable| {
                let mut scanner = CallableEdgeScanner::new(self, callable.namespace);
                scanner.scan_callable(callable.definition);
                scanner.edges
            })
            .collect::<Vec<_>>();
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

pub(super) struct CallableEdgeScanner<'analysis, 'arena> {
    analysis: &'analysis CallableReachability<'arena>,
    namespace: usize,
    scopes: Vec<FxHashSet<Name>>,
    edges: CallableEdges,
}

impl<'analysis, 'arena> CallableEdgeScanner<'analysis, 'arena> {
    pub(super) fn new(analysis: &'analysis CallableReachability<'arena>, namespace: usize) -> Self {
        Self {
            analysis,
            namespace,
            scopes: vec![FxHashSet::default()],
            edges: CallableEdges::default(),
        }
    }

    fn arena(&self) -> &AstArena {
        self.analysis.arena
    }

    fn scan_top_level_initializers(&mut self) {
        let statements = self
            .arena()
            .stmt_ids(self.analysis.modules[self.namespace].statements)
            .collect::<Vec<_>>();
        self.scan_sequence(&statements);
    }

    fn scan_root_signal_hooks(&mut self) {
        let statements = self
            .arena()
            .stmt_ids(self.analysis.modules[0].statements)
            .collect::<Vec<_>>();
        for stmt in statements {
            let ArenaStmtKind::SignalHook(hook) = self.arena().stmt(stmt).kind else {
                continue;
            };
            self.scan_block(self.arena().signal_hook(hook).body);
        }
    }

    fn scan_callable(&mut self, definition: FunctionDefId) {
        let definition = self.arena().function_def(definition).clone();
        for param in self.arena().params(definition.params).to_vec() {
            if let Some(default) = param.default {
                self.scan_expr(default);
            }
            self.define(param.name);
        }
        self.scan_block(definition.body);
    }

    fn scan_sequence(&mut self, statements: &[StmtId]) {
        for &stmt in statements {
            self.scan_stmt(stmt);
        }
    }

    fn scan_stmt(&mut self, stmt: StmtId) {
        match self.arena().stmt(stmt).kind {
            ArenaStmtKind::Export(inner) => self.scan_stmt(inner),
            ArenaStmtKind::CliMain(def) => {
                self.push_scope();
                self.scan_callable(def);
                self.pop_scope();
            }
            ArenaStmtKind::Let {
                target,
                initializer,
                ..
            }
            | ArenaStmtKind::Const {
                target,
                initializer,
                ..
            }
            | ArenaStmtKind::Var {
                target,
                initializer,
                ..
            } => {
                self.scan_expr_or_run(initializer);
                self.define_binding_target(target);
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                self.scan_assign_target(target);
                self.scan_expr_or_run(value);
            }
            ArenaStmtKind::Return(Some(value))
            | ArenaStmtKind::Yield(value)
            | ArenaStmtKind::Defer(value, _) => self.scan_expr_or_run(value),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in self.arena().if_branches(branches).to_vec() {
                    self.scan_expr(branch.condition);
                    self.scan_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.scan_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.scan_expr(condition);
                self.scan_block(block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
                ..
            } => {
                self.scan_expr(iter);
                self.push_scope();
                self.define_binding_target(target);
                self.scan_block(block);
                self.pop_scope();
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                let bindings = self.arena().with_bindings(bindings).to_vec();
                self.push_scope();
                for binding in &bindings {
                    self.scan_expr(binding.initializer);
                    if binding.name.as_str() != "_" {
                        self.define(binding.name);
                    }
                }
                self.scan_block(body);
                self.pop_scope();
                self.push_scope();
                self.scan_block(else_block);
                self.pop_scope();
            }
            ArenaStmtKind::Loop { block } => self.scan_block(block),
            ArenaStmtKind::Sugar { form, operands, .. } => {
                match self.arena().sugar(form, operands) {
                    // The name is in scope for the body, not for the
                    // destination.
                    ArenaSugar::Atomically { dest, name, body } => {
                        self.scan_expr(dest);
                        self.push_scope();
                        self.define_binding_target(name);
                        self.scan_block(body);
                        self.pop_scope();
                    }
                    ArenaSugar::ForIndex {
                        index,
                        item,
                        source,
                        body,
                    } => {
                        self.scan_expr(source);
                        self.push_scope();
                        self.define_binding_target(index);
                        self.define_binding_target(item);
                        self.scan_block(body);
                        self.pop_scope();
                    }
                    _ => {
                        for operand in self.arena().sugar_operands(operands).to_vec() {
                            match operand {
                                ArenaSugarOperand::Expr(expr) => self.scan_expr(expr),
                                ArenaSugarOperand::Block(block) => self.scan_block(block),
                                ArenaSugarOperand::Stmt(stmt) => self.scan_stmt(stmt),
                                _ => {}
                            }
                        }
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                self.scan_expr_or_run(initializer);
                self.push_scope();
                self.scan_block(else_block);
                self.pop_scope();
                self.define_binding_target(target);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.scan_expr(condition);
                if let Some(message) = message {
                    self.scan_expr(message);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.scan_expr(value);
                for arm in self.arena().match_arms(arms).to_vec() {
                    self.push_scope();
                    self.define_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.scan_expr(guard);
                    }
                    self.scan_block(arm.block);
                    self.pop_scope();
                }
            }
            ArenaStmtKind::Command(command) => self.scan_command(command),
            ArenaStmtKind::TailBareIdent(name) => self.add_direct_unqualified(name),
            ArenaStmtKind::Expr(expr)
            | ArenaStmtKind::YieldDelegate(expr)
            | ArenaStmtKind::Exit(expr) => self.scan_expr(expr),
            // Callable bodies and hooks have their own entry conditions. A
            // declaration is never executed while its containing initializer
            // runs, so only roots and graph edges scan those bodies.
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::SignalHook(_)
            | ArenaStmtKind::Return(None)
            | ArenaStmtKind::Break { value: None }
            | ArenaStmtKind::Continue => {}
            ArenaStmtKind::Break { value: Some(value) } => self.scan_expr(value),
        }
    }

    fn scan_block(&mut self, block: BlockId) {
        let block = self.arena().block(block).clone();
        self.push_scope();
        for param in self.arena().block_params(block.params).to_vec() {
            self.define(param.name);
        }
        let statements = self.arena().stmt_ids(block.statements).collect::<Vec<_>>();
        self.scan_sequence(&statements);
        self.pop_scope();
    }

    fn scan_expr_or_run(&mut self, value: ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.scan_expr(expr),
            ArenaExprOrRun::Run(run) => self.scan_run(run),
        }
    }

    fn scan_comp_qualifiers(&mut self, range: ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.arena().comp_qualifiers(range).to_vec() {
            self.scan_expr(qualifier.expr());
            if let ArenaCompQualifier::For { target, .. } = qualifier {
                self.push_scope();
                scopes += 1;
                self.define_binding_target(target);
            }
        }
        scopes
    }

    fn scan_expr(&mut self, expr: ExprId) {
        match self.arena().expr(expr).kind {
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.scan_expr(input);
                self.scan_expr(call);
            }

            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in self.arena().fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(expr, _) = part {
                        self.scan_expr(expr);
                    }
                }
            }
            ArenaExprKind::Ident(name) => self.add_dynamic_unqualified(name),
            ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
                for item in self.arena().list_element_exprs(items).collect::<Vec<_>>() {
                    self.scan_expr(item);
                }
            }
            ArenaExprKind::ListComp { expr, qualifiers }
            | ArenaExprKind::SetComp { expr, qualifiers } => {
                let scopes = self.scan_comp_qualifiers(qualifiers);
                self.scan_expr(expr);
                for _ in 0..scopes {
                    self.pop_scope();
                }
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let scopes = self.scan_comp_qualifiers(qualifiers);
                self.scan_expr(key);
                self.scan_expr(value);
                for _ in 0..scopes {
                    self.pop_scope();
                }
            }
            ArenaExprKind::Record(fields) => {
                for field in self.arena().record_fields(fields).to_vec() {
                    match field.kind {
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            self.scan_expr(key);
                            self.scan_expr(value);
                        }
                        ArenaRecordFieldKind::Named { value, .. }
                        | ArenaRecordFieldKind::Path { value, .. }
                        | ArenaRecordFieldKind::Spread { expr: value, .. } => self.scan_expr(value),
                        ArenaRecordFieldKind::Shorthand { name, .. } => {
                            self.add_dynamic_unqualified(name)
                        }
                    }
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                for branch in self.arena().if_expr_branches(branches).to_vec() {
                    self.scan_expr(branch.condition);
                    self.scan_expr(branch.value);
                }
                self.scan_expr(else_value);
            }
            ArenaExprKind::Match { value, arms }
            | ArenaExprKind::PatternTest { value, arms }
            | ArenaExprKind::PatternCondition { value, arms } => {
                self.scan_expr(value);
                for arm in self.arena().match_expr_arms(arms).to_vec() {
                    self.push_scope();
                    self.define_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.scan_expr(guard);
                    }
                    self.scan_expr(arm.value);
                    self.pop_scope();
                }
            }
            ArenaExprKind::Unary { expr, .. }
            | ArenaExprKind::Try(expr)
            | ArenaExprKind::Require { value: expr, .. }
            | ArenaExprKind::Convert { value: expr, .. } => self.scan_expr(expr),
            ArenaExprKind::ComparisonChain(pairs) => {
                for operand in self
                    .arena()
                    .comparison_chain_operands(pairs)
                    .collect::<Vec<_>>()
                {
                    self.scan_expr(operand);
                }
            }
            ArenaExprKind::Binary { left, right, .. }
            | ArenaExprKind::Index {
                base: left,
                index: right,
                ..
            } => {
                self.scan_expr(left);
                self.scan_expr(right);
            }
            ArenaExprKind::Call { callee, args } => {
                if self.add_direct_callee(callee).is_none() {
                    self.scan_expr(callee);
                }
                for arg in self.arena().call_args(args).to_vec() {
                    self.scan_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if self.add_dynamic_qualified_field(base, name).is_none() {
                    self.scan_expr(base);
                }
            }
            ArenaExprKind::Slice {
                base, start, end, ..
            } => {
                self.scan_expr(base);
                if let Some(start) = start {
                    self.scan_expr(start);
                }
                if let Some(end) = end {
                    self.scan_expr(end);
                }
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.scan_expr(input);
                for stage in self.arena().pipe_stages(stages).to_vec() {
                    self.scan_pipe_stage(&stage);
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.scan_expr(input);
                for stage in self.arena().stream_stages(stages).to_vec() {
                    self.scan_stream_stage(&stage);
                }
            }
            ArenaExprKind::Run(run) => self.scan_run(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.scan_run(run),
                ArenaSpawnTarget::Command(command) => self.scan_expr(command),
            },
            ArenaExprKind::Wait(form) => self.scan_expr(form.target),
            ArenaExprKind::BuilderCall { call, block } => {
                self.scan_expr(call);
                self.scan_builder_block(block);
            }
            ArenaExprKind::ErrorContext { message, block }
            | ArenaExprKind::ContextScope {
                input: message,
                block,
                ..
            } => {
                self.scan_expr(message);
                self.scan_block(block);
            }
            ArenaExprKind::Capture(block)
            | ArenaExprKind::ValueBlock(block)
            | ArenaExprKind::Loop { block }
            | ArenaExprKind::Collect { block } => self.scan_block(block),
            ArenaExprKind::TempDirScope { path, block, .. } => {
                if let Some(path) = path {
                    self.scan_expr(path);
                }
                self.scan_block(block);
            }
            ArenaExprKind::ResourceScope {
                bindings, block, ..
            } => {
                for binding in self.arena().with_bindings(bindings).to_vec() {
                    self.scan_expr(binding.initializer);
                }
                self.scan_block(block);
            }
            ArenaExprKind::Retry { delays, block, .. } => {
                for delay in self.arena().expr_ids(delays).collect::<Vec<_>>() {
                    self.scan_expr(delay);
                }
                self.scan_block(block);
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus
            | ArenaExprKind::EnvString(_)
            | ArenaExprKind::EnvPathList => {}
        }
    }

    fn scan_call_arg(&mut self, arg: &ArenaCallArg) {
        match arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Named { value: expr, .. }
            | ArenaCallArgKind::Splice { value: expr, .. }
            | ArenaCallArgKind::NamedSpread { value: expr, .. } => self.scan_expr(expr),
        }
    }

    fn scan_pipe_stage(&mut self, stage: &ArenaPipeStage) {
        match &stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.scan_expr(*expr),
            ArenaPipeStageKind::Stream(stage) => self.scan_stream_stage(stage),
        }
    }

    fn scan_stream_stage(&mut self, stage: &ArenaStreamStage) {
        for arg in self.arena().call_args(stage.args).to_vec() {
            self.scan_call_arg(&arg);
        }
        if let Some(block) = stage.block {
            self.scan_block(block);
        }
    }

    fn scan_builder_block(&mut self, block: BuilderBlockId) {
        let entries = self
            .arena()
            .builder_entries(self.arena().builder_block(block).entries)
            .to_vec();
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => self.scan_expr(value),
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.arena().command_args(args).to_vec() {
                        self.scan_command_arg(&arg);
                    }
                    if let Some(block) = block {
                        self.scan_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => self.scan_block(block),
                ArenaBuilderEntryKind::Stmt(stmt) => self.scan_stmt(stmt),
            }
        }
    }

    fn scan_command(&mut self, command: CommandStmtId) {
        match self.arena().command_stmt(command).command.clone() {
            ArenaCommand::Proc { name, args } => {
                self.add_direct_unqualified(name);
                for arg in self.arena().command_args(args).to_vec() {
                    self.scan_command_arg(&arg);
                }
            }
            ArenaCommand::Core {
                args, env, block, ..
            } => {
                for arg in self.arena().command_args(args).to_vec() {
                    self.scan_command_arg(&arg);
                }
                self.scan_env_assignments(env);
                if let Some(block) = block {
                    self.scan_block(block);
                }
            }
            ArenaCommand::Run(run) => self.scan_run(run),
        }
    }

    fn scan_run(&mut self, run: RunFormId) {
        let segments = self
            .arena()
            .run_segments(self.arena().run_form(run).segments)
            .to_vec();
        for segment in segments {
            if let Some(timeout) = segment.timeout {
                self.scan_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.scan_expr(cpu_max);
            }
            if let Some(accept) = segment.accept {
                self.scan_expr(accept);
            }
            self.scan_env_assignments(segment.env);
            self.scan_command_arg(&segment.target);
            for arg in self.arena().command_args(segment.args).to_vec() {
                self.scan_command_arg(&arg);
            }
            for redirection in self.arena().redirections(segment.redirections).to_vec() {
                match redirection.target {
                    ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                        self.scan_command_arg(&arg)
                    }
                }
            }
        }
    }

    fn scan_env_assignments(&mut self, assignments: ArenaRange) {
        for assignment in self.arena().env_assignments(assignments).to_vec() {
            match assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => self.scan_command_arg(&arg),
                ArenaEnvAssignmentValue::Expr(expr) => self.scan_expr(expr),
            }
        }
    }

    fn scan_command_arg(&mut self, arg: &ArenaCommandArg) {
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                for part in self.arena().word_parts(*parts).collect::<Vec<_>>() {
                    match part {
                        ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                            self.scan_expr(expr)
                        }
                        ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => {}
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(name) => self.add_dynamic_unqualified(*name),
            ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
                self.scan_expr(*expr)
            }
        }
    }

    fn scan_assign_target(&mut self, target: AssignTargetId) {
        match self.arena().assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. } => self.scan_assign_target(base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.scan_assign_target(base);
                self.scan_expr(index);
            }
        }
    }

    fn define_binding_target(&mut self, target: BindingTargetId) {
        match self.arena().binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => self.define(name),
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena().destructure_fields(fields).to_vec() {
                    self.define_binding_target(field.target);
                }
            }
        }
    }

    fn define_pattern(&mut self, pattern: PatternId) {
        match self.arena().pattern(pattern).kind.clone() {
            ArenaPatternKind::Group(child) => self.define_pattern(child),
            ArenaPatternKind::Alias { pattern, name, .. } => {
                self.define_pattern(pattern);
                self.define(name);
            }
            ArenaPatternKind::Binding(name) => self.define(name),
            ArenaPatternKind::Type {
                binding: Some(name),
                ..
            } => self.define(name),
            ArenaPatternKind::Record { fields, .. } => {
                for field in self.arena().pattern_fields(fields).to_vec() {
                    self.define_pattern(field.pattern);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in self
                    .arena()
                    .pattern_ids(elements)
                    .chain(rest)
                    .collect::<Vec<_>>()
                {
                    self.define_pattern(child);
                }
            }
            ArenaPatternKind::Alternation(patterns)
            | ArenaPatternKind::Tuple(patterns)
            | ArenaPatternKind::Text(patterns) => {
                for pattern in self.arena().pattern_ids(patterns).collect::<Vec<_>>() {
                    self.define_pattern(pattern);
                }
            }
            ArenaPatternKind::TextHole {
                binding: Some(name),
                ..
            } => self.define(name),
            ArenaPatternKind::Constructor { arg: Some(arg), .. } => self.define_pattern(arg),
            ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in self.arena().pattern_fields(fields).to_vec() {
                    self.define_pattern(field.pattern);
                }
            }
            ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Wildcard
            | ArenaPatternKind::Type { binding: None, .. }
            | ArenaPatternKind::TextHole { binding: None, .. }
            | ArenaPatternKind::Constructor { arg: None, .. }
            | ArenaPatternKind::Facet(_)
            | ArenaPatternKind::TestName { .. } => {}
        }
    }

    fn add_direct_callee(&mut self, callee: ExprId) -> Option<usize> {
        let target = match self.arena().expr(callee).kind {
            // `Checker::check_call_arena` resolves declared callables before
            // ordinary bindings. Match that resolution order here; scopes only
            // matter when an identity is used as a value rather than called.
            ArenaExprKind::Ident(name) => self.analysis.resolve_unqualified(self.namespace, name),
            ArenaExprKind::Field { base, name } => match self.arena().expr(base).kind {
                ArenaExprKind::Ident(alias) => {
                    self.analysis.resolve_qualified(self.namespace, alias, name)
                }
                _ => None,
            },
            ArenaExprKind::NullSafeField { .. } => None,
            _ => None,
        };
        if let Some(target) = target {
            self.edges.direct.insert(target);
        }
        target
    }

    fn add_direct_unqualified(&mut self, name: Name) {
        if let Some(target) = self.analysis.resolve_unqualified(self.namespace, name) {
            self.edges.direct.insert(target);
        }
    }

    fn add_dynamic_unqualified(&mut self, name: Name) {
        if let Some(target) = self.resolve_unqualified(name) {
            self.edges.dynamic.insert(target);
        }
    }

    fn add_dynamic_qualified_field(&mut self, base: ExprId, name: Name) -> Option<usize> {
        let ArenaExprKind::Ident(alias) = self.arena().expr(base).kind else {
            return None;
        };
        let target = self.resolve_qualified(alias, name)?;
        self.edges.dynamic.insert(target);
        Some(target)
    }

    fn resolve_unqualified(&self, name: Name) -> Option<usize> {
        if self.is_bound(name) {
            return None;
        }
        self.analysis.resolve_unqualified(self.namespace, name)
    }

    fn resolve_qualified(&self, alias: Name, name: Name) -> Option<usize> {
        if self.is_bound(alias) {
            return None;
        }
        self.analysis.resolve_qualified(self.namespace, alias, name)
    }

    fn push_scope(&mut self) {
        self.scopes.push(FxHashSet::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop().expect("scanner scope underflow");
    }

    fn define(&mut self, name: Name) {
        self.scopes
            .last_mut()
            .expect("scanner always has a scope")
            .insert(name);
    }

    fn is_bound(&self, name: Name) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(&name))
    }
}
